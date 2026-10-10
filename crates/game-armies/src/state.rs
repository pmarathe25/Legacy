use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct KnowledgePatch {
    actor: PersonId,
    before: Option<ArmyKnowledge>,
    after: ArmyKnowledge,
}

/// One army and its affected personal knowledge, with a stale-state guard.
/// The coordinator preflights requests with their actual owners before commit.
#[derive(Clone, Debug)]
pub struct PreparedChange {
    id: ArmyId,
    before: Option<Army>,
    after: Army,
    knowledge: Vec<KnowledgePatch>,
    claims: BTreeMap<MissionId, Option<ArmyId>>,
    pub effects: Vec<Effect>,
}
impl PreparedChange {
    pub fn check(&self, world: &World) -> Result<(), Error> {
        if world.armies.get(&self.id) != self.before.as_ref()
            || self
                .knowledge
                .iter()
                .any(|p| world.view(p.actor, self.id) != p.before.as_ref())
            || self
                .claims
                .iter()
                .any(|(m, a)| world.commanding_army(*m) != *a)
            || self.after.orders.keys().any(|id| {
                self.before
                    .as_ref()
                    .is_none_or(|a| !a.orders.contains_key(id))
                    && world.armies.values().any(|a| a.orders.contains_key(id))
            })
            || self.after.reports.keys().any(|id| {
                self.before
                    .as_ref()
                    .is_none_or(|a| !a.reports.contains_key(id))
                    && world.armies.values().any(|a| a.reports.contains_key(id))
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn commit(self, world: &mut World) -> Result<Vec<Effect>, Error> {
        self.check(world)?;
        world.armies.insert(self.id, self.after);
        for patch in self.knowledge {
            world
                .known
                .entry(patch.actor)
                .or_default()
                .insert(self.id, patch.after);
        }
        Ok(self.effects)
    }
    fn knowledge<'a>(&'a mut self, world: &World, actor: PersonId) -> &'a mut ArmyKnowledge {
        let index = if let Some(index) = self.knowledge.iter().position(|p| p.actor == actor) {
            index
        } else {
            let before = world.view(actor, self.id).cloned();
            self.knowledge.push(KnowledgePatch {
                actor,
                after: before.clone().unwrap_or_default(),
                before,
            });
            self.knowledge.len() - 1
        };
        &mut self.knowledge[index].after
    }
}
impl World {
    pub fn view(&self, actor: PersonId, army: ArmyId) -> Option<&ArmyKnowledge> {
        self.known.get(&actor)?.get(&army)
    }
    pub fn views(&self, actor: PersonId) -> Vec<ArmyView> {
        self.known
            .get(&actor)
            .into_iter()
            .flat_map(|armies| armies.iter())
            .map(|(id, k)| ArmyView {
                id: *id,
                knowledge: k.clone(),
                local_engagements: Vec::new(),
            })
            .collect()
    }
    pub fn commanding_army(&self, mission: MissionId) -> Option<ArmyId> {
        self.armies
            .values()
            .find(|a| {
                a.disbanded_at.is_none()
                    && a.detachments
                        .get(&mission)
                        .is_some_and(|d| d.participation.active())
            })
            .map(|a| a.id)
    }
    fn edit(&self, id: ArmyId) -> Result<PreparedChange, Error> {
        let before = self.armies.get(&id).ok_or(Error::Unavailable)?.clone();
        Ok(PreparedChange {
            id,
            before: Some(before.clone()),
            after: before,
            knowledge: Vec::new(),
            claims: BTreeMap::new(),
            effects: Vec::new(),
        })
    }
    pub fn prepare_form(
        &self,
        id: ArmyId,
        commander: PersonId,
        missions: &BTreeSet<MissionId>,
        facts: &BTreeMap<MissionId, DetachmentFacts>,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        if id.0 == 0
            || commander.0 == 0
            || self.armies.contains_key(&id)
            || missions.is_empty()
            || missions.len() > MAX_DETACHMENTS
        {
            return Err(Error::Unavailable);
        }
        let mut members = BTreeSet::new();
        let mut detachments = BTreeMap::new();
        for id in missions {
            let f = facts.get(id).ok_or(Error::Unavailable)?;
            if f.mission != *id
                || !f.available()
                || !members.is_disjoint(&f.members)
                || self.commanding_army(*id).is_some()
            {
                return Err(Error::Unavailable);
            }
            members.extend(&f.members);
            detachments.insert(
                *id,
                Detachment {
                    mission: *id,
                    leader: f.leader,
                    participation: Participation::Offered,
                    invited_at: now,
                    heard_at: None,
                    changed_at: now,
                    explanation: "Awaiting the leader's own decision".into(),
                },
            );
        }
        let army = Army {
            id,
            commander,
            formed_at: now,
            disbanded_at: None,
            detachments,
            orders: BTreeMap::new(),
            reports: BTreeMap::new(),
        };
        let known = ArmyKnowledge {
            commander: Some(observed(commander, commander, now)),
            detachments: army
                .detachments
                .iter()
                .map(|(m, d)| (*m, observed(d.clone(), commander, now)))
                .collect(),
            ..ArmyKnowledge::default()
        };
        Ok(PreparedChange {
            id,
            before: None,
            after: army,
            knowledge: vec![KnowledgePatch {
                actor: commander,
                before: self.view(commander, id).cloned(),
                after: known,
            }],
            claims: missions
                .iter()
                .map(|m| (*m, self.commanding_army(*m)))
                .collect(),
            effects: Vec::new(),
        })
    }
    pub fn prepare_hear_invitation(
        &self,
        id: ArmyId,
        mission: MissionId,
        receipt: Communication,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let commander = plan.after.commander;
        let d = plan
            .after
            .detachments
            .get_mut(&mission)
            .ok_or(Error::Unavailable)?;
        if plan.after.disbanded_at.is_some()
            || d.heard_at.is_some()
            || d.participation != Participation::Offered
            || !receipt.valid(commander, d.leader, d.invited_at, now)
        {
            return Err(Error::Unavailable);
        }
        d.heard_at = Some(now);
        let statement = reported(d.clone(), commander, now, now);
        let k = plan.knowledge(self, receipt.listener);
        k.commander = Some(reported(commander, commander, now, now));
        k.detachments.insert(mission, statement);
        Ok(plan)
    }
    /// A commander leading a detachment can personally consider their own
    /// invitation without inventing a conversation with themselves.
    pub fn prepare_consider_own_invitation(
        &self,
        id: ArmyId,
        mission: MissionId,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get_mut(&mission)
            .ok_or(Error::Unavailable)?;
        if plan.after.commander != actor
            || d.leader != actor
            || d.heard_at.is_some()
            || plan.after.disbanded_at.is_some()
            || now < d.invited_at
        {
            return Err(Error::Unavailable);
        }
        d.heard_at = Some(now);
        let statement = observed(d.clone(), actor, now);
        plan.knowledge(self, actor)
            .detachments
            .insert(mission, statement);
        Ok(plan)
    }
    pub fn prepare_respond(
        &self,
        decision: ParticipationDecision,
        facts: &DetachmentFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let ParticipationDecision {
            army: id,
            mission,
            actor,
            accept,
            explanation,
        } = decision;
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get_mut(&mission)
            .ok_or(Error::Unavailable)?;
        if !explanation_valid(&explanation)
            || plan.after.disbanded_at.is_some()
            || d.leader != actor
            || d.participation != Participation::Offered
            || d.heard_at.is_none_or(|at| at > now)
            || facts.mission != mission
            || facts.leader != actor
            || !facts.available()
            || self
                .view(actor, id)
                .and_then(|k| k.detachments.get(&mission))
                .is_none()
            || self.commanding_army(mission).is_some()
        {
            return Err(Error::Unavailable);
        }
        plan.claims.insert(mission, None);
        d.participation = if accept {
            Participation::Accepted
        } else {
            Participation::Refused
        };
        d.changed_at = now;
        d.explanation = explanation;
        let statement = observed(d.clone(), actor, now);
        plan.knowledge(self, actor)
            .detachments
            .insert(mission, statement);
        Ok(plan)
    }
    /// Reporting a decision does not refresh other detachments or private plans.
    pub fn prepare_hear_participation(
        &self,
        id: ArmyId,
        mission: MissionId,
        receipt: Communication,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get(&mission)
            .ok_or(Error::Unavailable)?;
        let sender = self
            .view(d.leader, id)
            .and_then(|k| k.detachments.get(&mission))
            .ok_or(Error::Unavailable)?;
        if !receipt.valid(d.leader, plan.after.commander, sender.learned_at, now) {
            return Err(Error::Unavailable);
        }
        let incoming = reported(sender.value.clone(), d.leader, sender.observed_at, now);
        merge(
            plan.knowledge(self, receipt.listener)
                .detachments
                .entry(mission),
            incoming,
        );
        Ok(plan)
    }
    /// Deliver the sender's frozen personal statement. A later withdrawal or
    /// termination does not rewrite an earlier message already in transit.
    pub fn prepare_receive_participation(
        &self,
        id: ArmyId,
        mission: MissionId,
        statement: &Dated<Detachment>,
        receipt: Communication,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get(&mission)
            .ok_or(Error::Unavailable)?;
        if statement.value.mission != mission
            || statement.value.leader != d.leader
            || statement.value.invited_at != d.invited_at
            || statement.value.participation == Participation::Offered
            || statement.value.changed_at > statement.observed_at
            || statement
                .value
                .heard_at
                .is_none_or(|at| at > statement.value.changed_at || at < statement.value.invited_at)
            || statement.value.explanation.is_empty()
            || statement.value.explanation.len() > 512
            || statement.observed_at > statement.learned_at
            || statement.learned_at > receipt.started_at
            || statement.source != KnowledgeSource::Observed(receipt.speaker)
            || !receipt.valid(d.leader, plan.after.commander, statement.learned_at, now)
        {
            return Err(Error::Unavailable);
        }
        let incoming = reported(
            statement.value.clone(),
            receipt.speaker,
            statement.observed_at,
            now,
        );
        merge(
            plan.knowledge(self, receipt.listener)
                .detachments
                .entry(mission),
            incoming,
        );
        Ok(plan)
    }
    pub fn prepare_order(
        &self,
        proposal: OrderProposal,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let OrderProposal {
            army: id,
            id: order,
            commander: actor,
            missions,
            objective,
            expires,
        } = proposal;
        let mut plan = self.edit(id)?;
        if actor != plan.after.commander
            || plan.after.disbanded_at.is_some()
            || now < plan.after.formed_at
            || order.0 == 0
            || expires <= now
            || missions.is_empty()
            || missions.len() > MAX_DETACHMENTS
            || self.armies.values().any(|a| a.orders.contains_key(&order))
        {
            return Err(Error::Unavailable);
        }
        let mut recipients = BTreeMap::new();
        for mission in &missions {
            let d = plan
                .after
                .detachments
                .get(mission)
                .ok_or(Error::Unavailable)?;
            // Execution checks actual accepted authority; intention checks the
            // commander's own dated relationship, never an unseen response.
            if !d.participation.active()
                || now < d.changed_at
                || self
                    .view(actor, id)
                    .and_then(|k| k.detachments.get(mission))
                    .is_none_or(|d| !d.value.participation.active())
            {
                return Err(Error::Unavailable);
            }
            recipients.insert(
                *mission,
                OrderRecipient {
                    leader: d.leader,
                    status: OrderStatus::AwaitingDelivery,
                    received_at: None,
                    responded_at: None,
                    explanation: String::new(),
                },
            );
        }
        let order = Order {
            id: order,
            commander: actor,
            objective,
            issued_at: now,
            expires,
            recipients,
        };
        plan.after.orders.insert(order.id, order.clone());
        plan.knowledge(self, actor)
            .orders
            .insert(order.id, observed(order, actor, now));
        Ok(plan)
    }
    pub fn prepare_hear_order(
        &self,
        id: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
        receipt: Communication,
        facts: &DetachmentFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get(&mission)
            .ok_or(Error::Unavailable)?;
        let o = plan
            .after
            .orders
            .get_mut(&order)
            .ok_or(Error::Unavailable)?;
        let r = o.recipients.get_mut(&mission).ok_or(Error::Unavailable)?;
        if plan.after.disbanded_at.is_some()
            || !d.participation.active()
            || now >= o.expires
            || r.status != OrderStatus::AwaitingDelivery
            || !receipt.valid(o.commander, r.leader, o.issued_at, now)
            || facts.mission != mission
            || facts.leader != r.leader
            || !facts.available()
        {
            return Err(Error::Unavailable);
        }
        self.receive_order(plan, order, mission, receipt.listener, receipt.speaker, now)
    }
    pub fn prepare_consider_own_order(
        &self,
        id: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
        actor: PersonId,
        facts: &DetachmentFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get(&mission)
            .ok_or(Error::Unavailable)?;
        let o = plan.after.orders.get(&order).ok_or(Error::Unavailable)?;
        let r = o.recipients.get(&mission).ok_or(Error::Unavailable)?;
        if plan.after.disbanded_at.is_some()
            || plan.after.commander != actor
            || d.leader != actor
            || !d.participation.active()
            || now < o.issued_at
            || now >= o.expires
            || r.status != OrderStatus::AwaitingDelivery
            || facts.mission != mission
            || facts.leader != actor
            || !facts.available()
        {
            return Err(Error::Unavailable);
        }
        self.receive_order(plan, order, mission, actor, actor, now)
    }
    fn receive_order(
        &self,
        mut plan: PreparedChange,
        order: ArmyOrderId,
        mission: MissionId,
        listener: PersonId,
        speaker: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let key = {
            let o = &plan.after.orders[&order];
            (o.issued_at, o.id)
        };
        let stale = plan.after.orders.values().any(|older| {
            older.id != order
                && (older.issued_at, older.id) > key
                && older
                    .recipients
                    .get(&mission)
                    .is_some_and(|r| r.received_at.is_some())
        });
        let incoming = {
            let o = plan.after.orders.get_mut(&order).unwrap();
            let r = o.recipients.get_mut(&mission).unwrap();
            r.status = if stale {
                OrderStatus::Superseded
            } else {
                OrderStatus::Received
            };
            r.received_at = Some(now);
            let mut incoming = o.clone();
            incoming.recipients.retain(|m, _| *m == mission);
            incoming
        };
        let mut superseded = Vec::new();
        if !stale {
            for older in plan.after.orders.values_mut() {
                if (older.issued_at, older.id) < key
                    && let Some(r) = older.recipients.get_mut(&mission)
                    && r.status == OrderStatus::Received
                {
                    r.status = OrderStatus::Superseded;
                    let mut view = older.clone();
                    view.recipients.retain(|m, _| *m == mission);
                    superseded.push((older.id, view));
                }
            }
        }
        let k = plan.knowledge(self, listener);
        k.orders
            .insert(order, reported(incoming, speaker, now, now));
        for (id, view) in superseded {
            k.orders.insert(id, observed(view, listener, now));
        }
        Ok(plan)
    }
    pub fn prepare_respond_order(
        &self,
        decision: OrderDecision,
        facts: &DetachmentFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let OrderDecision {
            army: id,
            order,
            mission,
            actor,
            response,
            explanation,
        } = decision;
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get_mut(&mission)
            .ok_or(Error::Unavailable)?;
        let o = plan
            .after
            .orders
            .get_mut(&order)
            .ok_or(Error::Unavailable)?;
        let r = o.recipients.get_mut(&mission).ok_or(Error::Unavailable)?;
        if !explanation_valid(&explanation)
            || plan.after.disbanded_at.is_some()
            || !d.participation.active()
            || r.leader != actor
            || r.status != OrderStatus::Received
            || now >= o.expires
            || r.received_at.is_none_or(|at| at > now)
            || facts.mission != mission
            || facts.leader != actor
            || !facts.available()
            || self
                .view(actor, id)
                .and_then(|k| k.orders.get(&order))
                .is_none_or(|o| {
                    o.value
                        .recipients
                        .get(&mission)
                        .is_none_or(|r| r.status != OrderStatus::Received)
                })
        {
            return Err(Error::Unavailable);
        }
        r.status = match response {
            OrderResponse::Accept => OrderStatus::Accepted,
            OrderResponse::Refuse => OrderStatus::Refused,
            OrderResponse::Retreat => OrderStatus::Retreated,
            OrderResponse::Defect => OrderStatus::Defected,
        };
        r.responded_at = Some(now);
        r.explanation = explanation;
        match response {
            OrderResponse::Accept => plan.effects.push(Effect::OfferObjective {
                mission,
                leader: actor,
                objective: o.objective.clone(),
            }),
            OrderResponse::Refuse => {}
            OrderResponse::Retreat | OrderResponse::Defect => {
                let reason = if response == OrderResponse::Retreat {
                    Departure::Retreat
                } else {
                    Departure::Defect
                };
                d.participation = reason.participation();
                d.changed_at = now;
                d.explanation = r.explanation.clone();
                plan.effects.push(Effect::ReleaseCommand {
                    mission,
                    leader: actor,
                    reason,
                });
                if reason == Departure::Retreat {
                    plan.effects.push(Effect::OfferRetreat {
                        mission,
                        leader: actor,
                    });
                }
            }
        }
        let response_statement = observed(r.clone(), actor, now);
        let mut statement = o.clone();
        statement.recipients.retain(|m, _| *m == mission);
        let detachment = d.clone();
        let k = plan.knowledge(self, actor);
        k.orders.insert(order, observed(statement, actor, now));
        k.responses
            .entry(order)
            .or_default()
            .insert(mission, response_statement);
        k.detachments
            .insert(mission, observed(detachment, actor, now));
        Ok(plan)
    }
    pub fn prepare_hear_order_response(
        &self,
        id: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
        receipt: Communication,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let sender = self
            .view(receipt.speaker, id)
            .and_then(|k| k.orders.get(&order))
            .ok_or(Error::Unavailable)?;
        let response = sender
            .value
            .recipients
            .get(&mission)
            .ok_or(Error::Unavailable)?;
        if response.responded_at.is_none()
            || response.leader != receipt.speaker
            || !receipt.valid(
                receipt.speaker,
                plan.after.commander,
                sender.learned_at,
                now,
            )
        {
            return Err(Error::Unavailable);
        }
        if self
            .view(receipt.listener, id)
            .and_then(|k| k.orders.get(&order))
            .is_none_or(|o| {
                o.value
                    .recipients
                    .get(&mission)
                    .is_none_or(|r| r.leader != receipt.speaker)
            })
        {
            return Err(Error::Unavailable);
        }
        let incoming = reported(
            response.clone(),
            receipt.speaker,
            response.responded_at.unwrap(),
            now,
        );
        merge(
            plan.knowledge(self, receipt.listener)
                .responses
                .entry(order)
                .or_default()
                .entry(mission),
            incoming,
        );
        Ok(plan)
    }
    pub fn prepare_leave(
        &self,
        id: ArmyId,
        mission: MissionId,
        actor: PersonId,
        reason: Departure,
        explanation: String,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get_mut(&mission)
            .ok_or(Error::Unavailable)?;
        if !explanation_valid(&explanation)
            || plan.after.disbanded_at.is_some()
            || d.leader != actor
            || !d.participation.active()
            || now < d.changed_at
        {
            return Err(Error::Unavailable);
        }
        d.participation = reason.participation();
        d.changed_at = now;
        d.explanation = explanation;
        let incoming = observed(d.clone(), actor, now);
        plan.knowledge(self, actor)
            .detachments
            .insert(mission, incoming);
        plan.effects.push(Effect::ReleaseCommand {
            mission,
            leader: actor,
            reason,
        });
        if reason == Departure::Retreat {
            plan.effects.push(Effect::OfferRetreat {
                mission,
                leader: actor,
            });
        }
        Ok(plan)
    }
    pub fn prepare_disband(
        &self,
        id: ArmyId,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        if plan.after.commander != actor
            || plan.after.disbanded_at.is_some()
            || now < plan.after.formed_at
        {
            return Err(Error::Unavailable);
        }
        plan.after.disbanded_at = Some(now);
        plan.knowledge(self, actor).disbanded_at = Some(observed(now, actor, now));
        // Ending a relationship grants no power to cancel an absent person's
        // native mission, supplies, agreement or accepted objective.
        Ok(plan)
    }
    pub fn prepare_expire(&self, id: ArmyId, now: SimTime) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        for o in plan.after.orders.values_mut() {
            if now >= o.expires {
                for r in o.recipients.values_mut() {
                    if matches!(
                        r.status,
                        OrderStatus::AwaitingDelivery | OrderStatus::Received
                    ) {
                        r.status = OrderStatus::Expired;
                    }
                }
            }
        }
        // Expiration does not tell distant parties anything or undo work.
        Ok(plan)
    }
    /// The mission owner reports its actual terminal lifecycle. This releases
    /// command authority without publishing distant results to any person.
    pub fn prepare_native_termination(
        &self,
        id: ArmyId,
        facts: &DetachmentFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get_mut(&facts.mission)
            .ok_or(Error::Unavailable)?;
        if facts.leader != d.leader
            || now < d.changed_at
            || !matches!(
                facts.status,
                game_missions::MissionStatus::Completed
                    | game_missions::MissionStatus::Failed
                    | game_missions::MissionStatus::Cancelled
            )
        {
            return Err(Error::Unavailable);
        }
        if d.participation.active() {
            d.participation = Participation::Withdrawn;
            d.changed_at = now;
            d.explanation = "The native detachment mission has ended".into();
        }
        Ok(plan)
    }
    pub fn next_boundary(&self, now: SimTime) -> Option<SimTime> {
        self.armies
            .values()
            .filter(|a| a.disbanded_at.is_none())
            .flat_map(|a| a.orders.values())
            .filter(|o| {
                o.expires > now
                    && o.recipients.values().any(|r| {
                        matches!(
                            r.status,
                            OrderStatus::AwaitingDelivery | OrderStatus::Received
                        )
                    })
            })
            .map(|o| o.expires)
            .min()
    }
    pub fn prepare_report(
        &self,
        id: ArmyId,
        report: ArmyReport,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let d = plan
            .after
            .detachments
            .get(&report.mission)
            .ok_or(Error::Unavailable)?;
        if report.id.0 == 0
            || report.army != id
            || report.speaker != d.leader
            || report.sent_at != now
            || now < plan.after.formed_at
            || report.snapshot.id != report.mission
            || report.snapshot.leader != report.speaker
            || report.snapshot.observed_at > now
            || report
                .snapshot
                .companions
                .iter()
                .any(|c| c.observed_at > now)
            || self
                .armies
                .values()
                .any(|a| a.reports.contains_key(&report.id))
        {
            return Err(Error::Unavailable);
        }
        let speaker = report.speaker;
        plan.after.reports.insert(report.id, report.clone());
        plan.knowledge(self, speaker)
            .reports
            .insert(report.id, observed(report, speaker, now));
        Ok(plan)
    }
    pub fn prepare_hear_report(
        &self,
        id: ArmyId,
        report: ArmyReportId,
        receipt: Communication,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let packet = plan.after.reports.get(&report).ok_or(Error::Unavailable)?;
        if !receipt.valid(packet.speaker, receipt.listener, packet.sent_at, now)
            || self
                .view(packet.speaker, id)
                .and_then(|k| k.reports.get(&report))
                .is_none()
        {
            return Err(Error::Unavailable);
        }
        let incoming = reported(
            packet.clone(),
            packet.speaker,
            packet.snapshot.observed_at,
            now,
        );
        merge(
            plan.knowledge(self, receipt.listener).reports.entry(report),
            incoming,
        );
        Ok(plan)
    }
}

fn observed<T>(value: T, actor: PersonId, now: SimTime) -> Dated<T> {
    Dated {
        value,
        observed_at: now,
        learned_at: now,
        source: KnowledgeSource::Observed(actor),
    }
}
fn reported<T>(value: T, speaker: PersonId, at: SimTime, now: SimTime) -> Dated<T> {
    Dated {
        value,
        observed_at: at,
        learned_at: now,
        source: KnowledgeSource::Reported(speaker),
    }
}
fn merge<K: Ord, T>(
    entry: std::collections::btree_map::Entry<'_, K, Dated<T>>,
    incoming: Dated<T>,
) {
    match entry {
        std::collections::btree_map::Entry::Vacant(v) => {
            v.insert(incoming);
        }
        std::collections::btree_map::Entry::Occupied(mut o) => {
            if o.get().observed_at < incoming.observed_at {
                o.insert(incoming);
            }
        }
    }
}
fn explanation_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512
}
