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
    report_membership: BTreeMap<PersonId, Vec<(PersonId, Report)>>,
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
            report_membership: BTreeMap::new(),
        }
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub(crate) fn current_portions<'a>(
        &'a self,
        store: &'a Store,
        id: InventoryId,
    ) -> Option<&'a Portions> {
        self.portions
            .get(&id)
            .map(|(_, after)| after)
            .or_else(|| portions(store, id))
    }
    pub(crate) fn set_portions(&mut self, store: &Store, id: InventoryId, after: Portions) {
        self.portions
            .insert(id, (portions(store, id).cloned(), after));
    }
    /// Prove that the future physical transfer has a valid conserved receipt.
    /// Keep both opening partitions in the prepared CAS without moving them:
    /// only the authoritative physical receipt will actually relocate portions.
    fn prepare_transfer_receipt(
        &mut self,
        store: &Store,
        source: StockFacts,
        destination: StockFacts,
        quantity: Quantity,
    ) -> Result<Portions, Error> {
        let from = self
            .current_portions(store, source.id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let to = self
            .current_portions(store, destination.id)
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
        self.set_portions(store, source.id, from);
        self.set_portions(store, destination.id, to);
        Ok(moved)
    }
    fn set_incident(&mut self, store: &Store, id: IncidentId, after: Incident) {
        self.incidents
            .insert(id, (incident(store, id).cloned(), after));
    }
    fn set_knowledge(&mut self, store: &Store, observer: PersonId, after: IncidentKnowledge) {
        let id = after.incident.id;
        self.knowledge.insert(
            (observer, id),
            (
                known_incidents(store, observer)
                    .and_then(|k| k.get(&id))
                    .cloned(),
                after,
            ),
        );
    }
    pub(crate) fn cancel(&mut self, store: &Store, actor: PersonId, now: SimTime) {
        self.report_membership
            .entry(actor)
            .or_insert_with(|| report_membership(store, actor));
        if let Some(id) = pending(store, actor)
            && let Some(incident) = incident(store, *id)
        {
            let mut after = incident.clone();
            after.outcome = IncidentOutcome::Interrupted;
            after.changed_at = now;
            self.set_incident(store, *id, after);
            self.pending.insert(actor, (Some(*id), None));
            self.effects.push(Effect::TakingInterrupted {
                incident: *id,
                actor,
            });
            self.effects.push(Effect::ClearTaking { actor });
        }
        for report in reports(store)
            .map(|(_, row)| row)
            .filter(|r| r.sender == actor || r.recipient == actor)
        {
            if self
                .reports
                .get(&report.sender)
                .is_some_and(|(_, after)| after.is_none())
            {
                continue;
            }
            self.reports
                .insert(report.sender, (Some(report.clone()), None));
            self.effects.push(Effect::ClearReport {
                sender: report.sender,
                completes: report.completes,
                interrupted: true,
            });
        }
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if self
            .claims
            .iter()
            .any(|(id, (before, _))| claim(store, *id) != before.as_ref())
            || self
                .incidents
                .iter()
                .any(|(id, (before, _))| incident(store, *id) != before.as_ref())
            || self
                .portions
                .iter()
                .any(|(id, (before, _))| portions(store, *id) != before.as_ref())
            || self
                .pending
                .iter()
                .any(|(actor, (before, _))| pending(store, *actor) != before.as_ref())
            || self
                .reports
                .iter()
                .any(|(actor, (before, _))| report(store, *actor) != before.as_ref())
            || self.known_claims.iter().any(|((actor, id), (before, _))| {
                known_claims(store, *actor).and_then(|rows| rows.get(id)) != before.as_ref()
            })
            || self.knowledge.iter().any(|((actor, id), (before, _))| {
                known_incidents(store, *actor).and_then(|rows| rows.get(id)) != before.as_ref()
            })
            || self
                .cooldown
                .iter()
                .any(|(actor, (before, _))| next_search(store, *actor) != before.as_ref())
            || self
                .cursor
                .is_some_and(|(before, _)| runtime.last_event != before)
            || self
                .report_membership
                .iter()
                .any(|(actor, before)| report_membership(store, *actor) != *before)
        {
            return Err(Error::Unavailable);
        }
        for (id, (_, after)) in &self.claims {
            if *id != after.id {
                return Err(Error::Unavailable);
            }
            after.validate()?;
        }
        for (id, (_, after)) in &self.incidents {
            if *id != after.id {
                return Err(Error::Unavailable);
            }
            after.validate()?;
        }
        for (_, after) in self.portions.values() {
            Provenance {
                portions: after.clone(),
            }
            .validate()?;
        }
        Ok(())
    }
    /// Stage only this owner's rows. The caller discards the entire transient
    /// batch on failure and finishes headers/effects after its combined admission.
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<PropertyFinalization, Error> {
        self.validate(runtime, store)?;
        let mut memories = BTreeMap::<PersonId, Knowledge>::new();
        let mut actions = BTreeMap::<PersonId, Activity>::new();
        for (id, (_, after)) in self.claims {
            batch.insert(StableId(id.0), after);
        }
        for (id, (_, after)) in self.incidents {
            batch.insert(StableId(id.0), after);
        }
        for (id, (_, after)) in self.portions {
            batch.insert(StableId(id.0), Provenance { portions: after });
        }
        for (actor, (_, after)) in self.pending {
            actions
                .entry(actor)
                .or_insert_with(|| activity(store, actor).cloned().unwrap_or_default())
                .pending = after;
        }
        for (actor, (_, after)) in self.reports {
            actions
                .entry(actor)
                .or_insert_with(|| activity(store, actor).cloned().unwrap_or_default())
                .report = after;
        }
        for ((actor, id), (_, after)) in self.known_claims {
            memories
                .entry(actor)
                .or_insert_with(|| knowledge(store, actor).cloned().unwrap_or_default())
                .claims
                .insert(id, after);
        }
        for ((actor, id), (_, after)) in self.knowledge {
            memories
                .entry(actor)
                .or_insert_with(|| knowledge(store, actor).cloned().unwrap_or_default())
                .incidents
                .insert(id, after);
        }
        for actor in self.born {
            if attitude(store, actor).is_none() {
                batch.insert(
                    StableId(actor.0),
                    Attitude {
                        exclusive_claims: 0,
                        sharing: 0,
                        willingness_to_defend: 0,
                    },
                );
            }
            memories
                .entry(actor)
                .or_insert_with(|| knowledge(store, actor).cloned().unwrap_or_default());
        }
        for (actor, row) in memories {
            batch.insert(StableId(actor.0), row);
        }
        for (actor, row) in actions {
            if row.pending.is_none() && row.report.is_none() {
                batch.remove::<Activity>(StableId(actor.0));
            } else {
                batch.insert(StableId(actor.0), row);
            }
        }
        for (actor, (_, after)) in self.cooldown {
            batch.insert(StableId(actor.0), Requests { next_search: after });
        }
        Ok(PropertyFinalization {
            cursor: self.cursor.map(|(_, after)| after),
            effects: self.effects,
        })
    }
    pub fn commit(self, runtime: &mut Runtime, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
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
impl Runtime {
    pub fn actor_talking(&self, store: &Store, actor: PersonId) -> bool {
        report(store, actor).is_some()
            || reports(store)
                .map(|(_, row)| row)
                .any(|r| r.recipient == actor)
    }
    pub fn prepare_cancel(&self, store: &Store, actor: PersonId, now: SimTime) -> PreparedProperty {
        let mut change = PreparedProperty::new();
        change.cancel(store, actor, now);
        change
    }
    pub fn prepare_action(
        &self,
        store: &Store,
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
                    .current_portions(store, *inventory)
                    .ok_or(Error::Unavailable)?;
                if parts.values().try_fold(0u64, |total, q| {
                    total.checked_add(*q).ok_or(Error::Overflow)
                })? != stock.quantity.0
                {
                    return Err(Error::Unavailable);
                }
                if id.0 == 0
                    || claim(store, id).is_some()
                    || stock.id != *inventory
                    || stock.area != Some(area)
                    || stock.quantity.0 == 0
                    || rights.is_empty()
                    || evidence.len() > 16
                    || evidence.iter().any(|id| {
                        !known_incidents(store, actor.actor)
                            .is_some_and(|known| known.contains_key(id))
                    })
                    || claims(store).map(|(_, row)| row).any(|c| {
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
                change.set_portions(store, *inventory, parts);
                for observer in witnesses.into_iter().chain([actor.actor]) {
                    change.known_claims.insert(
                        (observer, id),
                        (
                            known_claims(store, observer)
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
                let mut view = known_claims(store, actor.actor)
                    .and_then(|k| k.get(claim))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                let before = crate::claim(store, *claim).ok_or(Error::Unavailable)?;
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
                    let before = known_claims(store, observer)
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
                        next_search(store, actor.actor).copied(),
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
                    && attitude(store, custodian.actor).is_some_and(|a| a.sharing >= 700)
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
                    || incident(store, id).is_some()
                    || !taking_possible(actor, from, to, *quantity, carrying_room, true)
                {
                    return Err(Error::Unavailable);
                }
                let completes =
                    SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
                let (_, portions) = split_portions(
                    change
                        .current_portions(store, *source)
                        .ok_or(Error::Unavailable)?,
                    quantity.0,
                )?;
                change.cancel(store, actor.actor, actor.now);
                change
                    .effects
                    .push(Effect::Interrupt { actor: actor.actor });
                change.set_incident(
                    store,
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
                    (pending(store, actor.actor).copied(), Some(id)),
                );
                change.cooldown.insert(
                    actor.actor,
                    (
                        next_search(store, actor.actor).copied(),
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
                let snapshot = known_incidents(store, actor.actor)
                    .and_then(|k| k.get(incident))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                if *recipient != contact.actor
                    || duration.0 == 0
                    || *recipient == actor.actor
                    || !contact.present
                    || contact.area != Some(area)
                    || !contact.available
                    || self.actor_talking(store, *recipient)
                    || snapshot.learned_at > actor.now
                {
                    return Err(Error::Unavailable);
                }
                let completes =
                    SimTime(actor.now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
                change.cancel(store, actor.actor, actor.now);
                change
                    .effects
                    .push(Effect::Interrupt { actor: actor.actor });
                change.reports.insert(
                    actor.actor,
                    (
                        report(store, actor.actor).cloned(),
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
                let id = pending(store, *thief).ok_or(Error::Unavailable)?;
                let incident = incident(store, *id).ok_or(Error::Unavailable)?;
                if *thief != contact.actor
                    || *thief == actor.actor
                    || !contact.present
                    || contact.area != Some(area)
                    || incident.area != area
                    || !known_incidents(store, actor.actor).is_some_and(|k| k.contains_key(id))
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
impl Runtime {
    fn interpretation(&self, store: &Store, observer: ObserverFacts, incident: &Incident) -> i32 {
        if observer.actor == incident.actor {
            return 0;
        }
        let Some(attitude) = attitude(store, observer.actor) else {
            return 0;
        };
        let supported = known_claims(store, observer.actor).is_some_and(|known| {
            known.keys().any(|id| {
                claim(store, *id).is_some_and(|claim| {
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
        store: &Store,
        id: IncidentId,
        now: SimTime,
        observers: &[ObserverFacts],
    ) -> Result<PreparedProperty, Error> {
        let incident = incident(store, id).ok_or(Error::Unavailable)?;
        if incident.changed_at > now {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedProperty::new();
        for observer in observers {
            let earlier = known_incidents(store, observer.actor)
                .and_then(|k| k.get(&id))
                .map(|k| k.incident.outcome);
            change.set_knowledge(
                store,
                observer.actor,
                IncidentKnowledge {
                    incident: incident.clone(),
                    learned_at: now,
                    source: IncidentSource::Observed,
                },
            );
            if incident.outcome == IncidentOutcome::Taken && earlier != Some(IncidentOutcome::Taken)
            {
                let delta = self.interpretation(store, *observer, incident);
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
    pub fn resistance(
        &self,
        store: &Store,
        thief: PersonId,
        observers: &[ObserverFacts],
    ) -> Option<Effect> {
        let id = pending(store, thief)?;
        let incident = incident(store, *id)?;
        observers
            .iter()
            .filter(|p| p.actor != thief && p.can_defend)
            .find(|p| {
                let supported = known_claims(store, p.actor).is_some_and(|known| {
                    known.keys().any(|id| {
                        claim(store, *id).is_some_and(|claim| {
                            claim.recognized_by.contains(&p.actor)
                                && claim
                                    .portions
                                    .iter()
                                    .any(|marker| incident.portions.contains_key(marker))
                        })
                    })
                });
                (p.personal_custody || supported)
                    && attitude(store, p.actor).is_some_and(|a| a.willingness_to_defend >= 500)
            })
            .map(|p| Effect::Engage {
                defender: p.actor,
                thief,
            })
    }
    pub fn prepare_finish_take(
        &self,
        store: &Store,
        actor: ActorFacts,
        source: StockFacts,
        destination: StockFacts,
        carrying_room: Quantity,
        prefix: Option<PreparedProperty>,
    ) -> Result<PreparedProperty, Error> {
        let id = pending(store, actor.actor).ok_or(Error::Unavailable)?;
        let before = incident(store, *id).ok_or(Error::Unavailable)?;
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
                change.prepare_transfer_receipt(store, source, destination, before.quantity)?;
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
        change.set_incident(store, *id, after);
        change.pending.insert(actor.actor, (Some(*id), None));
        change
            .effects
            .push(Effect::ClearTaking { actor: actor.actor });
        Ok(change)
    }
    pub fn prepare_finish_report(
        &self,
        store: &Store,
        actor: ActorFacts,
        recipient: ContactFacts,
        observer: ObserverFacts,
    ) -> Result<PreparedProperty, Error> {
        let sender = actor.actor;
        let now = actor.now;
        let report = report(store, sender).ok_or(Error::Unavailable)?;
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
        let previous = known_incidents(store, recipient.actor).and_then(|k| k.get(&id));
        let received = &report.snapshot.incident;
        let update = previous.is_none_or(|known| {
            known.incident.changed_at < received.changed_at
                || known.incident.changed_at == received.changed_at
                    && (known.incident.outcome == IncidentOutcome::Attempt
                        || received.outcome != IncidentOutcome::Attempt)
        });
        if update {
            change.set_knowledge(
                store,
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
                let delta = self.interpretation(store, observer, received);
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

fn report_membership(store: &Store, actor: PersonId) -> Vec<(PersonId, Report)> {
    reports(store)
        .filter(|(_, report)| report.sender == actor || report.recipient == actor)
        .map(|(id, report)| (id, report.clone()))
        .collect()
}
/// Finish only after the staged host batch was admitted in the same serialized window.
pub struct PropertyFinalization {
    cursor: Option<u64>,
    effects: Vec<Effect>,
}
impl PropertyFinalization {
    pub fn finish(self, runtime: &mut Runtime) -> Vec<Effect> {
        if let Some(cursor) = self.cursor {
            runtime.last_event = cursor;
        }
        self.effects
    }
}
