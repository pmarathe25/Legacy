//! Ordered receipt interpretation prepares sparse native rows before publication.
use super::*;
use crate::storage::{Concept, Exclusions, Knowledge};
use game_ecs::{BatchKey, Component, ReadFootprint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptObservation {
    pub observers: BTreeMap<PersonId, u16>,
    pub reads: ReadFootprint,
}
pub trait ReceiptObserverFacts {
    fn subject(&self, actor: PersonId) -> Option<SubjectFacts>;
    fn subject_reads(&self, actor: PersonId) -> ReadFootprint;
    fn observe(&self, case: &SocialCaseView, public: bool) -> Result<ReceiptObservation, Error>;
    fn observe_batch(
        &self,
        batch: &Batch,
        case: &SocialCaseView,
        public: bool,
    ) -> Result<ReceiptObservation, Error>;
}
#[derive(Default)]
struct ProjectedRows {
    knowledge: BTreeMap<PersonId, (Option<Knowledge>, Knowledge)>,
    exclusions: BTreeMap<PersonId, (Option<Exclusions>, Exclusions)>,
    cases: BTreeMap<CaseId, SocialCaseView>,
}
pub(super) struct ReceiptRows<'a> {
    store: &'a Store,
    projected: Option<&'a ProjectedRows>,
}
impl<'a> ReceiptRows<'a> {
    pub(super) fn opening(store: &'a Store) -> Self {
        Self {
            store,
            projected: None,
        }
    }
    fn knowledge(&self, actor: PersonId) -> Option<&Knowledge> {
        self.projected
            .and_then(|rows| rows.knowledge.get(&actor).map(|(_, after)| after))
            .or_else(|| crate::storage::knowledge(self.store, actor))
    }
    fn exclusions(&self, actor: PersonId) -> Option<&Exclusions> {
        self.projected
            .and_then(|rows| rows.exclusions.get(&actor).map(|(_, after)| after))
            .or_else(|| crate::storage::exclusions(self.store, actor))
    }
    fn case(&self, id: CaseId) -> Option<&SocialCaseView> {
        self.projected
            .and_then(|rows| rows.cases.get(&id))
            .or_else(|| crate::storage::case(self.store, id))
    }
    pub(super) fn known_count(
        &self,
        actor: PersonId,
        subject: PersonId,
        kind: SocialCaseKind,
    ) -> usize {
        self.knowledge(actor).map_or(0, |known| {
            known
                .cases
                .iter()
                .rev()
                .take(64)
                .filter(|id| {
                    self.case(**id)
                        .is_some_and(|case| case.subject == subject && case.kind == kind)
                })
                .count()
        })
    }
    pub(super) fn exclusion_id(&self, actor: PersonId, source: InventoryId) -> Option<CaseId> {
        self.exclusions(actor)?.stores.get(&source).copied()
    }
    pub(super) fn excluded_case(
        &self,
        actor: PersonId,
        source: InventoryId,
    ) -> Option<&SocialCaseView> {
        self.case(self.exclusion_id(actor, source)?)
    }
}
#[derive(Clone, Copy)]
pub(super) enum Selector {
    Linked(AgreementId),
    Sharing(PersonId, InventoryId),
}
struct SelectionSource {
    selector: Selector,
    selected: Option<(InstitutionId, ConceptId)>,
    institution: Option<Institution>,
    concept: Option<Concept>,
}
impl Selector {
    pub(super) fn select(
        &self,
        store: &Store,
        batch: Option<&Batch>,
    ) -> Option<(InstitutionId, ConceptId)> {
        let mut ids = crate::storage::institutions(store)
            .map(|(id, _)| StableId(id.0))
            .collect::<BTreeSet<_>>();
        if let Some(batch) = batch {
            ids.extend(batch.insertions::<Institution>().map(|(id, _)| id));
        }
        for id in ids {
            let Some(institution) = projected::<Institution>(store, batch, id) else {
                continue;
            };
            let concept = match self {
                Self::Linked(agreement) => institution.applied.iter().find_map(|(concept, people)| {
                    people.values().any(|id| id == agreement).then_some(*concept)
                }),
                Self::Sharing(actor, source) => institution.concepts.iter().find(|concept| {
                    institution.grants.get(concept).is_some_and(|people| people.contains(actor))
                        && projected::<Concept>(store, batch, StableId(concept.0)).is_some_and(|row| {
                            matches!(row.rule.effect, SocialEffect::Access { inventory, allowed: true } if inventory == *source)
                        })
                }).copied(),
            };
            if let Some(concept) = concept {
                return Some((institution.id, concept));
            }
        }
        None
    }
}
fn projected<'a, C: Component>(
    store: &'a Store,
    batch: Option<&'a Batch>,
    id: StableId,
) -> Option<&'a C> {
    if let Some(batch) = batch.filter(|batch| batch.touches::<C>(id)) {
        batch
            .insertions::<C>()
            .find(|(edited, _)| *edited == id)
            .map(|(_, row)| row)
    } else {
        store.get::<C>(id)
    }
}
struct ObservationSource {
    case: SocialCaseView,
    public: bool,
    before: ReceiptObservation,
}
pub struct PreparedReceiptReactions {
    before_cursor: u64,
    through: u64,
    at: SimTime,
    subjects: Vec<(SubjectFacts, ReadFootprint)>,
    observations: Vec<ObservationSource>,
    selections: Vec<SelectionSource>,
    knowledge_reads: BTreeMap<PersonId, Option<Knowledge>>,
    exclusion_reads: BTreeMap<PersonId, Option<Exclusions>>,
    case_reads: BTreeMap<CaseId, Option<SocialCaseView>>,
    reads: ReadFootprint,
    projected: ProjectedRows,
}
pub struct ReceiptReactionsFinalization {
    key: BatchKey,
    source: PreparedReceiptReactions,
    writes: ReadFootprint,
    reads: ReadFootprint,
}
impl Runtime {
    pub fn prepare_receipt_reactions(
        &self,
        store: &Store,
        receipts: &[(SocialReceipt, SubjectFacts)],
        at: SimTime,
        start_case: CaseId,
        through: u64,
        facts: &impl ReceiptObserverFacts,
    ) -> Result<PreparedReceiptReactions, Error> {
        if start_case.0 == 0 || through < self.last_event {
            return Err(Error::Unavailable);
        }
        let all = receipts
            .iter()
            .map(|(receipt, _)| *receipt)
            .collect::<Vec<_>>();
        let index = ReceiptIndex::new(&all);
        let mut plan = PreparedReceiptReactions {
            before_cursor: self.last_event,
            through,
            at,
            subjects: Vec::new(),
            observations: Vec::new(),
            selections: Vec::new(),
            knowledge_reads: BTreeMap::new(),
            exclusion_reads: BTreeMap::new(),
            case_reads: BTreeMap::new(),
            reads: ReadFootprint::new(),
            projected: ProjectedRows::default(),
        };
        let mut next = start_case.0;
        for (receipt, subject) in receipts {
            if facts.subject(subject.actor) != Some(*subject) {
                return Err(Error::Unavailable);
            }
            let subject_reads = facts.subject_reads(subject.actor);
            plan.subjects.push((*subject, subject_reads.clone()));
            plan.reads.extend(subject_reads);
            match *receipt {
                SocialReceipt::Defaulted { actor, .. } if actor == subject.actor => {
                    plan.read_known_cases(store, actor)
                }
                SocialReceipt::SupportReceived { actor, source }
                | SocialReceipt::Ate { actor, source }
                | SocialReceipt::Transferred {
                    actor,
                    source,
                    good: Good::Food,
                    destination_holder: Some(_),
                    ..
                } if actor == subject.actor
                    && !matches!(*receipt,SocialReceipt::Transferred {destination_holder:Some(holder),..} if holder!=actor) =>
                {
                    plan.exclusion_reads
                        .entry(actor)
                        .or_insert_with(|| crate::storage::exclusions(store, actor).cloned());
                    plan.reads.protect::<Exclusions>(StableId(actor.0));
                    if let Some(id) = crate::storage::exclusions(store, actor)
                        .and_then(|rows| rows.stores.get(&source))
                        .copied()
                    {
                        plan.read_case(store, id);
                    }
                }
                _ => {}
            }
            let rows = ReceiptRows {
                store,
                projected: Some(&plan.projected),
            };
            let Some(mut reaction) =
                self.receipt_reaction_in(store, receipt, *subject, at, &index, &rows)
            else {
                continue;
            };
            let selector = match *receipt {
                SocialReceipt::Performed { agreement, .. }
                | SocialReceipt::Defaulted { agreement, .. } => Some(Selector::Linked(agreement)),
                SocialReceipt::SupportReceived { actor, source }
                | SocialReceipt::Ate { actor, source }
                | SocialReceipt::Transferred { actor, source, .. } => {
                    Some(Selector::Sharing(actor, source))
                }
                _ => None,
            };
            if let Some(selector) = selector {
                plan.read_selection(store, selector);
            }
            if reaction
                .concept
                .is_some_and(|id| crate::storage::concept(store, id).is_none())
            {
                return Err(Error::Unavailable);
            }
            // The legacy observation selection sees an unallocated case id.
            let observation = facts.observe(&reaction.case, reaction.public)?;
            plan.reads.extend(observation.reads.clone());
            plan.observations.push(ObservationSource {
                case: reaction.case.clone(),
                public: reaction.public,
                before: observation.clone(),
            });
            let id = CaseId(next);
            next = next.checked_add(1).ok_or(Error::Overflow)?;
            if store.contains(StableId(id.0)) {
                return Err(Error::Unavailable);
            }
            reaction.case.id = id;
            reaction.case.validate_shape()?;
            for (actor, empathy) in observation.observers {
                if actor.0 == 0 {
                    return Err(Error::Unavailable);
                }
                let row = plan.projected.knowledge.entry(actor).or_insert_with(|| {
                    let before = crate::storage::knowledge(store, actor).cloned();
                    (before.clone(), before.unwrap_or_default())
                });
                crate::institutions::apply_case_learning(
                    &mut row.1,
                    &reaction.case,
                    reaction.concept,
                    actor,
                    empathy,
                );
            }
            if let Some((actor, source)) = reaction.clear_exclusion {
                let row = plan.projected.exclusions.entry(actor).or_insert_with(|| {
                    let before = crate::storage::exclusions(store, actor).cloned();
                    (before.clone(), before.unwrap_or_default())
                });
                row.1.stores.remove(&source);
            }
            plan.projected.cases.insert(id, reaction.case);
        }
        through
            .checked_add(plan.event_count())
            .ok_or(Error::Overflow)?;
        plan.check_source(self, store, at, facts)?;
        Ok(plan)
    }
}
impl PreparedReceiptReactions {
    pub fn cases(&self) -> impl Iterator<Item = (CaseId, PersonId)> + '_ {
        self.projected
            .cases
            .iter()
            .map(|(id, case)| (*id, case.subject))
    }
    pub fn allocations(&self) -> u64 {
        self.projected.cases.len() as u64
    }
    pub fn event_count(&self) -> u64 {
        self.allocations()
    }
    fn read_known_cases(&mut self, store: &Store, actor: PersonId) {
        let before = crate::storage::knowledge(store, actor).cloned();
        if let Some(known) = &before {
            for id in known.cases.iter().rev().take(64) {
                self.read_case(store, *id);
            }
        }
        self.knowledge_reads.entry(actor).or_insert(before);
        self.reads.protect::<Knowledge>(StableId(actor.0));
    }
    fn read_case(&mut self, store: &Store, id: CaseId) {
        self.case_reads
            .entry(id)
            .or_insert_with(|| crate::storage::case(store, id).cloned());
        self.reads.protect::<SocialCaseView>(StableId(id.0));
    }
    fn read_selection(&mut self, store: &Store, selector: Selector) {
        let selected = selector.select(store, None);
        let institution =
            selected.and_then(|(id, _)| crate::storage::institution(store, id).cloned());
        let concept = selected.and_then(|(_, id)| store.get::<Concept>(StableId(id.0)).cloned());
        if let Some((institution, concept)) = selected {
            self.reads.protect::<Institution>(StableId(institution.0));
            self.reads.protect::<Concept>(StableId(concept.0));
        }
        self.selections.push(SelectionSource {
            selector,
            selected,
            institution,
            concept,
        });
    }
    fn check_source(
        &self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        facts: &impl ReceiptObserverFacts,
    ) -> Result<(), Error> {
        if runtime.last_event != self.before_cursor
            || at != self.at
            || self.subjects.iter().any(|(before, reads)| {
                facts.subject(before.actor) != Some(*before)
                    || facts.subject_reads(before.actor) != *reads
            })
            || self.observations.iter().any(|source| {
                facts.observe(&source.case, source.public).as_ref() != Ok(&source.before)
            })
            || self
                .knowledge_reads
                .iter()
                .any(|(actor, before)| crate::storage::knowledge(store, *actor) != before.as_ref())
            || self
                .exclusion_reads
                .iter()
                .any(|(actor, before)| crate::storage::exclusions(store, *actor) != before.as_ref())
            || self
                .case_reads
                .iter()
                .any(|(id, before)| crate::storage::case(store, *id) != before.as_ref())
            || self.projected.knowledge.iter().any(|(actor, (before, _))| {
                crate::storage::knowledge(store, *actor) != before.as_ref()
            })
            || self
                .projected
                .exclusions
                .iter()
                .any(|(actor, (before, _))| {
                    crate::storage::exclusions(store, *actor) != before.as_ref()
                })
            || self
                .projected
                .cases
                .keys()
                .any(|id| store.contains(StableId(id.0)))
            || self.selections.iter().any(|source| {
                source.selector.select(store, None) != source.selected
                    || source.selected.is_some_and(|(id, concept)| {
                        crate::storage::institution(store, id) != source.institution.as_ref()
                            || store.get::<Concept>(StableId(concept.0)) != source.concept.as_ref()
                    })
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        facts: &impl ReceiptObserverFacts,
        batch: &mut Batch,
    ) -> Result<ReceiptReactionsFinalization, Error> {
        self.check_source(runtime, store, at, facts)?;
        let mut writes = ReadFootprint::new();
        let mut reads = self.reads.clone();
        for (actor, (_, after)) in &self.projected.knowledge {
            let id = StableId(actor.0);
            if batch.touches::<Knowledge>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, after.clone());
            writes.protect::<Knowledge>(id);
            reads.remove::<Knowledge>(id);
        }
        for (actor, (_, after)) in &self.projected.exclusions {
            let id = StableId(actor.0);
            if batch.touches::<Exclusions>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, after.clone());
            writes.protect::<Exclusions>(id);
            reads.remove::<Exclusions>(id);
        }
        for (id, case) in &self.projected.cases {
            let entity = StableId(id.0);
            if batch.edit_count(entity) != 0 {
                return Err(Error::Unavailable);
            }
            batch.insert(entity, case.clone());
            writes.protect::<SocialCaseView>(entity);
        }
        let receipt = ReceiptReactionsFinalization {
            key: batch.key(),
            source: self,
            writes,
            reads,
        };
        receipt.validate_batch(runtime, store, at, facts, batch)?;
        Ok(receipt)
    }
}
impl ReceiptReactionsFinalization {
    pub fn validate_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        facts: &impl ReceiptObserverFacts,
        batch: &Batch,
    ) -> Result<(), Error> {
        self.source.check_source(runtime, store, at, facts)?;
        if self.key != batch.key()
            || !self.writes.has_one_edit_per_row(batch)
            || !self.reads.is_untouched(batch)
            || self.source.observations.iter().any(|source| {
                !facts
                    .observe_batch(batch, &source.case, source.public)
                    .is_ok_and(|after| after.observers == source.before.observers)
            })
            || self
                .source
                .projected
                .cases
                .keys()
                .any(|id| batch.edit_count(StableId(id.0)) != 1)
            || self
                .source
                .selections
                .iter()
                .any(|source| source.selector.select(store, Some(batch)) != source.selected)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn finish(self, runtime: &mut Runtime) {
        runtime.last_event = self.source.through;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_fixtures::{case, institution, labor, seed_institution, world};
    struct Facts {
        subject: SubjectFacts,
        observers: BTreeMap<PersonId, u16>,
    }
    impl ReceiptObserverFacts for Facts {
        fn subject(&self, actor: PersonId) -> Option<SubjectFacts> {
            (actor == self.subject.actor).then_some(self.subject)
        }
        fn subject_reads(&self, actor: PersonId) -> ReadFootprint {
            let mut reads = ReadFootprint::new();
            reads.protect::<u8>(StableId(actor.0));
            reads
        }
        fn observe(&self, case: &SocialCaseView, _: bool) -> Result<ReceiptObservation, Error> {
            assert_eq!(case.id, CaseId(0));
            let mut reads = ReadFootprint::new();
            for actor in self.observers.keys() {
                reads.protect::<u8>(StableId(actor.0));
            }
            Ok(ReceiptObservation {
                observers: self.observers.clone(),
                reads,
            })
        }
        fn observe_batch(
            &self,
            batch: &Batch,
            case: &SocialCaseView,
            public: bool,
        ) -> Result<ReceiptObservation, Error> {
            let mut after = self.observe(case, public)?;
            for (id, body) in batch.insertions::<u8>() {
                if *body == 1 {
                    after.observers.insert(PersonId(id.0), 400);
                }
            }
            Ok(after)
        }
    }
    fn fixture() -> (Runtime, Store, Facts) {
        let (runtime, base) = world();
        let mut schema = base.schema().clone();
        schema.register::<u8>("test/body", |_| Ok(())).unwrap();
        schema
            .register::<String>("test/reject", |value| {
                if value == "reject" {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        let mut batch = Batch::new();
        batch.insert(StableId(1), 0u8).insert(StableId(2), 0u8);
        store.apply_batch(batch).unwrap();
        let facts = Facts {
            subject: SubjectFacts {
                actor: PersonId(1),
                area: Some(AreaId(3)),
                hunger: 300_000,
            },
            observers: BTreeMap::from([(PersonId(1), 500), (PersonId(2), 800)]),
        };
        (runtime, store, facts)
    }
    fn receipt_pair() -> Vec<SocialReceipt> {
        let asset = CaseAsset {
            area: Some(AreaId(3)),
            inventory: InventoryId(4),
            good: Good::Food,
        };
        vec![
            SocialReceipt::Performed {
                actor: PersonId(1),
                counterpart: PersonId(2),
                agreement: AgreementId(40),
                target: CooperationTarget::Labor {
                    site: SiteId(20),
                    asset,
                },
            },
            SocialReceipt::Defaulted {
                actor: PersonId(1),
                counterpart: PersonId(2),
                agreement: AgreementId(40),
                asset,
            },
        ]
    }
    fn inputs(
        receipts: Vec<SocialReceipt>,
        subject: SubjectFacts,
    ) -> Vec<(SocialReceipt, SubjectFacts)> {
        receipts
            .into_iter()
            .map(|receipt| (receipt, subject))
            .collect()
    }
    #[test]
    fn ordered_prefix_matches_legacy_sequential_personal_learning_and_reciprocity() {
        let (mut runtime, mut store, facts) = fixture();
        let mut row = institution(10, vec![labor()], InstitutionStatus::Active);
        row.applied.insert(
            ConceptId(1000),
            BTreeMap::from([(PersonId(1), AgreementId(40))]),
        );
        seed_institution(&mut store, row);
        let mut legacy = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
        let receipts = receipt_pair();
        let index = ReceiptIndex::new(&receipts);
        for (offset, receipt) in receipts.iter().enumerate() {
            let reaction = runtime
                .receipt_reaction(&legacy, receipt, facts.subject, SimTime(10), &index)
                .unwrap();
            let observation = facts.observe(&reaction.case, reaction.public).unwrap();
            runtime
                .commit_case_reaction(
                    &mut legacy,
                    reaction,
                    CaseId(50 + offset as u64),
                    &observation.observers,
                )
                .unwrap();
        }
        let plan = runtime
            .prepare_receipt_reactions(
                &store,
                &inputs(receipts, facts.subject),
                SimTime(10),
                CaseId(50),
                12,
                &facts,
            )
            .unwrap();
        assert_eq!(
            plan.cases().collect::<Vec<_>>(),
            vec![(CaseId(50), PersonId(1)), (CaseId(51), PersonId(1))]
        );
        assert_eq!((plan.allocations(), plan.event_count()), (2, 2));
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        store.apply_batch(batch).unwrap();
        finish.finish(&mut runtime);
        assert_eq!(store.snapshot().unwrap(), legacy.snapshot().unwrap());
        assert!(crate::storage::case(&store, CaseId(51)).unwrap().reciprocal);
        assert_eq!(
            crate::storage::knowledge(&store, PersonId(1))
                .unwrap()
                .values[&ConceptId(1000)]
                .weight,
            200
        );
        assert_eq!(runtime.last_event, 12);
    }
    #[test]
    fn support_clears_once_and_whole_prefix_index_suppresses_duplicate_production() {
        let (mut runtime, mut store, facts) = fixture();
        crate::storage::insert_case(
            &mut store,
            CaseId(30),
            case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
        )
        .unwrap();
        crate::storage::insert_exclusions(
            &mut store,
            PersonId(1),
            Exclusions {
                stores: BTreeMap::from([(InventoryId(4), CaseId(30))]),
            },
        )
        .unwrap();
        let asset = CaseAsset {
            area: Some(AreaId(3)),
            inventory: InventoryId(4),
            good: Good::Food,
        };
        let mut receipts = vec![SocialReceipt::Produced {
            actor: PersonId(1),
            site: SiteId(20),
            asset,
            custodian: Some(PersonId(2)),
        }];
        receipts.push(receipt_pair()[0]);
        receipts.extend([
            SocialReceipt::SupportReceived {
                actor: PersonId(1),
                source: InventoryId(4),
            },
            SocialReceipt::Ate {
                actor: PersonId(1),
                source: InventoryId(4),
            },
        ]);
        let plan = runtime
            .prepare_receipt_reactions(
                &store,
                &inputs(receipts, facts.subject),
                SimTime(10),
                CaseId(50),
                9,
                &facts,
            )
            .unwrap();
        assert_eq!(plan.allocations(), 2);
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        store.apply_batch(batch).unwrap();
        finish.finish(&mut runtime);
        assert!(
            crate::storage::exclusions(&store, PersonId(1))
                .unwrap()
                .stores
                .is_empty()
        );
        assert_eq!(
            crate::storage::case(&store, CaseId(50)).unwrap().kind,
            SocialCaseKind::Cooperation
        );
        assert_eq!(
            crate::storage::case(&store, CaseId(51)).unwrap().outcome,
            SocialOutcome::Shared
        );
        assert!(crate::storage::case(&store, CaseId(52)).is_none());
    }
    #[test]
    fn final_guard_protects_cursor_subject_observers_fresh_ids_and_scoped_native_rows() {
        let (mut runtime, mut store, mut facts) = fixture();
        let receipt = inputs(receipt_pair(), facts.subject);
        let before = store.snapshot().unwrap();
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &Batch::new())
                .is_err()
        );
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(11), &facts, &batch)
                .is_err()
        );
        facts.subject.hunger += 1;
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        facts.subject.hunger -= 1;
        facts.observers.insert(PersonId(2), 801);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        facts.observers.insert(PersonId(2), 800);
        runtime.last_event = 1;
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        runtime.last_event = 0;
        batch
            .insert(StableId(77), Knowledge::default())
            .insert(StableId(77), 0u8);
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        batch.remove::<Knowledge>(StableId(1));
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        batch.insert(StableId(50), 0u8);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        batch.insert(StableId(1), "reject".to_owned());
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(runtime.last_event, 0);
        assert!(
            runtime
                .prepare_receipt_reactions(
                    &store,
                    &receipt,
                    SimTime(10),
                    CaseId(u64::MAX),
                    9,
                    &facts
                )
                .is_err()
        );
    }

    #[test]
    fn projected_observer_membership_rejects_a_new_witness_and_permits_unrelated_body_edits() {
        let (runtime, store, facts) = fixture();
        let receipts = inputs(vec![receipt_pair()[0]], facts.subject);
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipts, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        batch.insert(StableId(77), 2u8); // outside this case's observation membership
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        batch.insert(StableId(3), 1u8); // newly alive and locally eligible witness
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        assert!(crate::storage::case(&store, CaseId(50)).is_none());
        assert_eq!(runtime.last_event, 0);
    }
    #[test]
    fn staged_concept_cannot_create_a_new_sharing_interpretation() {
        let (runtime, mut store, facts) = fixture();
        crate::storage::insert_case(
            &mut store,
            CaseId(30),
            case(30, 1, SocialCaseKind::Exclusion, SocialOutcome::Unresolved),
        )
        .unwrap();
        crate::storage::insert_exclusions(
            &mut store,
            PersonId(1),
            Exclusions {
                stores: BTreeMap::from([(InventoryId(4), CaseId(30))]),
            },
        )
        .unwrap();
        let mut row = institution(10, vec![labor()], InstitutionStatus::Active);
        row.grants
            .insert(ConceptId(1000), BTreeSet::from([PersonId(1)]));
        seed_institution(&mut store, row);
        seed_institution(
            &mut store,
            institution(20, vec![labor()], InstitutionStatus::Active),
        );
        let receipts = inputs(
            vec![SocialReceipt::SupportReceived {
                actor: PersonId(1),
                source: InventoryId(4),
            }],
            facts.subject,
        );
        assert!(
            runtime
                .prepare_receipt_reactions(
                    &store,
                    &receipts,
                    SimTime(10),
                    CaseId(50),
                    u64::MAX,
                    &facts
                )
                .is_err()
        );
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipts, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        let mut unrelated = store.get::<Concept>(StableId(2000)).unwrap().clone();
        if let SocialEffect::Labor { millis, .. } = &mut unrelated.rule.effect {
            *millis += 1;
        }
        batch.insert(StableId(2000), unrelated);
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        let mut concept = store.get::<Concept>(StableId(1000)).unwrap().clone();
        concept.rule.effect = SocialEffect::Access {
            inventory: InventoryId(4),
            allowed: true,
        };
        batch.insert(StableId(1000), concept);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
    }
    #[test]
    fn institution_selection_guards_new_qualifying_membership_but_allows_unrelated_edits() {
        let (runtime, mut store, facts) = fixture();
        let mut selected = institution(10, vec![labor()], InstitutionStatus::Active);
        selected.applied.insert(
            ConceptId(1000),
            BTreeMap::from([(PersonId(1), AgreementId(40))]),
        );
        seed_institution(&mut store, selected);
        seed_institution(
            &mut store,
            institution(20, vec![labor()], InstitutionStatus::Active),
        );
        let receipt = inputs(vec![receipt_pair()[0]], facts.subject);
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        let mut unrelated = crate::storage::institution(&store, InstitutionId(20))
            .unwrap()
            .clone();
        unrelated.charter.purpose = "Unrelated change".into();
        batch.insert(StableId(20), unrelated);
        finish
            .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
            .unwrap();
        let mut earlier = crate::storage::institution(&store, InstitutionId(10))
            .unwrap()
            .clone();
        earlier.id = InstitutionId(5);
        batch.insert(StableId(5), earlier);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        let concept = store.get::<Concept>(StableId(1000)).unwrap().clone();
        batch.insert(StableId(1000), concept);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
        let plan = runtime
            .prepare_receipt_reactions(&store, &receipt, SimTime(10), CaseId(50), 9, &facts)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&runtime, &store, SimTime(10), &facts, &mut batch)
            .unwrap();
        batch.insert(StableId(1), 0u8);
        assert!(
            finish
                .validate_batch(&runtime, &store, SimTime(10), &facts, &batch)
                .is_err()
        );
    }
}
