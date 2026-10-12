//! Closing ledger publication demand from owed terms and explicit possible
//! physical receipts. This is a resource bound, never performance or default
//! mutation: only the conserved closing operations can satisfy an obligation.
use crate::*;
#[derive(Clone, Copy, Debug)]
pub enum ClosingReceipt {
    Transfer {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
    },
    Production {
        actor: Option<PersonId>,
        site: SiteId,
        good: Good,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClosingReceiptDemand {
    pub defaults: u64,
    pub performed: u64,
    pub accrued: u64,
}
impl ClosingReceiptDemand {
    pub fn events(self) -> Result<u64, Error> {
        self.defaults
            .checked_add(self.performed)
            .and_then(|n| n.checked_add(self.accrued))
            .ok_or(Error::Overflow)
    }
}
impl Ledger<'_> {
    /// Includes current pending work and unreconciled physical receipts supplied
    /// by the coordinator along with the next interval's possible operations.
    pub fn closing_receipt_demand(
        &self,
        at: SimTime,
        receipts: &[ClosingReceipt],
        work: &[WorkAssignment],
    ) -> Result<ClosingReceiptDemand, Error> {
        let mut demand = ClosingReceiptDemand::default();
        let mut labor = self.pending_work().keys().collect::<BTreeSet<_>>();
        for obligation in self.obligations().values() {
            let outstanding = obligation.performed < obligation.amount();
            let future_share=receipts.iter().any(|receipt|matches!((receipt,&obligation.term),
                (ClosingReceipt::Production{site,good,..},AgreementTerm::OutputShare{site:owed,good:kind,..}) if site==owed && good==kind));
            if !obligation.defaulted
                && (outstanding || future_share)
                && obligation.term.deadline().is_some_and(|due| due <= at)
            {
                demand.defaults = demand.defaults.checked_add(1).ok_or(Error::Overflow)?;
            }
            if outstanding
                && work.iter().any(|assignment| match obligation.term {
                    AgreementTerm::Labor { debtor, site, .. } => {
                        assignment.actor == debtor && assignment.target == WorkTarget::Site(site)
                    }
                    AgreementTerm::ConstructionLabor {
                        debtor, project, ..
                    } => {
                        assignment.actor == debtor
                            && assignment.target == WorkTarget::Project(project)
                    }
                    _ => false,
                })
            {
                labor.insert(obligation.id);
            }
            let mut qualified = obligation.cargo.keys().copied().collect::<BTreeSet<_>>();
            if let AgreementTerm::Delivery { source, .. } = obligation.term {
                qualified.insert(source);
            }
            // A possible pickup may stage and deliver in the same closing
            // sequence. Reachability qualifies sources, never invents stock.
            let mut todo = qualified.iter().copied().collect::<Vec<_>>();
            while let Some(source) = todo.pop() {
                for receipt in receipts {
                    if let ClosingReceipt::Transfer {
                        actor,
                        source: from,
                        destination,
                    } = receipt
                        && *actor == obligation.term.parties()[0]
                        && *from == source
                        && qualified.insert(*destination)
                    {
                        todo.push(*destination);
                    }
                }
            }
            let mut performance_receipts = 0u64;
            for receipt in receipts {
                let performs = match (receipt, &obligation.term) {
                    (
                        ClosingReceipt::Transfer {
                            actor,
                            source,
                            destination,
                        },
                        AgreementTerm::Delivery {
                            debtor,
                            source: origin,
                            destination: goal,
                            ..
                        },
                    ) => {
                        outstanding
                            && actor == debtor
                            && destination == goal
                            && source != goal
                            && (source == origin || qualified.contains(source))
                    }
                    // Closing movement can stage qualified cargo and deliver it
                    // in another operation; destination/actor bind the receipt.
                    (
                        ClosingReceipt::Transfer {
                            actor, destination, ..
                        },
                        AgreementTerm::OutputShare {
                            debtor,
                            destination: goal,
                            ..
                        },
                    ) => (outstanding || future_share) && actor == debtor && destination == goal,
                    _ => false,
                };
                if performs {
                    performance_receipts =
                        performance_receipts.checked_add(1).ok_or(Error::Overflow)?;
                }
                if matches!((receipt,&obligation.term),(ClosingReceipt::Production{actor,site,good},AgreementTerm::OutputShare{debtor,site:owed,good:kind,..}) if site==owed && good==kind && actor.is_none_or(|actor|actor==*debtor))
                {
                    demand.accrued = demand.accrued.checked_add(1).ok_or(Error::Overflow)?;
                }
            }
            let owed = if future_share {
                u64::MAX
            } else {
                obligation.amount().saturating_sub(obligation.performed)
            };
            demand.performed = demand
                .performed
                .checked_add(performance_receipts.min(owed))
                .ok_or(Error::Overflow)?;
        }
        demand.performed = demand
            .performed
            .checked_add(labor.len() as u64)
            .ok_or(Error::Overflow)?;
        demand.events()?;
        Ok(demand)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_demand_scales_with_owed_receipts_not_population_or_institution_count() {
        let mut state = fixture();
        for id in 1..=1000 {
            insert_obligation(
                &mut state,
                ObligationId(id),
                Obligation {
                    id: ObligationId(id),
                    agreement: AgreementId(2000 + id),
                    term: AgreementTerm::Delivery {
                        debtor: PersonId(1),
                        creditor: PersonId(2),
                        source: InventoryId(3),
                        destination: InventoryId(4),
                        quantity: Quantity(1),
                        deadline: SimTime(1),
                    },
                    performed: 0,
                    accrued: 0,
                    share_remainder: 0,
                    share_scale: 0,
                    defaulted: false,
                    cargo: BTreeMap::new(),
                },
            )
            .unwrap();
        }
        assert_eq!(
            Ledger::new(&state)
                .closing_receipt_demand(SimTime(0), &[], &[])
                .unwrap()
                .defaults,
            0
        );
        assert_eq!(
            Ledger::new(&state)
                .closing_receipt_demand(SimTime(1), &[], &[])
                .unwrap()
                .defaults,
            1000
        );
        let demand = Ledger::new(&state)
            .closing_receipt_demand(
                SimTime(1),
                &[ClosingReceipt::Transfer {
                    actor: PersonId(1),
                    source: InventoryId(3),
                    destination: InventoryId(4),
                }],
                &[],
            )
            .unwrap();
        assert_eq!(demand.performed, 1000);
        assert_eq!(demand.events().unwrap(), 2000);
    }
    #[test]
    fn staged_receipts_and_pending_work_are_bounded_without_duplicate_flushes() {
        let mut state = fixture();
        insert_obligation(
            &mut state,
            ObligationId(1),
            Obligation {
                id: ObligationId(1),
                agreement: AgreementId(20),
                term: AgreementTerm::Delivery {
                    debtor: PersonId(1),
                    creditor: PersonId(2),
                    source: InventoryId(3),
                    destination: InventoryId(4),
                    quantity: Quantity(1),
                    deadline: SimTime(100),
                },
                performed: 0,
                accrued: 0,
                share_remainder: 0,
                share_scale: 0,
                defaulted: false,
                cargo: BTreeMap::new(),
            },
        )
        .unwrap();
        let staged = [
            ClosingReceipt::Transfer {
                actor: PersonId(1),
                source: InventoryId(3),
                destination: InventoryId(5),
            },
            ClosingReceipt::Transfer {
                actor: PersonId(1),
                source: InventoryId(5),
                destination: InventoryId(4),
            },
        ];
        assert_eq!(
            Ledger::new(&state)
                .closing_receipt_demand(SimTime(1), &staged, &[])
                .unwrap()
                .performed,
            1
        );
        let unmarked = [ClosingReceipt::Transfer {
            actor: PersonId(1),
            source: InventoryId(9),
            destination: InventoryId(4),
        }];
        assert_eq!(
            Ledger::new(&state)
                .closing_receipt_demand(SimTime(1), &unmarked, &[])
                .unwrap()
                .performed,
            0
        );
        insert_obligation(
            &mut state,
            ObligationId(2),
            Obligation {
                id: ObligationId(2),
                agreement: AgreementId(21),
                term: AgreementTerm::Labor {
                    debtor: PersonId(1),
                    creditor: PersonId(2),
                    site: SiteId(6),
                    millis: 100,
                    deadline: SimTime(100),
                },
                performed: 50,
                accrued: 0,
                share_remainder: 0,
                share_scale: 0,
                defaulted: false,
                cargo: BTreeMap::new(),
            },
        )
        .unwrap();
        state
            .insert_or_spawn(StableId((ObligationId(2)).0), PendingWork { millis: 50 })
            .unwrap();
        let assignment = WorkAssignment {
            actor: PersonId(1),
            target: WorkTarget::Site(SiteId(6)),
        };
        assert_eq!(
            Ledger::new(&state)
                .closing_receipt_demand(SimTime(1), &[], &[assignment, assignment])
                .unwrap()
                .performed,
            1
        );
    }
}
/// Raw physical state for a potential closing work interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosingWorkFact {
    pub actor: PersonId,
    pub alive: bool,
    pub health: u64,
    pub target: Option<WorkTarget>,
}
pub fn closing_work_assignments(
    now: SimTime,
    at: SimTime,
    facts: &[ClosingWorkFact],
) -> Vec<WorkAssignment> {
    facts
        .iter()
        .filter(|fact| at > now && fact.alive && fact.health > 0)
        .filter_map(|fact| {
            fact.target.map(|target| WorkAssignment {
                actor: fact.actor,
                target,
            })
        })
        .collect()
}
pub fn transition_event_reserve(store: &Store, allocated: u64) -> Result<u64, Error> {
    allocated
        .checked_add(pending_works(store).count() as u64)
        .and_then(|count| count.checked_add(2))
        .ok_or(Error::Overflow)
}
#[cfg(test)]
mod closing_work_tests {
    use super::*;
    #[test]
    fn only_actual_eligible_future_work_reserves_an_exclusive_interval() {
        let site = Some(WorkTarget::Site(SiteId(20)));
        let facts = [
            ClosingWorkFact {
                actor: PersonId(1),
                alive: true,
                health: 1,
                target: site,
            },
            ClosingWorkFact {
                actor: PersonId(2),
                alive: false,
                health: 1,
                target: site,
            },
            ClosingWorkFact {
                actor: PersonId(3),
                alive: true,
                health: 0,
                target: site,
            },
            ClosingWorkFact {
                actor: PersonId(4),
                alive: true,
                health: 1,
                target: None,
            },
        ];
        assert!(closing_work_assignments(SimTime(1), SimTime(1), &facts).is_empty());
        assert_eq!(
            closing_work_assignments(SimTime(1), SimTime(2), &facts),
            vec![WorkAssignment {
                actor: PersonId(1),
                target: WorkTarget::Site(SiteId(20))
            }]
        );
    }
}
