use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

pub type Portions = BTreeMap<ClaimId, u64>;
#[derive(Clone, Copy, Debug)]
pub struct ActorFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub area: Option<AreaId>,
    pub capable: bool,
    pub exertion_ready: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct StockFacts {
    pub id: InventoryId,
    pub area: Option<AreaId>,
    pub good: Good,
    pub quantity: Quantity,
    pub room: Quantity,
    pub carried_by: Option<PersonId>,
    pub actor_allowed: bool,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Copy, Debug)]
pub struct ContactFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub present: bool,
    pub capable: bool,
    pub available: bool,
    pub autonomous: bool,
}
pub enum ActionFacts {
    Claim {
        stock: StockFacts,
        id: ClaimId,
        witnesses: Vec<PersonId>,
    },
    Recognition {
        witnesses: Vec<PersonId>,
    },
    Request {
        stock: StockFacts,
        custodian: ContactFacts,
        trust: i32,
        reserve: Quantity,
    },
    Share {
        stock: StockFacts,
        recipient: ContactFacts,
    },
    Take {
        source: StockFacts,
        destination: StockFacts,
        id: IncidentId,
        carrying_room: Quantity,
        duration: SimDuration,
    },
    Report {
        recipient: ContactFacts,
        duration: SimDuration,
    },
    Resist {
        thief: ContactFacts,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Interrupt {
        actor: PersonId,
    },
    ClearTaking {
        actor: PersonId,
    },
    ClearReport {
        sender: PersonId,
        completes: SimTime,
        interrupted: bool,
    },
    ClaimPublished {
        claim: ClaimId,
        actor: PersonId,
        inventory: InventoryId,
    },
    Recognized {
        claim: ClaimId,
        actor: PersonId,
        support: bool,
    },
    Requested {
        actor: PersonId,
        custodian: PersonId,
        inventory: InventoryId,
    },
    Refused {
        actor: PersonId,
        custodian: PersonId,
        inventory: InventoryId,
    },
    Permission {
        actor: PersonId,
        inventory: InventoryId,
        recipient: PersonId,
        allowed: bool,
    },
    StartTake {
        incident: IncidentId,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    TakingInterrupted {
        incident: IncidentId,
        actor: PersonId,
    },
    Transfer {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Taken {
        incident: IncidentId,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    StartReport {
        sender: PersonId,
        recipient: PersonId,
        incident: IncidentId,
        completes: SimTime,
    },
    Reported {
        sender: PersonId,
        recipient: PersonId,
        incident: IncidentId,
    },
    Engage {
        defender: PersonId,
        thief: PersonId,
    },
    Relationship {
        observer: PersonId,
        target: PersonId,
        trust: i32,
    },
}
#[derive(Clone, Debug)]
pub struct PreparedProperty {
    pub(crate) claims: BTreeMap<ClaimId, (Option<Claim>, Claim)>,
    pub(crate) incidents: BTreeMap<IncidentId, (Option<Incident>, Incident)>,
    pub(crate) portions: BTreeMap<InventoryId, (Option<Portions>, Portions)>,
    pending: BTreeMap<PersonId, (Option<IncidentId>, Option<IncidentId>)>,
    reports: BTreeMap<PersonId, (Option<Report>, Option<Report>)>,
    known_claims: BTreeMap<(PersonId, ClaimId), (Option<ClaimView>, ClaimView)>,
    knowledge: BTreeMap<(PersonId, IncidentId), (Option<IncidentKnowledge>, IncidentKnowledge)>,
    pub(crate) born: BTreeSet<PersonId>,
    cooldown: BTreeMap<PersonId, (Option<SimTime>, SimTime)>,
    pub(crate) cursor: Option<(u64, u64)>,
    effects: Vec<Effect>,
}
impl PreparedProperty {
    pub(crate) fn new() -> Self {
        Self {
            claims: BTreeMap::new(),
            incidents: BTreeMap::new(),
            portions: BTreeMap::new(),
            pending: BTreeMap::new(),
            reports: BTreeMap::new(),
            known_claims: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            born: BTreeSet::new(),
            cooldown: BTreeMap::new(),
            cursor: None,
            effects: Vec::new(),
        }
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub(crate) fn current_portions<'a>(
        &'a self,
        world: &'a World,
        id: InventoryId,
    ) -> Option<&'a Portions> {
        self.portions
            .get(&id)
            .map(|(_, after)| after)
            .or_else(|| world.portions.get(&id))
    }
    pub(crate) fn set_portions(&mut self, world: &World, id: InventoryId, after: Portions) {
        self.portions
            .insert(id, (world.portions.get(&id).cloned(), after));
    }
    /// Prove that the future physical transfer has a valid conserved receipt.
    /// Keep both opening partitions in the prepared CAS without moving them:
    /// only the authoritative physical receipt will actually relocate portions.
    fn prepare_transfer_receipt(
        &mut self,
        world: &World,
        source: StockFacts,
        destination: StockFacts,
        quantity: Quantity,
    ) -> Result<Portions, Error> {
        let from = self
            .current_portions(world, source.id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let to = self
            .current_portions(world, destination.id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let total = |parts: &Portions| {
            parts.values().try_fold(0u64, |total, q| {
                total.checked_add(*q).ok_or(Error::Overflow)
            })
        };
        if total(&from)? != source.quantity.0 || total(&to)? != destination.quantity.0 {
            return Err(Error::Unavailable);
        }
        let (_, moved) = split_portions(&from, quantity.0)?;
        let mut predicted = to.clone();
        for (marker, q) in &moved {
            let current = predicted.entry(*marker).or_default();
            *current = current.checked_add(*q).ok_or(Error::Overflow)?;
        }
        let closing = destination
            .quantity
            .0
            .checked_add(quantity.0)
            .ok_or(Error::Overflow)?;
        if total(&predicted)? != closing {
            return Err(Error::Unavailable);
        }
        self.set_portions(world, source.id, from);
        self.set_portions(world, destination.id, to);
        Ok(moved)
    }
    fn set_incident(&mut self, world: &World, id: IncidentId, after: Incident) {
        self.incidents
            .insert(id, (world.incidents.get(&id).cloned(), after));
    }
    fn set_knowledge(&mut self, world: &World, observer: PersonId, after: IncidentKnowledge) {
        let id = after.incident.id;
        self.knowledge.insert(
            (observer, id),
            (
                world
                    .knowledge
                    .get(&observer)
                    .and_then(|k| k.get(&id))
                    .cloned(),
                after,
            ),
        );
    }
    fn cancel(&mut self, world: &World, actor: PersonId, now: SimTime) {
        if let Some(id) = world.pending.get(&actor)
            && let Some(incident) = world.incidents.get(id)
        {
            let mut after = incident.clone();
            after.outcome = IncidentOutcome::Interrupted;
            after.changed_at = now;
            self.set_incident(world, *id, after);
            self.pending.insert(actor, (Some(*id), None));
            self.effects.push(Effect::TakingInterrupted {
                incident: *id,
                actor,
            });
            self.effects.push(Effect::ClearTaking { actor });
        }
        for report in world
            .reports
            .values()
            .filter(|r| r.sender == actor || r.recipient == actor)
        {
            self.reports
                .insert(report.sender, (Some(report.clone()), None));
            self.effects.push(Effect::ClearReport {
                sender: report.sender,
                completes: report.completes,
                interrupted: true,
            });
        }
    }
    pub fn commit(self, world: &mut World) -> Result<Vec<Effect>, Error> {
        if self
            .claims
            .iter()
            .any(|(id, (before, _))| world.claims.get(id) != before.as_ref())
            || self
                .incidents
                .iter()
                .any(|(id, (before, _))| world.incidents.get(id) != before.as_ref())
            || self
                .portions
                .iter()
                .any(|(id, (before, _))| world.portions.get(id) != before.as_ref())
            || self
                .pending
                .iter()
                .any(|(id, (before, _))| world.pending.get(id) != before.as_ref())
            || self
                .reports
                .iter()
                .any(|(id, (before, _))| world.reports.get(id) != before.as_ref())
            || self.known_claims.iter().any(|((person, id), (before, _))| {
                world.known_claims.get(person).and_then(|k| k.get(id)) != before.as_ref()
            })
            || self.knowledge.iter().any(|((person, id), (before, _))| {
                world.knowledge.get(person).and_then(|k| k.get(id)) != before.as_ref()
            })
            || self
                .cooldown
                .iter()
                .any(|(id, (before, _))| world.next_search.get(id) != before.as_ref())
            || self
                .cursor
                .is_some_and(|(before, _)| world.last_event != before)
        {
            return Err(Error::Unavailable);
        }
        for (id, (_, after)) in self.claims {
            world.claims.insert(id, after);
        }
        for (id, (_, after)) in self.incidents {
            world.incidents.insert(id, after);
        }
        for (id, (_, after)) in self.portions {
            world.portions.insert(id, after);
        }
        for (id, (_, after)) in self.pending {
            if let Some(after) = after {
                world.pending.insert(id, after);
            } else {
                world.pending.remove(&id);
            }
        }
        for (id, (_, after)) in self.reports {
            if let Some(after) = after {
                world.reports.insert(id, after);
            } else {
                world.reports.remove(&id);
            }
        }
        for ((person, id), (_, after)) in self.known_claims {
            world
                .known_claims
                .entry(person)
                .or_default()
                .insert(id, after);
        }
        for ((person, id), (_, after)) in self.knowledge {
            world.knowledge.entry(person).or_default().insert(id, after);
        }
        for actor in self.born {
            world.attitudes.entry(actor).or_insert(Attitude {
                exclusive_claims: 0,
                sharing: 0,
                willingness_to_defend: 0,
            });
            world.known_claims.entry(actor).or_default();
            world.knowledge.entry(actor).or_default();
        }
        for (id, (_, after)) in self.cooldown {
            world.next_search.insert(id, after);
        }
        if let Some((_, after)) = self.cursor {
            world.last_event = after;
        }
        Ok(self.effects)
    }
}
pub fn split_portions(source: &Portions, quantity: u64) -> Result<(Portions, Portions), Error> {
    let total = source.values().try_fold(0u64, |total, q| {
        total.checked_add(*q).ok_or(Error::Overflow)
    })?;
    if quantity > total {
        return Err(Error::Unavailable);
    }
    let mut left = source.clone();
    let mut removed = BTreeMap::new();
    let mut remaining = quantity;
    for (id, q) in source {
        if remaining == 0 {
            break;
        }
        let amount = (*q).min(remaining);
        if amount == 0 {
            continue;
        }
        if amount == *q {
            left.remove(id);
        } else {
            left.insert(*id, *q - amount);
        }
        removed.insert(*id, amount);
        remaining -= amount;
    }
    Ok((left, removed))
}
pub fn taking_possible(
    actor: ActorFacts,
    source: StockFacts,
    destination: StockFacts,
    quantity: Quantity,
    carrying_room: Quantity,
    require_denial: bool,
) -> bool {
    actor.capable
        && actor.exertion_ready
        && actor.area.is_some()
        && quantity.0 > 0
        && source.id != destination.id
        && (!require_denial || !source.actor_allowed)
        && source.area == actor.area
        && destination.area == actor.area
        && source.good == destination.good
        && destination.carried_by == Some(actor.actor)
        && destination.actor_allowed
        && source.quantity >= quantity
        && destination.room >= quantity
        && carrying_room >= quantity
}
impl World {
    pub fn actor_talking(&self, actor: PersonId) -> bool {
        self.reports.contains_key(&actor) || self.reports.values().any(|r| r.recipient == actor)
    }
    pub fn prepare_cancel(&self, actor: PersonId, now: SimTime) -> PreparedProperty {
        let mut change = PreparedProperty::new();
        change.cancel(self, actor, now);
        change
    }
    pub fn prepare_action(
        &self,
        actor: ActorFacts,
        action: &PropertyAction,
        facts: ActionFacts,
        prefix: Option<PreparedProperty>,
    ) -> Result<PreparedProperty, Error> {
        if !actor.capable {
            return Err(Error::Unavailable);
        }
        let area = actor.area.ok_or(Error::Unavailable)?;
        let mut change = prefix.unwrap_or_else(PreparedProperty::new);
        match (action, facts) {
            (
                PropertyAction::Claim {
                    inventory,
                    rights,
                    evidence,
                },
                ActionFacts::Claim {
                    stock,
                    id,
                    witnesses,
                },
            ) => {
                let parts = change
                    .current_portions(self, *inventory)
                    .ok_or(Error::Unavailable)?;
                if parts.values().try_fold(0u64, |total, q| {
                    total.checked_add(*q).ok_or(Error::Overflow)
                })? != stock.quantity.0
                {
                    return Err(Error::Unavailable);
                }
                if id.0 == 0
                    || self.claims.contains_key(&id)
                    || stock.id != *inventory
                    || stock.area != Some(area)
                    || stock.quantity.0 == 0
                    || rights.is_empty()
                    || evidence.len() > 16
                    || evidence.iter().any(|id| {
                        !self
                            .knowledge
                            .get(&actor.actor)
                            .is_some_and(|known| known.contains_key(id))
                    })
                    || self.claims.values().any(|c| {
                        c.claimant == actor.actor
                            && c.inventory == *inventory
                            && c.rights == *rights
                            && c.evidence == *evidence
                            && parts.keys().all(|p| c.portions.contains(p))
                    })
                {
                    return Err(Error::Unavailable);
                }
                let mut parts = parts.clone();
                if let Some(q) = parts.remove(&ClaimId(0)) {
                    parts.insert(id, q);
                }
                let assertion = Claim {
                    id,
                    claimant: actor.actor,
                    inventory: *inventory,
                    rights: rights.clone(),
                    evidence: evidence.clone(),
                    portions: parts.keys().copied().collect(),
                    recognized_by: BTreeSet::from([actor.actor]),
                    opposed_by: BTreeSet::new(),
                };
                let view = ClaimView {
                    id,
                    claimant: actor.actor,
                    inventory: *inventory,
                    rights: rights.clone(),
                    evidence: evidence.clone(),
                    recognized_by: BTreeSet::from([actor.actor]),
                    opposed_by: BTreeSet::new(),
                };
                change.claims.insert(id, (None, assertion));
                change.set_portions(self, *inventory, parts);
                for observer in witnesses.into_iter().chain([actor.actor]) {
                    change.known_claims.insert(
                        (observer, id),
                        (
                            self.known_claims
                                .get(&observer)
                                .and_then(|k| k.get(&id))
                                .cloned(),
                            view.clone(),
                        ),
                    );
                }
                change.effects.push(Effect::ClaimPublished {
                    claim: id,
                    actor: actor.actor,
                    inventory: *inventory,
                });
            }
            (PropertyAction::Recognize { claim }, ActionFacts::Recognition { witnesses })
            | (PropertyAction::Contest { claim }, ActionFacts::Recognition { witnesses }) => {
                let support = matches!(action, PropertyAction::Recognize { .. });
                let mut view = self
                    .known_claims
                    .get(&actor.actor)
                    .and_then(|k| k.get(claim))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                let before = self.claims.get(claim).ok_or(Error::Unavailable)?;
                let mut after = before.clone();
                recognize(
                    &mut after.recognized_by,
                    &mut after.opposed_by,
                    actor.actor,
                    support,
                );
                recognize(
                    &mut view.recognized_by,
                    &mut view.opposed_by,
                    actor.actor,
                    support,
                );
                change.claims.insert(*claim, (Some(before.clone()), after));
                for observer in witnesses.into_iter().chain([actor.actor]) {
                    let before = self
                        .known_claims
                        .get(&observer)
                        .and_then(|k| k.get(claim))
                        .cloned();
                    let mut after = before.clone().unwrap_or_else(|| view.clone());
                    recognize(
                        &mut after.recognized_by,
                        &mut after.opposed_by,
                        actor.actor,
                        support,
                    );
                    change
                        .known_claims
                        .insert((observer, *claim), (before, after));
                }
                change.effects.push(Effect::Recognized {
                    claim: *claim,
                    actor: actor.actor,
                    support,
                });
            }
            (
                PropertyAction::RequestAccess { inventory },
                ActionFacts::Request {
                    stock,
                    custodian,
                    trust,
                    reserve,
                },
            ) => {
                if stock.id != *inventory
                    || stock.custodian != Some(custodian.actor)
                    || custodian.actor == actor.actor
                    || !custodian.present
                    || custodian.area != Some(area)
                    || stock.area != Some(area)
                    || stock.actor_allowed
                {
                    return Err(Error::Unavailable);
                }
                change.cooldown.insert(
                    actor.actor,
                    (
                        self.next_search.get(&actor.actor).copied(),
                        SimTime(actor.now.0.saturating_add(120_000)),
                    ),
                );
                change.effects.push(Effect::Requested {
                    actor: actor.actor,
                    custodian: custodian.actor,
                    inventory: *inventory,
                });
                if custodian.autonomous
                    && custodian.capable
                    && self
                        .attitudes
                        .get(&custodian.actor)
                        .is_some_and(|a| a.sharing >= 700)
                    && stock.quantity >= reserve
                    && trust >= 0
                {
                    change.effects.push(Effect::Permission {
                        actor: custodian.actor,
                        inventory: *inventory,
                        recipient: actor.actor,
                        allowed: true,
                    });
                } else {
                    change.effects.push(Effect::Refused {
                        actor: actor.actor,
                        custodian: custodian.actor,
                        inventory: *inventory,
                    });
                }
            }
            (
                PropertyAction::Share {
                    inventory,
                    recipient,
                    allowed,
                },
                ActionFacts::Share {
                    stock,
                    recipient: contact,
                },
            ) => {
                if stock.id != *inventory
                    || stock.custodian != Some(actor.actor)
                    || contact.actor != *recipient
                    || !contact.present
                    || contact.area != Some(area)
                    || stock.area != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                change.effects.push(Effect::Permission {
                    actor: actor.actor,
                    inventory: *inventory,
                    recipient: *recipient,
                    allowed: *allowed,
                });
            }
            (
                PropertyAction::Take {
                    source,
                    destination,
                    quantity,
                },
                ActionFacts::Take {
                    source: from,
                    destination: to,
                    id,
                    carrying_room,
                    duration,
                },
            ) => {
                if from.id != *source
                    || to.id != *destination
                    || id.0 == 0
                    || duration.0 == 0
                    || self.incidents.contains_key(&id)
                    || !taking_possible(actor, from, to, *quantity, carrying_room, true)
                {
                    return Err(Error::Unavailable);
                }
                let completes =
                    SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
                let (_, portions) = split_portions(
                    change
                        .current_portions(self, *source)
                        .ok_or(Error::Unavailable)?,
                    quantity.0,
                )?;
                change.cancel(self, actor.actor, actor.now);
                change
                    .effects
                    .push(Effect::Interrupt { actor: actor.actor });
                change.set_incident(
                    self,
                    id,
                    Incident {
                        id,
                        actor: actor.actor,
                        source: *source,
                        destination: *destination,
                        quantity: *quantity,
                        area,
                        started_at: actor.now,
                        completes,
                        changed_at: actor.now,
                        outcome: IncidentOutcome::Attempt,
                        portions,
                    },
                );
                change.pending.insert(
                    actor.actor,
                    (self.pending.get(&actor.actor).copied(), Some(id)),
                );
                change.cooldown.insert(
                    actor.actor,
                    (
                        self.next_search.get(&actor.actor).copied(),
                        SimTime(actor.now.0.saturating_add(120_000)),
                    ),
                );
                change.effects.push(Effect::StartTake {
                    incident: id,
                    actor: actor.actor,
                    source: *source,
                    destination: *destination,
                    quantity: *quantity,
                    completes,
                });
            }
            (
                PropertyAction::ReportIncident {
                    recipient,
                    incident,
                },
                ActionFacts::Report {
                    recipient: contact,
                    duration,
                },
            ) => {
                let snapshot = self
                    .knowledge
                    .get(&actor.actor)
                    .and_then(|k| k.get(incident))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                if *recipient != contact.actor
                    || duration.0 == 0
                    || *recipient == actor.actor
                    || !contact.present
                    || contact.area != Some(area)
                    || !contact.available
                    || self.actor_talking(*recipient)
                    || snapshot.learned_at > actor.now
                {
                    return Err(Error::Unavailable);
                }
                let completes =
                    SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
                change.cancel(self, actor.actor, actor.now);
                change
                    .effects
                    .push(Effect::Interrupt { actor: actor.actor });
                change.reports.insert(
                    actor.actor,
                    (
                        self.reports.get(&actor.actor).cloned(),
                        Some(Report {
                            sender: actor.actor,
                            recipient: *recipient,
                            area,
                            started_at: actor.now,
                            completes,
                            snapshot,
                        }),
                    ),
                );
                change.effects.push(Effect::StartReport {
                    sender: actor.actor,
                    recipient: *recipient,
                    incident: *incident,
                    completes,
                });
            }
            (PropertyAction::ResistTaking { thief }, ActionFacts::Resist { thief: contact }) => {
                let id = self.pending.get(thief).ok_or(Error::Unavailable)?;
                let incident = &self.incidents[id];
                if *thief != contact.actor
                    || *thief == actor.actor
                    || !contact.present
                    || contact.area != Some(area)
                    || incident.area != area
                    || !self
                        .knowledge
                        .get(&actor.actor)
                        .is_some_and(|k| k.contains_key(id))
                {
                    return Err(Error::Unavailable);
                }
                change.effects.push(Effect::Engage {
                    defender: actor.actor,
                    thief: *thief,
                });
            }
            _ => return Err(Error::Unavailable),
        }
        Ok(change)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ObserverFacts {
    pub actor: PersonId,
    pub personal_custody: bool,
    pub personally_allowed: bool,
    pub can_defend: bool,
}
impl World {
    fn interpretation(&self, observer: ObserverFacts, incident: &Incident) -> i32 {
        if observer.actor == incident.actor {
            return 0;
        }
        let Some(attitude) = self.attitudes.get(&observer.actor) else {
            return 0;
        };
        let supported = self.known_claims.get(&observer.actor).is_some_and(|known| {
            known.keys().any(|id| {
                self.claims.get(id).is_some_and(|claim| {
                    claim.recognized_by.contains(&observer.actor)
                        && claim.rights.contains(&ClaimRight::Transfer)
                        && claim
                            .portions
                            .iter()
                            .any(|portion| incident.portions.contains_key(portion))
                })
            })
        });
        if !observer.personally_allowed
            && (supported || observer.personal_custody)
            && attitude.exclusive_claims >= 500
        {
            -100
        } else if attitude.sharing >= 700 {
            10
        } else {
            0
        }
    }
    pub fn prepare_witness(
        &self,
        id: IncidentId,
        now: SimTime,
        observers: &[ObserverFacts],
    ) -> Result<PreparedProperty, Error> {
        let incident = self.incidents.get(&id).ok_or(Error::Unavailable)?;
        if incident.changed_at > now {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedProperty::new();
        for observer in observers {
            let earlier = self
                .knowledge
                .get(&observer.actor)
                .and_then(|k| k.get(&id))
                .map(|k| k.incident.outcome);
            change.set_knowledge(
                self,
                observer.actor,
                IncidentKnowledge {
                    incident: incident.clone(),
                    learned_at: now,
                    source: IncidentSource::Observed,
                },
            );
            if incident.outcome == IncidentOutcome::Taken && earlier != Some(IncidentOutcome::Taken)
            {
                let delta = self.interpretation(*observer, incident);
                if delta != 0 {
                    change.effects.push(Effect::Relationship {
                        observer: observer.actor,
                        target: incident.actor,
                        trust: delta,
                    });
                }
            }
        }
        Ok(change)
    }
    pub fn resistance(&self, thief: PersonId, observers: &[ObserverFacts]) -> Option<Effect> {
        let id = self.pending.get(&thief)?;
        let incident = self.incidents.get(id)?;
        observers
            .iter()
            .filter(|p| p.actor != thief && p.can_defend)
            .find(|p| {
                let supported = self.known_claims.get(&p.actor).is_some_and(|known| {
                    known.keys().any(|id| {
                        self.claims.get(id).is_some_and(|claim| {
                            claim.recognized_by.contains(&p.actor)
                                && claim
                                    .portions
                                    .iter()
                                    .any(|marker| incident.portions.contains_key(marker))
                        })
                    })
                });
                (p.personal_custody || supported)
                    && self
                        .attitudes
                        .get(&p.actor)
                        .is_some_and(|a| a.willingness_to_defend >= 500)
            })
            .map(|p| Effect::Engage {
                defender: p.actor,
                thief,
            })
    }
    pub fn prepare_finish_take(
        &self,
        actor: ActorFacts,
        source: StockFacts,
        destination: StockFacts,
        carrying_room: Quantity,
        prefix: Option<PreparedProperty>,
    ) -> Result<PreparedProperty, Error> {
        let id = self.pending.get(&actor.actor).ok_or(Error::Unavailable)?;
        let before = self.incidents.get(id).ok_or(Error::Unavailable)?;
        if actor.now < before.completes
            || source.id != before.source
            || destination.id != before.destination
        {
            return Err(Error::Unavailable);
        }
        let success = taking_possible(
            actor,
            source,
            destination,
            before.quantity,
            carrying_room,
            false,
        );
        let mut change = prefix.unwrap_or_else(PreparedProperty::new);
        let mut after = before.clone();
        if success {
            let parts =
                change.prepare_transfer_receipt(self, source, destination, before.quantity)?;
            after.portions = parts;
            after.outcome = IncidentOutcome::Taken;
            change.effects.push(Effect::Transfer {
                actor: actor.actor,
                source: before.source,
                destination: before.destination,
                quantity: before.quantity,
            });
            change.effects.push(Effect::Taken {
                incident: *id,
                actor: actor.actor,
                source: before.source,
                destination: before.destination,
                quantity: before.quantity,
            });
        } else {
            after.outcome = IncidentOutcome::Interrupted;
            change.effects.push(Effect::TakingInterrupted {
                incident: *id,
                actor: actor.actor,
            });
        }
        after.changed_at = actor.now;
        change.set_incident(self, *id, after);
        change.pending.insert(actor.actor, (Some(*id), None));
        change
            .effects
            .push(Effect::ClearTaking { actor: actor.actor });
        Ok(change)
    }
    pub fn prepare_finish_report(
        &self,
        actor: ActorFacts,
        recipient: ContactFacts,
        observer: ObserverFacts,
    ) -> Result<PreparedProperty, Error> {
        let sender = actor.actor;
        let now = actor.now;
        let report = self.reports.get(&sender).ok_or(Error::Unavailable)?;
        if now < report.completes {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedProperty::new();
        change.reports.insert(sender, (Some(report.clone()), None));
        let interrupted = !actor.capable
            || actor.area != Some(report.area)
            || recipient.actor != report.recipient
            || !recipient.present
            || !recipient.capable
            || recipient.area != Some(report.area)
            || !recipient.available;
        change.effects.push(Effect::ClearReport {
            sender,
            completes: report.completes,
            interrupted,
        });
        if interrupted {
            return Ok(change);
        }
        let id = report.snapshot.incident.id;
        let previous = self
            .knowledge
            .get(&recipient.actor)
            .and_then(|k| k.get(&id));
        let received = &report.snapshot.incident;
        let update = previous.is_none_or(|known| {
            known.incident.changed_at < received.changed_at
                || known.incident.changed_at == received.changed_at
                    && (known.incident.outcome == IncidentOutcome::Attempt
                        || received.outcome != IncidentOutcome::Attempt)
        });
        if update {
            change.set_knowledge(
                self,
                recipient.actor,
                IncidentKnowledge {
                    incident: received.clone(),
                    learned_at: now,
                    source: IncidentSource::Reported { sender },
                },
            );
            if received.outcome == IncidentOutcome::Taken
                && previous.is_none_or(|k| k.incident.outcome != IncidentOutcome::Taken)
            {
                let delta = self.interpretation(observer, received);
                if delta != 0 {
                    change.effects.push(Effect::Relationship {
                        observer: observer.actor,
                        target: received.actor,
                        trust: delta,
                    });
                }
            }
        }
        change.effects.push(Effect::Reported {
            sender,
            recipient: recipient.actor,
            incident: id,
        });
        Ok(change)
    }
}
fn recognize(
    supported: &mut BTreeSet<PersonId>,
    opposed: &mut BTreeSet<PersonId>,
    actor: PersonId,
    yes: bool,
) {
    if yes {
        supported.insert(actor);
        opposed.remove(&actor);
    } else {
        opposed.insert(actor);
        supported.remove(&actor);
    }
}
