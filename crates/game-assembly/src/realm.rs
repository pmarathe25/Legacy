//! Regional commitments coordinate existing institutions and owners. This
//! adapter applies no substitute inventories, offices, votes or combat result.
use crate::*;
use game_realm::{
    Action, Backing, LocalWitness, OutcomeReceipt, Progress, Proposal, ProposalFacts, Ratification,
    Request, Term, TermKey, WorkReceipt,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub talk_time: SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self { talk_time: MINUTE }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.talk_time.0 == 0 {
            Err(Error::InvalidConfiguration(
                "realm talk time must be positive".into(),
            ))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Talk {
    pub recipient: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub report: game_realm::Report,
    pub institutions: Vec<InstitutionView>,
    pub stocks: Vec<StockKnowledge>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingOwner {
    Agreement(AgreementId),
    Mission(MissionId),
    Passage(RuleProposalId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWork {
    pub actor: PersonId,
    pub owner: PendingOwner,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub kernel: game_realm::World,
    pub rules: Rules,
    pub talks: BTreeMap<PersonId, Talk>,
    #[serde(with = "pending_entries")]
    pub pending: BTreeMap<TermKey, PendingWork>,
    pub reconsider_after: BTreeMap<PersonId, SimTime>,
    pub last_event: u64,
}
mod pending_entries {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        value: &BTreeMap<TermKey, PendingWork>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<TermKey, PendingWork>, D::Error> {
        let entries = Vec::<(TermKey, PendingWork)>::deserialize(deserializer)?;
        let mut result = BTreeMap::new();
        for (key, value) in entries {
            if result.insert(key, value).is_some() {
                return Err(serde::de::Error::custom("duplicate regional work key"));
            }
        }
        Ok(result)
    }
}
impl Simulation {
    pub fn realm_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::enforcement_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "realm-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let a = sim.host.state.ruler;
        let av = sim.host.state.people[3].id;
        let b = sim.host.state.people[4].id;
        let bv = sim.host.state.people[5].id;
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
            sim.interrupt_actor_activity(actor);
        }
        for actor in [b, bv] {
            *game_spatial::location_mut(
                &mut sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0),
            )
            .expect("location exists") = Location::AtArea(AreaId(3));
        }
        let food_a = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(6_000),
            Quantity(6_000),
            BTreeSet::from([a]),
        )?;
        let timber_a = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(6_000),
            BTreeSet::from([a]),
        )?;
        let food_b = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(0),
            Quantity(6_000),
            BTreeSet::from([b]),
        )?;
        let timber_b = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(3)),
            Quantity(6_000),
            Quantity(6_000),
            BTreeSet::from([b]),
        )?;
        for (id, holder) in [(food_a, a), (timber_a, a), (food_b, b), (timber_b, b)] {
            sim.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(id, holder);
        }
        for (area, holder, voter, stores, name) in [
            (
                AreaId(1),
                a,
                av,
                BTreeSet::from([food_a, timber_a]),
                "Willowbank trading council",
            ),
            (
                AreaId(3),
                b,
                bv,
                BTreeSet::from([food_b, timber_b]),
                "Northwood trading council",
            ),
        ] {
            sim.debug_institution_action(
                holder,
                InstitutionAction::Found {
                    charter: InstitutionCharter {
                        purpose: name.into(),
                        jurisdiction: area,
                        founders: BTreeSet::from([holder, voter]),
                        open_membership: true,
                        procedure: DecisionProcedure {
                            proposers: Subjects::members(),
                            voters: Subjects::members(),
                            minimum_support: 2,
                        },
                        offices: vec![OfficeMandate {
                            slot: 0,
                            holder,
                            inventories: stores,
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([
                                OfficePower::Negotiate,
                                OfficePower::Delivery,
                                OfficePower::Access,
                                OfficePower::Enforce,
                            ]),
                            minimum_support: 2,
                        }],
                        rules: Vec::new(),
                        contributions: Vec::new(),
                        evidence: BTreeSet::new(),
                    },
                    origin: None,
                },
            )?;
            let institution = *sim
                .host
                .state
                .institutions
                .as_ref()
                .unwrap()
                .institutions
                .keys()
                .next_back()
                .unwrap();
            sim.debug_institution_action(voter, InstitutionAction::Join { institution })?;
        }
        sim.initialize_realm(Rules::default())?;
        // These fixture delegates await the scripted negotiating choices. Their
        // ordinary biological needs, voting, and real agreement tasks continue.
        sim.host.state.realm.as_mut().unwrap().reconsider_after = sim
            .person_index
            .keys()
            .map(|id| (*id, SimTime(86_400_000)))
            .collect();
        sim.review_decisions()?;
        Ok(sim)
    }
    pub fn initialize_realm(&mut self, rules: Rules) -> Result<(), Error> {
        rules.validate()?;
        if self.host.state.realm.is_some()
            || self.host.state.institutions.is_none()
            || self.host.state.agreements.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.host.state.realm = Some(World {
            rules,
            last_event: self.host.next_event() - 1,
            ..World::default()
        });
        Ok(())
    }
    fn realm_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.in_combat(actor)
        {
            return None;
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn realm_facts(
        &self,
        actor: PersonId,
        proposal: &Proposal,
        fresh_local: bool,
    ) -> Result<ProposalFacts, Error> {
        let mind = self
            .host
            .state
            .decisions
            .as_ref()
            .and_then(|_| {
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            })
            .ok_or(Error::Unavailable)?;
        let cached = self.institution_view(actor);
        let mut facts = ProposalFacts {
            known_people: BTreeSet::from([actor]),
            known_areas: mind
                .routes
                .keys()
                .copied()
                .chain(mind.routes.values().flat_map(|links| links.keys().copied()))
                .collect(),
            ..ProposalFacts::default()
        };
        for (id, k) in &mind.knowledge {
            let capacity = game_cargo::inventory(&self.host.storage, *(id))
                .filter(|i| {
                    self.inventory_area(i) == self.realm_area(actor)
                        && k.observed_at == self.host.clock().time
                })
                .map(|i| i.capacity);
            facts.inventories.insert(
                *id,
                game_realm::InventoryFacts {
                    good: k.good,
                    capacity,
                },
            );
        }
        for p in proposal.participants.values() {
            let known = cached
                .iter()
                .find(|i| i.id == p.institution)
                .ok_or(Error::Unavailable)?;
            facts.known_people.extend(known.members.iter().copied());
            let office = known
                .offices
                .iter()
                .find(|o| {
                    o.mandate.holder == p.representative
                        && o.effective
                        && o.mandate.powers.contains(&OfficePower::Negotiate)
                })
                .ok_or(Error::Unavailable)?;
            let mut mandate = office.mandate.clone();
            let mut authorized = known.status == InstitutionStatus::Active;
            if fresh_local && p.representative == actor {
                let actual = self
                    .host
                    .state
                    .institutions
                    .as_ref()
                    .unwrap()
                    .institutions
                    .get(&p.institution)
                    .ok_or(Error::Unavailable)?;
                mandate = actual
                    .charter
                    .offices
                    .iter()
                    .find(|o| o.slot == mandate.slot && o.holder == actor)
                    .ok_or(Error::Unavailable)?
                    .clone();
                authorized = actual.charter.jurisdiction
                    == self.realm_area(actor).ok_or(Error::Unavailable)?
                    && self.office_effective(actual, &mandate)
                    && mandate.powers.contains(&OfficePower::Negotiate);
            }
            // An inventory scope grants no invented monetary balance or
            // compartment-sized lifetime spending limit. Owners validate each
            // actual promise and physical use; future production can be pledged.
            let delivery_caps = if mandate.powers.contains(&OfficePower::Delivery) {
                mandate
                    .inventories
                    .iter()
                    .map(|id| (*id, Quantity(u64::MAX)))
                    .collect()
            } else {
                BTreeMap::new()
            };
            let supply_caps = mandate
                .inventories
                .iter()
                .map(|id| (*id, Quantity(u64::MAX)))
                .collect();
            facts.representatives.insert(
                p.institution,
                game_realm::RepresentativeFacts {
                    settlement: known.charter.jurisdiction,
                    representative: p.representative,
                    may_negotiate: authorized,
                    delivery_caps,
                    supply_caps,
                    passage_areas: BTreeSet::from([known.charter.jurisdiction]),
                    protection_areas: facts.known_areas.clone(),
                },
            );
        }
        facts.known_people.extend(
            self.person_index
                .keys()
                .copied()
                .filter(|p| self.realm_area(*p) == self.realm_area(actor))
                .take(64),
        );
        Ok(facts)
    }
    pub(crate) fn realm_view(&self, actor: PersonId) -> game_realm::View {
        game_realm::View {
            commitments: self
                .host
                .state
                .realm
                .as_ref()
                .and_then(|w| w.kernel.known.get(&actor))
                .map_or_else(Vec::new, |known| {
                    known
                        .iter()
                        .rev()
                        .take(64)
                        .map(|(id, k)| game_realm::KnownCommitment {
                            id: *id,
                            knowledge: k.clone(),
                        })
                        .collect()
                }),
        }
    }
    pub(crate) fn realm_command(&mut self, actor: PersonId, action: &Action) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let area = self.realm_area(actor).ok_or(Error::Unavailable)?;
        let world = self.host.state.realm.as_ref().ok_or(Error::Unavailable)?;
        match action {
            Action::Propose { proposal } => {
                if proposal.proposer != actor {
                    return Err(Error::Unavailable);
                }
                let facts = self.realm_facts(actor, proposal, true)?;
                self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
                self.ensure_event_capacity(1)?;
                let id = RealmCommitmentId(self.host.next_id());
                let prepared = world.kernel.prepare_propose(
                    id,
                    proposal.clone(),
                    &facts,
                    self.host.clock().time,
                )?;
                let allocated = self.host.allocate_id()?.0;
                debug_assert_eq!(allocated, id.0);
                prepared.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
                self.emit(EventKind::RealmProposed {
                    commitment: id,
                    actor,
                    revision: 0,
                });
                self.realm_publish_proposal(id, 0, area)?;
            }
            Action::Revise {
                commitment,
                proposal,
            } => {
                if proposal.proposer != actor {
                    return Err(Error::Unavailable);
                }
                let facts = self.realm_facts(actor, proposal, true)?;
                let prepared = world.kernel.prepare_revise(
                    *commitment,
                    proposal.clone(),
                    &facts,
                    self.host.clock().time,
                )?;
                self.ensure_event_capacity(1)?;
                prepared.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
                let revision = self.host.state.realm.as_ref().unwrap().kernel.commitments
                    [commitment]
                    .current()
                    .number;
                self.emit(EventKind::RealmProposed {
                    commitment: *commitment,
                    actor,
                    revision,
                });
                self.realm_publish_proposal(*commitment, revision, area)?;
            }
            Action::Report {
                commitment,
                recipient,
            } => {
                if *recipient == actor
                    || self.realm_area(*recipient) != Some(area)
                    || !self.transport_same_place(actor, *recipient)
                    || !self.conversation_actor_available(*recipient)
                {
                    return Err(Error::Unavailable);
                }
                let report =
                    world
                        .kernel
                        .prepare_report(actor, *commitment, self.host.clock().time)?;
                let referenced = report
                    .snapshot
                    .revisions
                    .values()
                    .flat_map(|r| r.proposal.value.participants.keys().copied())
                    .collect::<BTreeSet<_>>();
                let stocks=game_characters::decisions::mind::<game_api::Command>(&self.host.storage,actor).unwrap().knowledge.values()
                    .filter(|k|report.snapshot.revisions.values().any(|r|r.proposal.value.terms.iter().any(|term|match term {
                        Term::Delivery {term:AgreementTerm::Delivery {source,destination,..},..}=>k.inventory==*source || k.inventory==*destination,
                        Term::Protection {patrol,..}=>patrol.supplies.iter().any(|s|s.source==k.inventory)
                            || patrol.compensation.values().flatten().any(|t|matches!(t,AgreementTerm::Delivery {source,destination,..} if k.inventory==*source || k.inventory==*destination)),
                        _=>false,
                    }))).take(64).cloned().collect();
                let institutions = self
                    .institution_view(actor)
                    .into_iter()
                    .filter(|i| referenced.contains(&i.id))
                    .collect();
                let completes = SimTime(
                    self.host
                        .clock()
                        .time
                        .0
                        .checked_add(world.rules.talk_time.0)
                        .ok_or(Error::Overflow)?,
                );
                self.ensure_event_capacity(1)?;
                self.interrupt_actor_activity(actor);
                self.host.state.realm.as_mut().unwrap().talks.insert(
                    actor,
                    Talk {
                        recipient: *recipient,
                        area,
                        completes,
                        report,
                        institutions,
                        stocks,
                    },
                );
                *self.host.storage.activity_mut(actor).unwrap() = Activity::RealmTalk {
                    commitment: *commitment,
                    recipient: *recipient,
                    completes,
                };
                self.host
                    .state
                    .due
                    .entry(completes)
                    .or_default()
                    .push(Due::Physical(actor));
            }
            Action::RequestWork { key } => self.realm_request_work(actor, *key)?,
        }
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn realm_publish_proposal(
        &mut self,
        id: RealmCommitmentId,
        revision: u32,
        area: AreaId,
    ) -> Result<(), Error> {
        let listeners = self
            .person_index
            .keys()
            .copied()
            .filter(|p| self.realm_area(*p) == Some(area))
            .collect::<Vec<_>>();
        for actor in listeners {
            let witness = LocalWitness {
                observer: actor,
                observer_area: area,
                statement_area: area,
                observed_at: self.host.clock().time,
            };
            let prepared = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_observe_proposal(witness, id, revision, self.host.clock().time)?;
            prepared.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        }
        Ok(())
    }
    pub(crate) fn complete_realm_talk(&mut self, actor: PersonId) {
        let owner_now = self.host.clock().time;
        let Some(talk) = self
            .host
            .state
            .realm
            .as_mut()
            .and_then(|w| w.talks.remove(&actor))
        else {
            return;
        };
        if self.realm_area(actor) == Some(talk.area)
            && self.realm_area(talk.recipient) == Some(talk.area)
            && self.transport_same_place(actor, talk.recipient)
            && matches!(
                *self
                    .host
                    .storage
                    .activity(talk.recipient)
                    .expect("body exists"),
                Activity::Idle | Activity::Rest
            )
        {
            if let Ok(prepared) = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_receive_report(talk.recipient, &talk.report, owner_now)
                && prepared
                    .commit(&mut self.host.state.realm.as_mut().unwrap().kernel)
                    .is_ok()
            {
                let memory = self
                    .host
                    .state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .entry(talk.recipient)
                    .or_default();
                for snapshot in talk.institutions {
                    if memory
                        .institutions
                        .get(&snapshot.id)
                        .is_none_or(|old| old.observed_at < snapshot.observed_at)
                    {
                        memory.institutions.insert(snapshot.id, snapshot);
                    }
                }
                let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut self.host.storage,
                    talk.recipient,
                )
                .unwrap();
                for mut fact in talk.stocks {
                    if mind
                        .knowledge
                        .get(&fact.inventory)
                        .is_none_or(|old| old.observed_at < fact.observed_at)
                    {
                        // A sender's access is not a grant to the recipient.
                        fact.accessible = mind
                            .knowledge
                            .get(&fact.inventory)
                            .is_some_and(|old| old.accessible);
                        fact.learned_at = owner_now;
                        fact.source = KnowledgeSource::Reported(actor);
                        mind.knowledge.insert(fact.inventory, fact);
                    }
                }
                self.emit(EventKind::RealmReported {
                    commitment: talk.report.commitment,
                    sender: actor,
                    recipient: talk.recipient,
                });
            } else {
                self.emit(EventKind::Interrupted { actor });
            }
        } else {
            self.emit(EventKind::Interrupted { actor });
        }
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
    }
    pub(crate) fn realm_actor_busy(&self, actor: PersonId) -> bool {
        self.host.state.realm.as_ref().is_some_and(|w| {
            w.talks
                .iter()
                .any(|(speaker, t)| *speaker == actor || t.recipient == actor)
        })
    }
    pub(crate) fn interrupt_realm(&mut self, actor: PersonId) {
        let speakers = self.host.state.realm.as_ref().map_or_else(Vec::new, |w| {
            w.talks
                .iter()
                .filter(|(speaker, t)| **speaker == actor || t.recipient == actor)
                .map(|(speaker, _)| *speaker)
                .collect()
        });
        for speaker in speakers {
            self.host
                .state
                .realm
                .as_mut()
                .unwrap()
                .talks
                .remove(&speaker);
            if matches!(
                *self.host.storage.activity(speaker).expect("body exists"),
                Activity::RealmTalk { .. }
            ) {
                *self.host.storage.activity_mut(speaker).unwrap() = Activity::Idle;
                for due in self.host.state.due.values_mut() {
                    due.retain(|d| *d != Due::Physical(speaker));
                }
            }
            self.emit(EventKind::Interrupted { actor: speaker });
        }
    }
    pub(crate) fn validate_realm_change(
        &self,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        if !matches!(
            change,
            InstitutionChange::RatifyRegional { .. }
                | InstitutionChange::AuthorizeRegionalPassage { .. }
        ) {
            return Ok(());
        }
        let Some(world) = &self.host.state.realm else {
            return Err(Error::Unavailable);
        };
        match change {
            InstitutionChange::RatifyRegional {
                commitment,
                revision,
                ..
            } => {
                let c = world
                    .kernel
                    .commitments
                    .get(commitment)
                    .ok_or(Error::Unavailable)?;
                let r = c.current();
                if r.number != *revision
                    || !r.proposal.participants.contains_key(&i.id)
                    || r.proposal.ratify_by < self.host.clock().time
                    || r.status() != game_realm::Status::AwaitingRatification
                {
                    return Err(Error::Unavailable);
                }
            }
            InstitutionChange::AuthorizeRegionalPassage {
                commitment,
                revision,
                index,
            } => {
                let r = world
                    .kernel
                    .commitments
                    .get(commitment)
                    .and_then(|c| c.revision(*revision))
                    .ok_or(Error::Unavailable)?;
                if !matches!(r.proposal.terms.get(usize::from(*index)),Some(Term::Passage {grantor,until,..}) if *grantor==i.id && *until>self.host.clock().time)
                    || r.ratifications.len() != r.proposal.participants.len()
                    || r.ratifications
                        .values()
                        .any(|r| r.outcome != game_realm::Decision::Adopted)
                {
                    return Err(Error::Unavailable);
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn realm_change_actor_allowed(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
        proposing: bool,
    ) -> bool {
        let (id, revision) = match change {
            InstitutionChange::RatifyRegional {
                commitment,
                revision,
                ..
            }
            | InstitutionChange::AuthorizeRegionalPassage {
                commitment,
                revision,
                ..
            } => (*commitment, *revision),
            _ => return true,
        };
        let Some(known) = self
            .host
            .state
            .realm
            .as_ref()
            .and_then(|w| w.kernel.view(actor, id))
            .and_then(|k| k.revisions.get(&revision))
        else {
            return false;
        };
        if !proposing {
            return true;
        }
        let Some(p) = known.proposal.value.participants.get(&i.id) else {
            return false;
        };
        p.representative == actor
            && self.realm_area(actor) == Some(i.charter.jurisdiction)
            && i.charter.offices.iter().any(|o| {
                o.holder == actor
                    && o.powers.contains(&OfficePower::Negotiate)
                    && self.office_effective(i, o)
            })
    }
    fn realm_native_plan(
        &self,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
    ) -> Result<Option<game_realm::PreparedChange>, Error> {
        let record = self
            .host
            .state
            .institutions
            .as_ref()
            .and_then(|w| w.proposals.get(&proposal))
            .ok_or(Error::Unavailable)?;
        let InstitutionChange::RatifyRegional {
            commitment,
            revision,
            support,
        } = record.change
        else {
            return Ok(None);
        };
        if record.institution != institution || status == RuleProposalStatus::Proposed {
            return Err(Error::Unavailable);
        }
        if !support && status == RuleProposalStatus::Rejected {
            return Ok(None);
        }
        let world = self.host.state.realm.as_ref().ok_or(Error::Unavailable)?;
        let decision = Ratification {
            proposal,
            outcome: if support && status == RuleProposalStatus::Adopted {
                game_realm::Decision::Adopted
            } else {
                game_realm::Decision::Rejected
            },
            decided_at: self.host.clock().time,
        };
        world
            .kernel
            .prepare_native_decision(
                commitment,
                revision,
                institution,
                decision,
                self.host.clock().time,
            )
            .map(Some)
    }
    pub(crate) fn preflight_realm_outcome(
        &self,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
    ) -> Result<(), Error> {
        let _ = self.realm_native_plan(institution, proposal, status)?;
        self.ensure_event_capacity(1)
    }
    pub(crate) fn apply_realm_outcome(
        &mut self,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
    ) -> Result<(), Error> {
        let record = self.host.state.institutions.as_ref().unwrap().proposals[&proposal].clone();
        let Some(plan) = self.realm_native_plan(institution, proposal, status)? else {
            return Ok(());
        };
        plan.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        let InstitutionChange::RatifyRegional {
            commitment,
            revision,
            support,
        } = record.change
        else {
            return Ok(());
        };
        let adopted = support && status == RuleProposalStatus::Adopted;
        self.emit(EventKind::RealmRatified {
            commitment,
            revision,
            institution,
            proposal,
            adopted,
        });
        let area = self.host.state.institutions.as_ref().unwrap().institutions[&institution]
            .charter
            .jurisdiction;
        self.realm_publish_proposal(commitment, revision, area)?;
        for actor in self
            .person_index
            .keys()
            .copied()
            .filter(|a| self.realm_area(*a) == Some(area))
            .collect::<Vec<_>>()
        {
            let witness = LocalWitness {
                observer: actor,
                observer_area: area,
                statement_area: area,
                observed_at: self.host.clock().time,
            };
            let p = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_observe_decision(
                    witness,
                    commitment,
                    revision,
                    institution,
                    self.host.clock().time,
                )?;
            p.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        }
        Ok(())
    }
    fn realm_request_work(&mut self, actor: PersonId, key: TermKey) -> Result<(), Error> {
        let world = self.host.state.realm.as_ref().unwrap();
        let r = world
            .kernel
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        let facts = self.realm_facts(actor, &r.proposal, true)?;
        let plan = world
            .kernel
            .prepare_work_request(key, actor, &facts, self.host.clock().time)?;
        let request = plan.requests.first().ok_or(Error::Unavailable)?.clone();
        self.ensure_event_capacity(65)?;
        let owner = match request {
            Request::Delivery { parties, term, .. } => {
                if let Some(existing) = self.agreement_view(actor).into_iter().find(|a| {
                    a.parties == parties
                        && a.terms.contains(&term)
                        && !matches!(
                            a.status,
                            AgreementStatus::Rejected | AgreementStatus::Withdrawn
                        )
                }) {
                    PendingOwner::Agreement(existing.id)
                } else {
                    let before = self.host.next_id();
                    self.agreement_command(
                        actor,
                        &AgreementAction::Propose {
                            parties,
                            terms: vec![term],
                            procedure: None,
                        },
                    )?;
                    let id = self
                        .host
                        .state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .agreements
                        .values()
                        .find(|a| a.id.0 >= before && a.proposer == actor)
                        .ok_or(Error::Unavailable)?
                        .id;
                    PendingOwner::Agreement(id)
                }
            }
            Request::Protection { patrol, .. } => {
                let before = self.host.next_id();
                self.mission_command(actor, &MissionAction::Propose { proposal: patrol })?;
                let id = self
                    .host
                    .state
                    .missions
                    .as_ref()
                    .unwrap()
                    .missions
                    .values()
                    .find(|m| m.id.0 >= before && m.sponsor == actor)
                    .ok_or(Error::Unavailable)?
                    .id;
                PendingOwner::Mission(id)
            }
            Request::Passage { institution, .. } => {
                let before = self.host.next_id();
                self.institution_command(
                    actor,
                    &InstitutionAction::Propose {
                        institution,
                        change: InstitutionChange::AuthorizeRegionalPassage {
                            commitment: key.commitment,
                            revision: key.revision,
                            index: key.index,
                        },
                    },
                )?;
                let id = self
                    .host
                    .state
                    .institutions
                    .as_ref()
                    .unwrap()
                    .proposals
                    .values()
                    .find(|p| p.id.0 >= before && p.proposer == actor)
                    .ok_or(Error::Unavailable)?
                    .id;
                PendingOwner::Passage(id)
            }
            Request::Ratification { .. } => return Err(Error::Unavailable),
        };
        plan.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        self.host
            .state
            .realm
            .as_mut()
            .unwrap()
            .pending
            .insert(key, PendingWork { actor, owner });
        self.emit(EventKind::RealmWorkRequested { key, actor });
        Ok(())
    }
    pub(crate) fn realm_vote_score(&self, actor: PersonId, change: &InstitutionChange) -> i64 {
        let (id, revision, support) = match change {
            InstitutionChange::RatifyRegional {
                commitment,
                revision,
                support,
            } => (*commitment, *revision, *support),
            InstitutionChange::AuthorizeRegionalPassage {
                commitment,
                revision,
                ..
            } => (*commitment, *revision, true),
            _ => return 0,
        };
        let Some(known) = self
            .host
            .state
            .realm
            .as_ref()
            .and_then(|w| w.kernel.view(actor, id))
            .and_then(|k| k.revisions.get(&revision))
        else {
            return -10_000;
        };
        let proposal = &known.proposal.value;
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .unwrap();
        let affiliations = self
            .institution_view(actor)
            .iter()
            .filter(|i| i.members.contains(&actor))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>();
        use game_characters::proposals::{Context, Effect, Evidence};
        let mut effects = Vec::new();
        let resource = |affiliation, gained, term: &AgreementTerm| {
            if let AgreementTerm::Delivery {
                source, quantity, ..
            } = term
            {
                Some(Effect::Resource {
                    affiliation,
                    gained,
                    good: mind.knowledge.get(source).map(|known| known.good),
                    quantity: *quantity,
                })
            } else {
                None
            }
        };
        for term in &proposal.terms {
            match term {
                Term::Delivery {
                    provider,
                    beneficiary,
                    term,
                } => {
                    effects.extend(resource(*provider, false, term));
                    effects.extend(resource(*beneficiary, true, term));
                }
                Term::Passage { beneficiary, .. } => {
                    effects.push(Effect::Passage {
                        beneficiary: *beneficiary,
                    });
                }
                Term::Protection {
                    provider,
                    beneficiary,
                    patrol,
                } => {
                    effects.push(Effect::Protection {
                        beneficiary: *beneficiary,
                    });
                    effects.push(Effect::Danger {
                        provider: *provider,
                    });
                    for term in patrol.compensation.values().flatten() {
                        effects.extend(resource(*provider, false, term));
                    }
                }
            }
        }
        let rules = self.body_rules(actor);
        let context = Context {
            affiliations: &affiliations,
            hunger: game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .hunger,
            hunger_exempt: self.host.state.people[self.person_index[&actor]].hunger_exempt,
            meal_relief: rules.meal_relief,
            meal_size: rules.meal_size,
            hydration_urgency: self.observed_hydration_urgency(actor),
            drink_quantity: self
                .host
                .state
                .health
                .as_ref()
                .map_or(Quantity(u64::MAX), |health| health.rules.drink_quantity),
            concepts: &self.host.state.decisions.as_ref().unwrap().concepts,
        };
        let score = game_characters::proposals::assess(
            &mind.personality,
            &mind.values,
            &context,
            &Evidence {
                proposal: (id, revision),
                effects,
            },
        )
        .total;
        if support {
            score
        } else {
            score.saturating_neg()
        }
    }
    pub(crate) fn observe_realm(&mut self) -> Result<(), Error> {
        let Some(world) = &self.host.state.realm else {
            return Ok(());
        };
        if world.kernel.commitments.is_empty() {
            self.host.state.realm.as_mut().unwrap().last_event = self.host.next_event() - 1;
            return Ok(());
        }
        let _cursor = world.last_event;
        let mut announcements = Vec::new();
        for actor in self.person_index.keys().copied() {
            let Some(area) = self.realm_area(actor) else {
                continue;
            };
            for i in self.institution_view(actor).into_iter().filter(|i| {
                i.observed_at == self.host.clock().time && i.charter.jurisdiction == area
            }) {
                for p in i.proposals {
                    let (commitment, revision) = match p.change {
                        InstitutionChange::RatifyRegional {
                            commitment,
                            revision,
                            ..
                        }
                        | InstitutionChange::AuthorizeRegionalPassage {
                            commitment,
                            revision,
                            ..
                        } => (commitment, revision),
                        _ => continue,
                    };
                    announcements.push((actor, area, i.id, p.id, commitment, revision, p.status));
                }
            }
        }
        for (actor, area, institution, proposal, commitment, revision, status) in announcements {
            let witness = LocalWitness {
                observer: actor,
                observer_area: area,
                statement_area: area,
                observed_at: self.host.clock().time,
            };
            if let Ok(p) = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_observe_proposal(witness, commitment, revision, self.host.clock().time)
            {
                p.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
            }
            let actual = &self.host.state.institutions.as_ref().unwrap().proposals[&proposal];
            if matches!(actual.change, InstitutionChange::RatifyRegional { .. })
                && status == RuleProposalStatus::Proposed
            {
                let kernel = &self.host.state.realm.as_ref().unwrap().kernel;
                let missing = kernel
                    .commitments
                    .get(&commitment)
                    .and_then(|c| c.revision(revision))
                    .is_some_and(|r| !r.ratification_proposals.contains_key(&institution));
                if missing
                    && let Ok(p) = kernel.prepare_bind_ratification(
                        commitment,
                        revision,
                        institution,
                        proposal,
                        self.host.clock().time,
                    )
                {
                    p.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
                }
            }
        }
        self.host.state.realm.as_mut().unwrap().last_event = self.host.next_event() - 1;
        self.reconcile_realm_work()
    }
    fn reconcile_realm_work(&mut self) -> Result<(), Error> {
        let pending = self.host.state.realm.as_ref().map_or_else(Vec::new, |w| {
            w.pending.iter().map(|(k, p)| (*k, p.clone())).collect()
        });
        for (key, pending) in pending {
            let world = self.host.state.realm.as_ref().unwrap();
            let r = world.kernel.commitments[&key.commitment]
                .revision(key.revision)
                .ok_or(Error::Unavailable)?;
            let term = &r.proposal.terms[usize::from(key.index)];
            let receipt = match (&pending.owner, term) {
                (PendingOwner::Agreement(id), Term::Delivery { term, .. }) => {
                    let a = &self.host.state.agreements.as_ref().unwrap().agreements[id];
                    if let Some(obligation) = a.obligations.iter().find(|o| {
                        self.host.state.agreements.as_ref().unwrap().obligations[o].term == *term
                    }) {
                        Some(WorkReceipt::DeliveryOpened {
                            agreement: *id,
                            obligation: Some(*obligation),
                            accepted: true,
                            term: term.clone(),
                        })
                    } else if matches!(
                        a.status,
                        AgreementStatus::Rejected | AgreementStatus::Withdrawn
                    ) {
                        Some(WorkReceipt::DeliveryOpened {
                            agreement: *id,
                            obligation: None,
                            accepted: false,
                            term: term.clone(),
                        })
                    } else {
                        Some(WorkReceipt::DeliveryProposed {
                            agreement: *id,
                            term: term.clone(),
                        })
                    }
                }
                (PendingOwner::Mission(id), Term::Protection { patrol, .. }) => {
                    let m = &self.host.state.missions.as_ref().unwrap().missions[id];
                    if m.proposal.objective != patrol.objective {
                        Some(WorkReceipt::ProtectionOpened {
                            mission: *id,
                            accepted: false,
                            patrol: patrol.clone(),
                        })
                    } else if m.members[&patrol.leader].status == CompanionStatus::Accepted {
                        Some(WorkReceipt::ProtectionOpened {
                            mission: *id,
                            accepted: true,
                            patrol: patrol.clone(),
                        })
                    } else if matches!(
                        m.members[&patrol.leader].status,
                        CompanionStatus::Refused | CompanionStatus::Withdrawn
                    ) {
                        Some(WorkReceipt::ProtectionOpened {
                            mission: *id,
                            accepted: false,
                            patrol: patrol.clone(),
                        })
                    } else {
                        Some(WorkReceipt::ProtectionProposed {
                            mission: *id,
                            patrol: patrol.clone(),
                        })
                    }
                }
                // Adoption is not a movement flag. The territory owner will
                // supply PassageApplied only when its actual effect exists.
                (PendingOwner::Passage(_), Term::Passage { .. }) => None,
                _ => return Err(Error::Unavailable),
            };
            if let Some(receipt) = receipt
                && r.backed
                    .get(&key.index)
                    .is_none_or(|b| b.progress == Progress::AwaitingWork)
            {
                let plan = world
                    .kernel
                    .prepare_backing(key, receipt, self.host.clock().time)?;
                plan.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
            }
            let Some(backed) = self.host.state.realm.as_ref().unwrap().kernel.commitments
                [&key.commitment]
                .revision(key.revision)
                .unwrap()
                .backed
                .get(&key.index)
                .cloned()
            else {
                continue;
            };
            let receipt = match backed.backing {
                Backing::Delivery {
                    obligation: Some(id),
                    ..
                } => {
                    let o = &self.host.state.agreements.as_ref().unwrap().obligations[&id];
                    Some(OutcomeReceipt::Delivery {
                        obligation: id,
                        performed: Quantity(o.performed.min(o.amount())),
                        defaulted: o.defaulted,
                    })
                }
                Backing::Protection { mission } => {
                    let m = &self.host.state.missions.as_ref().unwrap().missions[&mission];
                    let promised = &self.host.state.realm.as_ref().unwrap().kernel.commitments
                        [&key.commitment]
                        .revision(key.revision)
                        .unwrap()
                        .proposal
                        .terms[usize::from(key.index)];
                    let retasked = matches!(promised,Term::Protection {patrol,..} if m.proposal.objective!=patrol.objective);
                    let progress = if retasked {
                        Progress::Defaulted
                    } else {
                        match m.status {
                            MissionStatus::Completed => Progress::Fulfilled,
                            MissionStatus::Failed | MissionStatus::Cancelled => Progress::Defaulted,
                            _ => Progress::Active,
                        }
                    };
                    (!matches!(backed.progress, Progress::Refused | Progress::AwaitingWork))
                        .then_some(OutcomeReceipt::Protection { mission, progress })
                }
                _ => None,
            };
            if let Some(receipt) = receipt {
                let kernel = &self.host.state.realm.as_ref().unwrap().kernel;
                let changed = match &receipt {
                    OutcomeReceipt::Delivery {
                        performed,
                        defaulted,
                        ..
                    } => {
                        *performed != backed.performed
                            || *defaulted && backed.defaulted_at.is_none()
                    }
                    OutcomeReceipt::Protection { progress, .. } => *progress != backed.progress,
                    _ => false,
                };
                if changed {
                    kernel
                        .prepare_outcome(key, receipt, self.host.clock().time)?
                        .commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
                }
            }
            // Only the actor's personally known owner record can yield a local
            // observation. Canonical remote progress never refreshes this cache.
            self.realm_learn_own_work(key, pending.actor)?;
        }
        Ok(())
    }
    fn realm_learn_own_work(&mut self, key: TermKey, actor: PersonId) -> Result<(), Error> {
        let Some(area) = self.realm_area(actor) else {
            return Ok(());
        };
        let Some(r) = self.host.state.realm.as_ref().unwrap().kernel.commitments[&key.commitment]
            .revision(key.revision)
        else {
            return Ok(());
        };
        let Some(b) = r.backed.get(&key.index) else {
            return Ok(());
        };
        let term = &r.proposal.terms[usize::from(key.index)];
        let known = self
            .host
            .state
            .realm
            .as_ref()
            .unwrap()
            .kernel
            .view(actor, key.commitment)
            .and_then(|k| k.revisions.get(&key.revision))
            .and_then(|r| r.backed.get(&key.index));
        let known_default = known.and_then(|b| b.value.defaulted_at);
        let snapshot = match (&b.backing, term) {
            (Backing::Delivery { agreement, .. }, Term::Delivery { term, .. }) => {
                let Some(a) = self
                    .agreement_view(actor)
                    .into_iter()
                    .find(|a| a.id == *agreement)
                else {
                    return Ok(());
                };
                if let Some(o) = a.obligations.iter().find(|o| o.term == *term) {
                    let performed = Quantity(o.performed.min(o.amount()));
                    let progress = if performed.0 == term.amount() {
                        Progress::Fulfilled
                    } else if o.defaulted {
                        Progress::Defaulted
                    } else {
                        Progress::Active
                    };
                    let value = game_realm::BackedTerm {
                        backing: Backing::Delivery {
                            agreement: *agreement,
                            obligation: Some(o.id),
                        },
                        progress,
                        performed,
                        changed_at: o.observed_at,
                        defaulted_at: if o.defaulted {
                            known_default.or(Some(o.observed_at))
                        } else {
                            known_default
                        },
                    };
                    Some(game_realm::Dated {
                        value,
                        observed_at: o.observed_at,
                        learned_at: self.host.clock().time,
                        source: o.source.clone().unwrap_or(KnowledgeSource::Observed(actor)),
                    })
                } else {
                    let progress = if matches!(
                        a.status,
                        AgreementStatus::Rejected | AgreementStatus::Withdrawn
                    ) {
                        Progress::Refused
                    } else {
                        Progress::AwaitingWork
                    };
                    let value = game_realm::BackedTerm {
                        backing: Backing::Delivery {
                            agreement: *agreement,
                            obligation: None,
                        },
                        progress,
                        performed: Quantity(0),
                        changed_at: a.observed_at,
                        defaulted_at: None,
                    };
                    Some(game_realm::Dated {
                        value,
                        observed_at: a.observed_at,
                        learned_at: self.host.clock().time,
                        source: a.source.unwrap_or(KnowledgeSource::Observed(actor)),
                    })
                }
            }
            (Backing::Protection { mission }, Term::Protection { patrol, .. }) => {
                let Some(m) = self
                    .mission_view(actor)
                    .into_iter()
                    .find(|m| m.id == *mission)
                else {
                    return Ok(());
                };
                let progress = if m.objective != patrol.objective {
                    Progress::Defaulted
                } else {
                    match m.status {
                        MissionStatus::Completed => Progress::Fulfilled,
                        MissionStatus::Failed | MissionStatus::Cancelled => Progress::Defaulted,
                        _ => match m
                            .recruitments
                            .iter()
                            .find(|r| r.person == patrol.leader)
                            .map(|r| r.status)
                        {
                            Some(CompanionStatus::Accepted) => Progress::Active,
                            Some(CompanionStatus::Refused | CompanionStatus::Withdrawn) => {
                                Progress::Refused
                            }
                            _ => Progress::AwaitingWork,
                        },
                    }
                };
                let value = game_realm::BackedTerm {
                    backing: Backing::Protection { mission: *mission },
                    progress,
                    performed: Quantity(0),
                    changed_at: m.observed_at,
                    defaulted_at: if progress == Progress::Defaulted {
                        known_default.or(Some(m.observed_at))
                    } else {
                        known_default
                    },
                };
                Some(game_realm::Dated {
                    value,
                    observed_at: m.observed_at,
                    learned_at: self.host.clock().time,
                    source: m
                        .reported_by
                        .map_or(KnowledgeSource::Observed(actor), KnowledgeSource::Reported),
                })
            }
            _ => None,
        };
        let _ = area;
        if let Some(snapshot) = snapshot
            && let Ok(p) = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_learn_work(actor, key, snapshot, self.host.clock().time)
        {
            p.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        }
        Ok(())
    }
    pub(crate) fn available_realm_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = &self.host.state.realm else {
            return Vec::new();
        };
        let Some(area) = self.realm_area(actor) else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        for entry in self.realm_view(actor).commitments {
            for (number, known) in &entry.knowledge.revisions {
                if let Ok(facts) = self.realm_facts(actor, &known.proposal.value, true) {
                    for index in 0..known.proposal.value.terms.len() {
                        let key = TermKey {
                            commitment: entry.id,
                            revision: *number,
                            index: index as u16,
                        };
                        if world
                            .kernel
                            .prepare_work_request(key, actor, &facts, self.host.clock().time)
                            .is_ok()
                        {
                            commands.push(Command::Realm {
                                action: Action::RequestWork { key },
                            });
                        }
                    }
                }
            }
            for recipient in self
                .person_index
                .keys()
                .copied()
                .filter(|p| {
                    *p != actor
                        && self.realm_area(*p) == Some(area)
                        && matches!(
                            *self.host.storage.activity(*p).expect("body exists"),
                            Activity::Idle | Activity::Rest
                        )
                })
                .take(4)
            {
                commands.push(Command::Realm {
                    action: Action::Report {
                        commitment: entry.id,
                        recipient,
                    },
                });
            }
        }
        commands.truncate(64);
        commands
    }
    pub(crate) fn review_realm(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.observe_realm()?;
        let Some(world) = &self.host.state.realm else {
            return Ok(());
        };
        let expiries = world
            .kernel
            .commitments
            .values()
            .filter(|c| {
                c.current().status() == game_realm::Status::AwaitingRatification
                    && c.current().proposal.ratify_by < owner_now
            })
            .map(|c| c.id)
            .collect::<Vec<_>>();
        for id in expiries {
            let p = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .kernel
                .prepare_expire(id, owner_now)?;
            p.commit(&mut self.host.state.realm.as_mut().unwrap().kernel)?;
        }
        for actor in self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.host.state.ruler)
            .collect::<Vec<_>>()
        {
            if self.past_life_controls_actor(actor)
                || self.realm_area(actor).is_none()
                || !matches!(
                    *self.host.storage.activity(actor).expect("body exists"),
                    Activity::Idle | Activity::Rest
                )
                || self
                    .host
                    .state
                    .realm
                    .as_ref()
                    .unwrap()
                    .reconsider_after
                    .get(&actor)
                    .is_some_and(|at| *at > owner_now)
            {
                continue;
            }
            let offers = self.realm_view(actor).commitments;
            let mut signed = false;
            for offer in offers {
                let Some((revision, known)) = offer.knowledge.revisions.last_key_value() else {
                    continue;
                };
                let Ok(facts) = self.realm_facts(actor, &known.proposal.value, true) else {
                    continue;
                };
                for p in known
                    .proposal
                    .value
                    .participants
                    .values()
                    .filter(|p| p.representative == actor)
                {
                    if self
                        .host
                        .state
                        .realm
                        .as_ref()
                        .unwrap()
                        .kernel
                        .prepare_ratification_request(
                            offer.id,
                            p.institution,
                            actor,
                            &facts,
                            owner_now,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    let change = InstitutionChange::RatifyRegional {
                        commitment: offer.id,
                        revision: *revision,
                        support: true,
                    };
                    let support = self.realm_vote_score(actor, &change) >= 0;
                    let change = InstitutionChange::RatifyRegional {
                        commitment: offer.id,
                        revision: *revision,
                        support,
                    };
                    if self
                        .institution_command(
                            actor,
                            &InstitutionAction::Propose {
                                institution: p.institution,
                                change,
                            },
                        )
                        .is_ok()
                    {
                        signed = true;
                        break;
                    }
                }
                if signed {
                    break;
                }
            }
            if !signed {
                for command in self.available_realm_commands(actor) {
                    if let Command::Realm {
                        action: Action::RequestWork { key },
                    } = command
                        && self.realm_request_work(actor, key).is_ok()
                    {
                        break;
                    }
                }
            }
            self.host
                .state
                .realm
                .as_mut()
                .unwrap()
                .reconsider_after
                .insert(actor, SimTime(owner_now.0.saturating_add(180_000)));
        }
        Ok(())
    }
    pub(crate) fn realm_allocation_bound(&self) -> u64 {
        self.host.state.realm.as_ref().map_or(0, |_| {
            (self.host.state.people.len() as u64).saturating_mul(512)
        })
    }
    pub(crate) fn realm_boundary(&self, limit: SimTime) -> SimTime {
        self.host
            .state
            .realm
            .as_ref()
            .into_iter()
            .flat_map(|w| w.kernel.commitments.values())
            .filter(|c| c.current().status() == game_realm::Status::AwaitingRatification)
            .filter_map(|c| c.current().proposal.ratify_by.0.checked_add(1).map(SimTime))
            .filter(|at| *at > self.host.clock().time)
            .min()
            .map_or(limit, |at| at.min(limit))
    }
    pub(crate) fn validate_realm(&self) -> Result<(), Error> {
        let Some(world) = &self.host.state.realm else {
            return Ok(());
        };
        world.rules.validate()?;
        let bad = || Error::InvalidSave("invalid realm communication or owner references".into());
        let institutions = self.host.state.institutions.as_ref().ok_or_else(bad)?;
        let agreements = self.host.state.agreements.as_ref().ok_or_else(bad)?;
        let refs = game_realm::References {
            people: self.person_index.keys().copied().collect(),
            areas: self.host.state.areas.iter().map(|a| a.id).collect(),
            institutions: institutions.institutions.keys().copied().collect(),
            inventories: game_cargo::inventories(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            agreements: agreements.agreements.keys().copied().collect(),
            obligations: agreements.obligations.keys().copied().collect(),
            proposals: institutions.proposals.keys().copied().collect(),
            missions: self
                .host
                .state
                .missions
                .as_ref()
                .map_or_else(BTreeSet::new, |w| w.missions.keys().copied().collect()),
            records: BTreeSet::new(),
        };
        world.kernel.validate(&refs, self.host.clock().time)?;
        if world.last_event >= self.host.next_event() {
            return Err(bad());
        }
        for (actor, talk) in &world.talks {
            if !refs.people.contains(actor)
                || !refs.people.contains(&talk.recipient)
                || !refs.areas.contains(&talk.area)
                || talk.completes <= self.host.clock().time
                || !matches!(*self.host.storage.activity(*actor).expect("body exists"),Activity::RealmTalk {commitment,recipient,completes} if commitment==talk.report.commitment && recipient==talk.recipient && completes==talk.completes)
                || !self
                    .host
                    .state
                    .due
                    .get(&talk.completes)
                    .is_some_and(|d| d.contains(&Due::Physical(*actor)))
                || talk.report.speaker != *actor
                || talk.report.sent_at > self.host.clock().time
            {
                return Err(bad());
            }
            // Validate the frozen packet without delivering it or peeking at a
            // current foreign institution view.
            let _ = world
                .kernel
                .prepare_receive_report(talk.recipient, &talk.report, self.host.clock().time)
                .map_err(|_| bad())?;
            if talk
                .institutions
                .iter()
                .any(|i| !refs.institutions.contains(&i.id) || i.observed_at > talk.report.sent_at)
            {
                return Err(bad());
            }
        }
        for (key, pending) in &world.pending {
            let r = world
                .kernel
                .commitments
                .get(&key.commitment)
                .and_then(|c| c.revision(key.revision))
                .ok_or_else(bad)?;
            if !refs.people.contains(&pending.actor) || !r.requested.contains(&key.index) {
                return Err(bad());
            }
            match (&pending.owner, r.proposal.terms.get(usize::from(key.index))) {
                (PendingOwner::Agreement(id), Some(Term::Delivery { term, .. })) => {
                    if agreements
                        .agreements
                        .get(id)
                        .is_none_or(|a| !a.terms.contains(term))
                    {
                        return Err(bad());
                    }
                }
                (PendingOwner::Mission(id), Some(Term::Protection { patrol, .. })) => {
                    if self
                        .host
                        .state
                        .missions
                        .as_ref()
                        .and_then(|w| w.missions.get(id))
                        .is_none_or(|m| {
                            m.sponsor != pending.actor || m.proposal.leader != patrol.leader
                        })
                    {
                        return Err(bad());
                    }
                }
                (PendingOwner::Passage(id), Some(Term::Passage { .. })) => {
                    if institutions.proposals.get(id).is_none_or(|p| {
                        p.change
                            != InstitutionChange::AuthorizeRegionalPassage {
                                commitment: key.commitment,
                                revision: key.revision,
                                index: key.index,
                            }
                    }) {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        Ok(())
    }
}
