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
    obligation_members: BTreeSet<ObligationId>,
    agreement_members: BTreeSet<AgreementId>,
    cursor_before: u64,
    cursor_after: u64,
    stock_reads: BTreeMap<
        InventoryId,
        (
            Option<game_cargo::Inventory>,
            Option<game_cargo::Inventory>,
            usize,
        ),
    >,
    source_batch: Option<game_ecs::BatchKey>,
    pub events: Vec<ReconciliationEvent>,
}
impl PreparedReconciliation {
    fn stock_row(
        &mut self,
        store: &Store,
        projection: Option<&Batch>,
        id: InventoryId,
    ) -> Result<Option<game_cargo::Inventory>, Error> {
        if let Some((_, after, _)) = self.stock_reads.get(&id) {
            return Ok(after.clone());
        }
        let before = game_cargo::inventory(store, id).cloned();
        let after = if let Some(batch) = projection {
            batch
                .projected::<game_cargo::Inventory>(store, StableId(id.0))?
                .cloned()
        } else {
            before.clone()
        };
        let edits = usize::from(
            projection.is_some_and(|batch| batch.touches::<game_cargo::Inventory>(StableId(id.0))),
        );
        self.stock_reads.insert(id, (before, after.clone(), edits));
        Ok(after)
    }
    pub(crate) fn projected_agreement<'a>(
        &'a self,
        store: &'a Store,
        id: AgreementId,
    ) -> Option<&'a Agreement> {
        self.agreements_after
            .get(&id)
            .or_else(|| agreement(store, id))
    }
    pub(crate) fn projected_obligation<'a>(
        &'a self,
        store: &'a Store,
        id: ObligationId,
    ) -> Option<&'a Obligation> {
        self.after.get(&id).or_else(|| obligation(store, id))
    }
    pub(crate) fn write_footprint(&self) -> game_ecs::ReadFootprint {
        let mut writes = game_ecs::ReadFootprint::new();
        for id in self.after.keys() {
            writes.protect::<Obligation>(StableId(id.0));
        }
        for id in self.agreements_after.keys() {
            writes.protect::<Agreement>(StableId(id.0));
        }
        if let Some(pending) = &self.pending {
            for id in pending.keys() {
                writes.protect::<PendingWork>(StableId(id.0));
            }
        }
        writes
    }
    pub fn check(&self, store: &Store, runtime: &Runtime) -> Result<(), Error> {
        if self
            .stock_reads
            .iter()
            .any(|(id, (before, _, _))| game_cargo::inventory(store, *id) != before.as_ref())
            || runtime.last_event != self.cursor_before
            || obligations(store)
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>()
                != self.obligation_members
            || agreements(store).map(|(id, _)| id).collect::<BTreeSet<_>>()
                != self.agreement_members
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
    pub fn stage_into(
        &self,
        store: &Store,
        runtime: &Runtime,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.check(store, runtime)?;
        if self
            .source_batch
            .as_ref()
            .is_some_and(|key| *key != batch.key())
        {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for (id, (before, after, edits)) in &self.stock_reads {
            let id = StableId(id.0);
            if batch.projected::<game_cargo::Inventory>(store, id)? != after.as_ref()
                || usize::from(batch.touches::<game_cargo::Inventory>(id)) != *edits
            {
                return Err(Error::Unavailable);
            }
            reads.expect_row(id, before.clone());
            if *edits > 0 {
                admitted.protect::<game_cargo::Inventory>(id);
            }
        }
        reads.membership::<Obligation>(store, |_, _| true);
        reads.membership::<Agreement>(store, |_, _| true);
        for (id, row) in &self.before {
            reads.expect_row(StableId(id.0), Some(row.clone()));
        }
        for (id, row) in &self.agreements_before {
            reads.expect_row(StableId(id.0), Some(row.clone()));
        }
        for id in self.after.keys() {
            if batch.touches::<Obligation>(StableId(id.0)) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Obligation>(StableId(id.0));
        }
        for id in self.agreements_after.keys() {
            if batch.touches::<Agreement>(StableId(id.0)) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Agreement>(StableId(id.0));
        }
        if let Some(pending) = &self.pending {
            reads.membership::<PendingWork>(store, |_, _| true);
            for (id, value) in pending {
                reads.expect_row(StableId(id.0), Some(PendingWork { millis: *value }));
                if batch.touches::<PendingWork>(StableId(id.0)) {
                    return Err(Error::Unavailable);
                }
                admitted.protect::<PendingWork>(StableId(id.0));
            }
        }
        self.stage(batch);
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store, runtime: &mut Runtime) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, runtime, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        runtime.last_event = self.cursor_after;
        Ok(())
    }
    pub(crate) fn cursor_after(&self) -> u64 {
        self.cursor_after
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
    prepare_reconciliation_from(store, runtime, now, cursor, receipts, outputs, None)
}
pub fn prepare_reconciliation_projected(
    store: &Store,
    runtime: &Runtime,
    now: SimTime,
    cursor: u64,
    receipts: &[Receipt],
    outputs: &impl OutputSources,
    projection: &Batch,
) -> Result<PreparedReconciliation, Error> {
    prepare_reconciliation_from(
        store,
        runtime,
        now,
        cursor,
        receipts,
        outputs,
        Some(projection),
    )
}
fn prepare_reconciliation_from(
    store: &Store,
    runtime: &Runtime,
    now: SimTime,
    cursor: u64,
    receipts: &[Receipt],
    outputs: &impl OutputSources,
    projection: Option<&Batch>,
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
        obligation_members: obligations(store).map(|(id, _)| id).collect(),
        agreement_members: agreements(store).map(|(id, _)| id).collect(),
        cursor_before: runtime.last_event,
        cursor_after: cursor,
        stock_reads: BTreeMap::new(),
        source_batch: projection.map(Batch::key),
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
                let carried = plan
                    .stock_row(store, projection, destination)?
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
        let physical = plan
            .stock_row(store, projection, inventory)?
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
