//! Chronological physical receipts qualify conserved cargo exactly once.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptKind {
    Produced {
        actor: PersonId,
        site: SiteId,
        good: Good,
        quantity: Quantity,
    },
    Harvested {
        actor: PersonId,
        field: SiteId,
        food: Quantity,
        seed: Quantity,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Lost {
        source: InventoryId,
        quantity: Quantity,
    },
    ProjectCompleted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub at: SimTime,
    pub kind: ReceiptKind,
}
/// Production owners resolve the physical output belonging to a promised site.
pub trait OutputSources {
    fn output_source(&self, site: SiteId, good: Good) -> Option<InventoryId>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconciliationEvent {
    Accrued {
        obligation: ObligationId,
        actor: PersonId,
        quantity: Quantity,
    },
    Performed {
        obligation: ObligationId,
        actor: PersonId,
        quantity: Quantity,
    },
    Defaulted {
        obligation: ObligationId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedReconciliation {
    before: BTreeMap<ObligationId, Obligation>,
    after: BTreeMap<ObligationId, Obligation>,
    agreements_before: BTreeMap<AgreementId, Agreement>,
    agreements_after: BTreeMap<AgreementId, Agreement>,
    pending: Option<BTreeMap<ObligationId, u64>>,
    cursor_before: u64,
    cursor_after: u64,
    pub events: Vec<ReconciliationEvent>,
}
impl PreparedReconciliation {
    pub fn check(&self, store: &Store, runtime: &Runtime) -> Result<(), Error> {
        if runtime.last_event != self.cursor_before
            || self
                .before
                .iter()
                .any(|(id, row)| obligation(store, *id) != Some(row))
            || self
                .agreements_before
                .iter()
                .any(|(id, row)| agreement(store, *id) != Some(row))
            || self.pending.as_ref().is_some_and(|pending| {
                pending_works(store)
                    .map(|(id, q)| (id, *q))
                    .collect::<BTreeMap<_, _>>()
                    != *pending
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(&self, batch: &mut Batch) {
        for (id, row) in &self.after {
            batch.insert(StableId(id.0), row.clone());
        }
        for (id, row) in &self.agreements_after {
            batch.insert(StableId(id.0), row.clone());
        }
        if let Some(pending) = &self.pending {
            for id in pending.keys() {
                batch.remove::<PendingWork>(StableId(id.0));
            }
        }
    }
    pub fn commit(self, store: &mut Store, runtime: &mut Runtime) -> Result<(), Error> {
        self.check(store, runtime)?;
        let mut batch = Batch::new();
        self.stage(&mut batch);
        store.apply_batch(batch)?;
        runtime.last_event = self.cursor_after;
        Ok(())
    }
    fn edit(&mut self, store: &Store, id: ObligationId) -> &mut Obligation {
        self.after.entry(id).or_insert_with(|| {
            let row = obligation(store, id).expect("obligation exists");
            self.before.insert(id, row.clone());
            row.clone()
        })
    }
    fn row<'a>(&'a self, store: &'a Store, id: ObligationId) -> &'a Obligation {
        self.after
            .get(&id)
            .unwrap_or_else(|| obligation(store, id).expect("obligation exists"))
    }
    fn lose(&mut self, store: &Store, source: InventoryId, mut quantity: u64) {
        for (id, _) in obligations(store) {
            let Some(cargo) = self.row(store, id).cargo.get(&source).copied() else {
                continue;
            };
            let lost = quantity.min(cargo);
            if lost == 0 {
                continue;
            }
            quantity -= lost;
            let row = self.edit(store, id);
            if cargo == lost {
                row.cargo.remove(&source);
            } else {
                row.cargo.insert(source, cargo - lost);
            }
        }
    }
    fn refresh(&mut self, store: &Store, id: AgreementId, now: SimTime) -> Result<(), Error> {
        let original = agreement(store, id).ok_or(Error::Unavailable)?;
        let mut fulfilled = true;
        let mut breached = false;
        for id in &original.obligations {
            let row = self.row(store, *id);
            fulfilled &= row.performed >= row.amount() && stream_ended(&row.term, now);
            breached |= row.defaulted;
        }
        let status = if fulfilled {
            AgreementStatus::Fulfilled
        } else if breached {
            AgreementStatus::Breached
        } else {
            AgreementStatus::Active
        };
        // Every obligation used to compute a status is an opening dependency.
        for id in &original.obligations {
            self.before
                .entry(*id)
                .or_insert_with(|| obligation(store, *id).unwrap().clone());
        }
        self.agreements_before
            .entry(id)
            .or_insert_with(|| original.clone());
        let mut after = original.clone();
        after.status = status;
        self.agreements_after.insert(id, after);
        Ok(())
    }
}
pub fn prepare_reconciliation(
    store: &Store,
    runtime: &Runtime,
    now: SimTime,
    cursor: u64,
    receipts: &[Receipt],
    outputs: &impl OutputSources,
) -> Result<PreparedReconciliation, Error> {
    if cursor < runtime.last_event {
        return Err(Error::Unavailable);
    }
    let flush = now.0.is_multiple_of(60_000)
        || receipts.iter().any(|receipt| {
            receipt.at == now
                && matches!(
                    receipt.kind,
                    ReceiptKind::Produced { .. } | ReceiptKind::ProjectCompleted
                )
        })
        || obligations(store).any(|(_, row)| row.term.deadline() == Some(now));
    let mut plan = PreparedReconciliation {
        before: BTreeMap::new(),
        after: BTreeMap::new(),
        agreements_before: BTreeMap::new(),
        agreements_after: BTreeMap::new(),
        pending: None,
        cursor_before: runtime.last_event,
        cursor_after: cursor,
        events: Vec::new(),
    };
    let mut accruals = Vec::new();
    let mut credits = Vec::new();
    for receipt in receipts {
        let productions = match receipt.kind {
            ReceiptKind::Produced {
                actor,
                site,
                good,
                quantity,
            } => vec![(actor, site, good, quantity)],
            ReceiptKind::Harvested {
                actor,
                field,
                food,
                seed,
            } => vec![
                (actor, field, Good::Food, food),
                (actor, field, Good::Seed, seed),
            ],
            _ => Vec::new(),
        };
        for (producer, site, good, quantity) in productions {
            for (id, _) in obligations(store) {
                let old = plan.row(store, id);
                if let AgreementTerm::OutputShare {
                    site: owed,
                    good: kind,
                    numerator,
                    denominator,
                    ends_at,
                    ..
                } = old.term
                    && owed == site
                    && kind == good
                    && receipt.at < ends_at
                {
                    let (accrued, remainder, scale) = accrue_share(
                        old.accrued,
                        old.share_remainder,
                        old.share_scale,
                        numerator,
                        denominator,
                        u128::from(quantity.0),
                    )?;
                    let credit = accrued - old.accrued;
                    let row = plan.edit(store, id);
                    row.accrued = accrued;
                    row.share_remainder = remainder;
                    row.share_scale = scale;
                    if credit > 0 {
                        accruals.push(ReconciliationEvent::Accrued {
                            obligation: id,
                            actor: producer,
                            quantity: Quantity(credit),
                        });
                    }
                }
            }
        }
        match receipt.kind {
            ReceiptKind::Transferred {
                actor,
                source,
                destination,
                quantity,
            } => {
                let carried = game_cargo::inventory(store, destination)
                    .ok_or(Error::Unavailable)?
                    .location
                    == game_cargo::InventoryLocation::Carried(actor);
                let mut unallocated = quantity.0;
                for (id, _) in obligations(store) {
                    let old = plan.row(store, id);
                    let (debtor, origin, goal, promised) = match old.term {
                        AgreementTerm::Delivery {
                            debtor,
                            source,
                            destination,
                            quantity,
                            ..
                        } => (debtor, source, destination, quantity.0),
                        AgreementTerm::OutputShare {
                            debtor,
                            site,
                            good,
                            destination,
                            ..
                        } => (
                            debtor,
                            outputs
                                .output_source(site, good)
                                .ok_or(Error::Unavailable)?,
                            destination,
                            old.accrued,
                        ),
                        _ => continue,
                    };
                    let source_cargo = old.cargo.get(&source).copied().unwrap_or(0);
                    let from_cargo = source_cargo.min(unallocated);
                    unallocated -= from_cargo;
                    let direct = source == origin && actor == debtor;
                    let eligible = if direct { unallocated } else { from_cargo };
                    let outstanding = promised
                        .checked_sub(old.performed)
                        .ok_or(Error::Unavailable)?;
                    let credit = if actor == debtor && destination == goal {
                        eligible.min(outstanding)
                    } else {
                        0
                    };
                    let allocation = if actor == debtor && destination != goal && carried {
                        let already = old.cargo.values().sum::<u64>().saturating_sub(from_cargo);
                        eligible.min(outstanding.saturating_sub(already))
                    } else {
                        0
                    };
                    if source_cargo == 0 && credit == 0 && allocation == 0 {
                        continue;
                    }
                    let row = plan.edit(store, id);
                    row.cargo.remove(&source);
                    if source_cargo > from_cargo {
                        row.cargo.insert(source, source_cargo - from_cargo);
                    }
                    row.performed += credit;
                    if credit > 0 {
                        credits.push((
                            row.agreement,
                            ReconciliationEvent::Performed {
                                obligation: id,
                                actor,
                                quantity: Quantity(credit),
                            },
                        ));
                        if direct {
                            unallocated -= credit;
                        }
                    }
                    if allocation > 0 {
                        *row.cargo.entry(destination).or_default() += allocation;
                        if direct {
                            unallocated -= allocation;
                        }
                    }
                }
            }
            ReceiptKind::Lost { source, quantity } => plan.lose(store, source, quantity.0),
            _ => {}
        }
    }
    let mut totals = BTreeMap::<InventoryId, u64>::new();
    for (id, _) in obligations(store) {
        for (inventory, q) in &plan.row(store, id).cargo {
            let sum = totals.entry(*inventory).or_default();
            *sum = sum.checked_add(*q).ok_or(Error::Overflow)?;
        }
    }
    for (inventory, attributed) in totals {
        let physical = game_cargo::inventory(store, inventory)
            .ok_or(Error::Unavailable)?
            .quantity
            .0;
        if attributed > physical {
            plan.lose(store, inventory, attributed - physical);
        }
    }
    plan.events.extend(accruals);
    if flush {
        let pending = pending_works(store)
            .map(|(id, q)| (id, *q))
            .collect::<BTreeMap<_, _>>();
        for (id, millis) in &pending {
            plan.events.push(ReconciliationEvent::Performed {
                obligation: *id,
                actor: plan.row(store, *id).term.parties()[0],
                quantity: Quantity(*millis),
            });
        }
        plan.pending = Some(pending);
    }
    for (agreement, event) in credits {
        plan.events.push(event);
        plan.refresh(store, agreement, now)?;
    }
    for (id, row) in agreements(store) {
        if !row.obligations.is_empty()
            && row
                .terms
                .iter()
                .any(|term| matches!(term, AgreementTerm::OutputShare { .. }))
        {
            plan.refresh(store, id, now)?;
        }
    }
    for (id, _) in obligations(store) {
        let row = plan.row(store, id);
        if !row.defaulted
            && row.performed < row.amount()
            && row.term.deadline().is_some_and(|due| due <= now)
        {
            let agreement = row.agreement;
            plan.edit(store, id).defaulted = true;
            plan.events
                .push(ReconciliationEvent::Defaulted { obligation: id });
            plan.refresh(store, agreement, now)?;
        }
    }
    Ok(plan)
}
pub fn next_deadline(store: &Store, now: SimTime) -> Option<SimTime> {
    obligations(store)
        .flat_map(|(_, o)| {
            [
                (!o.defaulted && o.performed < o.amount())
                    .then(|| o.term.deadline())
                    .flatten(),
                match o.term {
                    AgreementTerm::OutputShare { ends_at, .. } => Some(ends_at),
                    _ => None,
                },
            ]
            .into_iter()
            .flatten()
        })
        .filter(|at| *at > now)
        .min()
}
pub fn preflight_output_shares(
    store: &Store,
    production: &[(SiteId, Good, Quantity)],
) -> Result<(), Error> {
    for (_, o) in obligations(store) {
        if let AgreementTerm::OutputShare {
            site,
            good,
            numerator,
            denominator,
            ..
        } = o.term
        {
            let total = production
                .iter()
                .filter(|(s, g, _)| *s == site && *g == good)
                .try_fold(0_u128, |sum, (_, _, q)| sum.checked_add(u128::from(q.0)))
                .ok_or(Error::Overflow)?;
            accrue_share(
                o.accrued,
                o.share_remainder,
                o.share_scale,
                numerator,
                denominator,
                total,
            )?;
        }
    }
    Ok(())
}
