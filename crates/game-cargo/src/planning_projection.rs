//! Immutable stock reservations and toolkit allocations for one planning batch.
//! The host supplies actual activity facts; these rules neither grant access nor
//! replace stock, activity, or permission state.
use game_core::{Error, InventoryId, PersonId, Quantity};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub actor: PersonId,
    pub source: InventoryId,
    pub quantity: Quantity,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Reserved {
    total: u128,
    by_actor: BTreeMap<PersonId, u128>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReservationTotals {
    sources: BTreeMap<InventoryId, Reserved>,
}

impl ReservationTotals {
    /// Multiple claims by one actor may come from different physical owners.
    /// Keep their full sum so exclusion happens before available stock clips it.
    pub fn prepare(claims: impl IntoIterator<Item = Reservation>) -> Result<Self, Error> {
        let mut projection = Self::default();
        for claim in claims {
            if claim.actor.0 == 0 || claim.source.0 == 0 {
                return Err(Error::Unavailable);
            }
            if claim.quantity.0 == 0 {
                continue;
            }
            let source = projection.sources.entry(claim.source).or_default();
            let amount = u128::from(claim.quantity.0);
            source.total = source.total.checked_add(amount).ok_or(Error::Overflow)?;
            let actor = source.by_actor.entry(claim.actor).or_default();
            *actor = actor.checked_add(amount).ok_or(Error::Overflow)?;
        }
        Ok(projection)
    }

    /// Quantity remains an authoritative caller fact, never a shadow stock.
    pub fn available(
        &self,
        source: InventoryId,
        actual_quantity: Quantity,
        excluding_actor: PersonId,
    ) -> Quantity {
        let reserved = self.sources.get(&source).map_or(0, |claims| {
            claims.total - claims.by_actor.get(&excluding_actor).copied().unwrap_or(0)
        });
        Quantity(u128::from(actual_quantity.0).saturating_sub(reserved) as u64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolSlots {
    pub tool: InventoryId,
    pub quantity: Quantity,
    pub last_granted: Option<PersonId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolUse {
    pub actor: PersonId,
    pub tool: InventoryId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Allocation {
    slots: u64,
    ranks: BTreeMap<PersonId, u64>,
    users: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolAllocations {
    tools: BTreeMap<InventoryId, Allocation>,
}

impl ToolAllocations {
    /// Prospective consumption cannot spend the toolkit needed by its own
    /// production, or displace other actors' already allocated physical units.
    pub fn allows_after_consumption(
        &self,
        actor: PersonId,
        tool: InventoryId,
        actual_quantity: Quantity,
        consumed: Quantity,
    ) -> bool {
        actor.0 > 0
            && actual_quantity
                .0
                .checked_sub(consumed.0)
                .is_some_and(|after| {
                    after / 1_000
                        > self
                            .allocated_users(tool)
                            .filter(|user| *user != actor)
                            .count() as u64
                })
    }
    /// Each use describes one live actor's physically selected toolkit. The host
    /// excludes dead, misplaced, and non-tool selections, but does not erase a
    /// physical use merely because somebody withdrew access permission.
    pub fn prepare(
        tools: impl IntoIterator<Item = ToolSlots>,
        uses: impl IntoIterator<Item = ToolUse>,
    ) -> Result<Self, Error> {
        let mut facts = BTreeMap::new();
        for tool in tools {
            if tool.tool.0 == 0
                || tool.last_granted.is_some_and(|actor| actor.0 == 0)
                || facts.insert(tool.tool, tool).is_some()
            {
                return Err(Error::Unavailable);
            }
        }
        let mut selected = BTreeMap::new();
        let mut users = BTreeMap::<InventoryId, Vec<PersonId>>::new();
        for usage in uses {
            if usage.actor.0 == 0
                || !facts.contains_key(&usage.tool)
                || selected.insert(usage.actor, usage.tool).is_some()
            {
                return Err(Error::Unavailable);
            }
            users.entry(usage.tool).or_default().push(usage.actor);
        }
        let mut projection = Self::default();
        for (id, tool) in facts {
            let mut actors = users.remove(&id).unwrap_or_default();
            actors.sort_by_key(|actor| {
                (tool.last_granted.is_some_and(|last| *actor <= last), *actor)
            });
            let count = u64::try_from(actors.len()).map_err(|_| Error::Overflow)?;
            let ranks = actors
                .into_iter()
                .enumerate()
                .map(|(rank, actor)| {
                    u64::try_from(rank)
                        .map(|rank| (actor, rank))
                        .map_err(|_| Error::Overflow)
                })
                .collect::<Result<_, _>>()?;
            projection.tools.insert(
                id,
                Allocation {
                    slots: tool.quantity.0 / 1_000,
                    ranks,
                    users: count,
                },
            );
        }
        Ok(projection)
    }

    /// An existing physical user keeps its cursor rank; a prospective user may
    /// take a free slot without displacing any currently selected user.
    pub fn allows(&self, actor: PersonId, tool: InventoryId) -> bool {
        actor.0 > 0
            && self.tools.get(&tool).is_some_and(|allocation| {
                allocation
                    .ranks
                    .get(&actor)
                    .map_or(allocation.users < allocation.slots, |rank| {
                        *rank < allocation.slots
                    })
            })
    }

    /// A finishing actor's own selection is released before a prospective
    /// availability check; other selected users keep their physical use.
    pub fn allows_after_release(&self, actor: PersonId, tool: InventoryId) -> bool {
        actor.0 > 0
            && self.tools.get(&tool).is_some_and(|allocation| {
                allocation.users - u64::from(allocation.ranks.contains_key(&actor))
                    < allocation.slots
            })
    }

    pub fn allocated_users(&self, tool: InventoryId) -> impl Iterator<Item = PersonId> + '_ {
        self.tools.get(&tool).into_iter().flat_map(|allocation| {
            allocation
                .ranks
                .iter()
                .filter_map(|(actor, rank)| (*rank < allocation.slots).then_some(*actor))
        })
    }

    pub fn allocated_actors(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.tools.values().flat_map(|allocation| {
            allocation
                .ranks
                .iter()
                .filter_map(|(actor, rank)| (*rank < allocation.slots).then_some(*actor))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(actor: u64, source: u64, quantity: u64) -> Reservation {
        Reservation {
            actor: PersonId(actor),
            source: InventoryId(source),
            quantity: Quantity(quantity),
        }
    }
    fn tool(id: u64, quantity: u64, cursor: Option<u64>) -> ToolSlots {
        ToolSlots {
            tool: InventoryId(id),
            quantity: Quantity(quantity),
            last_granted: cursor.map(PersonId),
        }
    }
    fn usage(actor: u64, tool: u64) -> ToolUse {
        ToolUse {
            actor: PersonId(actor),
            tool: InventoryId(tool),
        }
    }

    #[test]
    fn excludes_all_own_claims_across_physical_and_health_owners() {
        let totals = ReservationTotals::prepare([
            claim(1, 10, 1_000),
            claim(1, 10, 750),
            claim(2, 10, 2_000),
            claim(2, 11, 500),
        ])
        .unwrap();
        assert_eq!(
            totals.available(InventoryId(10), Quantity(3_000), PersonId(1)),
            Quantity(1_000)
        );
        assert_eq!(
            totals.available(InventoryId(10), Quantity(3_000), PersonId(2)),
            Quantity(1_250)
        );
        assert_eq!(
            totals.available(InventoryId(10), Quantity(3_000), PersonId(3)),
            Quantity(0)
        );
        assert_eq!(
            totals.available(InventoryId(11), Quantity(600), PersonId(1)),
            Quantity(100)
        );
        assert_eq!(
            totals.available(InventoryId(12), Quantity(600), PersonId(1)),
            Quantity(600)
        );
    }

    #[test]
    fn exclusion_precedes_clipping_even_when_total_exceeds_u64() {
        let totals = ReservationTotals::prepare([claim(1, 10, u64::MAX), claim(2, 10, 2)]).unwrap();
        assert_eq!(
            totals.available(InventoryId(10), Quantity(10), PersonId(1)),
            Quantity(8)
        );
        assert_eq!(
            totals.available(InventoryId(10), Quantity(10), PersonId(2)),
            Quantity(0)
        );
        let overbooked = ReservationTotals::prepare([
            claim(1, 10, u64::MAX),
            claim(2, 10, u64::MAX),
            claim(3, 10, u64::MAX),
        ])
        .unwrap();
        assert_eq!(
            overbooked.available(InventoryId(10), Quantity(u64::MAX), PersonId(1)),
            Quantity(0)
        );
    }

    #[test]
    fn reservation_order_and_zero_claims_do_not_change_answers() {
        let claims = [claim(2, 10, 3), claim(1, 10, 2), claim(1, 11, 4)];
        let expected = ReservationTotals::prepare(claims).unwrap();
        let reversed =
            ReservationTotals::prepare(claims.into_iter().rev().chain([claim(3, 12, 0)])).unwrap();
        assert_eq!(expected, reversed);
        assert!(matches!(
            ReservationTotals::prepare([claim(0, 10, 1)]),
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            ReservationTotals::prepare([claim(1, 0, 1)]),
            Err(Error::Unavailable)
        ));
    }

    #[test]
    fn cursor_wrap_selects_existing_users_without_prospective_displacement() {
        let allocated = ToolAllocations::prepare(
            [tool(10, 2_000, Some(2))],
            [usage(1, 10), usage(2, 10), usage(3, 10), usage(4, 10)],
        )
        .unwrap();
        assert!(!allocated.allows(PersonId(1), InventoryId(10)));
        assert!(!allocated.allows(PersonId(2), InventoryId(10)));
        assert!(allocated.allows(PersonId(3), InventoryId(10)));
        assert!(allocated.allows(PersonId(4), InventoryId(10)));
        assert!(!allocated.allows(PersonId(5), InventoryId(10)));
    }

    #[test]
    fn a_new_user_needs_a_whole_unselected_slot() {
        let allocated = ToolAllocations::prepare(
            [
                tool(10, 2_999, None),
                tool(11, 999, None),
                tool(12, 1_000, None),
            ],
            [usage(1, 10)],
        )
        .unwrap();
        assert!(allocated.allows(PersonId(1), InventoryId(10)));
        assert!(allocated.allows(PersonId(2), InventoryId(10)));
        assert!(!allocated.allows(PersonId(2), InventoryId(11)));
        assert!(allocated.allows(PersonId(2), InventoryId(12)));
        assert!(!allocated.allows(PersonId(2), InventoryId(13)));
        assert!(!allocated.allows(PersonId(0), InventoryId(12)));
    }

    #[test]
    fn a_physical_user_needs_no_permission_receipt_to_retain_its_slot() {
        let occupied = ToolAllocations::prepare([tool(10, 1_000, None)], [usage(1, 10)]).unwrap();
        assert!(occupied.allows(PersonId(1), InventoryId(10)));
        assert!(!occupied.allows(PersonId(2), InventoryId(10)));
        // On the next batch, actual cancellation/departure removes this use.
        let released = ToolAllocations::prepare([tool(10, 1_000, None)], []).unwrap();
        assert!(released.allows(PersonId(2), InventoryId(10)));
        // Physical removal of toolkit quantity also removes its capacity.
        let removed = ToolAllocations::prepare([tool(10, 0, None)], [usage(1, 10)]).unwrap();
        assert!(!removed.allows(PersonId(1), InventoryId(10)));
    }

    #[test]
    fn toolkit_input_order_cannot_change_fair_allocation() {
        let tools = [tool(10, 2_000, Some(2)), tool(11, 1_000, None)];
        let uses = [usage(3, 10), usage(1, 11), usage(4, 10), usage(2, 10)];
        assert_eq!(
            ToolAllocations::prepare(tools, uses).unwrap(),
            ToolAllocations::prepare(tools.into_iter().rev(), uses.into_iter().rev()).unwrap(),
        );
    }

    #[test]
    fn inconsistent_toolkit_input_fails_instead_of_partially_allocating() {
        assert!(matches!(
            ToolAllocations::prepare([tool(10, 1_000, None)], [usage(1, 11)]),
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            ToolAllocations::prepare([tool(10, 1_000, None), tool(10, 2_000, None)], []),
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            ToolAllocations::prepare(
                [tool(10, 1_000, None), tool(11, 1_000, None)],
                [usage(1, 10), usage(1, 11)]
            ),
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            ToolAllocations::prepare([tool(10, 1_000, None)], [usage(1, 10), usage(1, 10)]),
            Err(Error::Unavailable)
        ));
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;
    #[test]
    fn prospective_release_matches_an_actual_selection_removal_under_contention() {
        for slots in [0, 1, 2, 3] {
            for last in [None, Some(PersonId(1)), Some(PersonId(3))] {
                let tool = ToolSlots {
                    tool: InventoryId(10),
                    quantity: Quantity(slots * 1000),
                    last_granted: last,
                };
                let uses: Vec<_> = (1..=3)
                    .map(|actor| ToolUse {
                        actor: PersonId(actor),
                        tool: tool.tool,
                    })
                    .collect();
                let before = ToolAllocations::prepare([tool], uses.clone()).unwrap();
                for actor in [PersonId(1), PersonId(2), PersonId(3), PersonId(4)] {
                    let actual = ToolAllocations::prepare(
                        [tool],
                        uses.iter().copied().filter(|usage| usage.actor != actor),
                    )
                    .unwrap();
                    assert_eq!(
                        before.allows_after_release(actor, tool.tool),
                        actual.allows(actor, tool.tool)
                    );
                }
            }
        }
    }
}
