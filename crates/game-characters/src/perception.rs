//! Personal observation from local physical facts, without world or command access.
use game_core::*;

#[derive(Clone, Copy, Debug)]
pub struct StockObservationFacts {
    pub inventory: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
}

/// Stationary area observation includes ground stock before the actor's cargo.
/// Road observation includes only that cargo. Supplied rows remain lazy so road
/// observation does not read ground inventory balances or access rights.
pub fn observe_stock_facts(
    actor: PersonId,
    area: AreaId,
    at_area: bool,
    now: SimTime,
    local_ground: impl IntoIterator<Item = StockObservationFacts>,
    own_carried: impl IntoIterator<Item = StockObservationFacts>,
) -> Vec<StockKnowledge> {
    at_area
        .then_some(local_ground)
        .into_iter()
        .flatten()
        .chain(own_carried)
        .map(|row| StockKnowledge {
            inventory: row.inventory,
            area,
            good: row.good,
            quantity: row.quantity,
            accessible: row.accessible,
            observed_at: now,
            learned_at: now,
            source: KnowledgeSource::Observed(actor),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fact(inventory: u64, quantity: u64, accessible: bool) -> StockObservationFacts {
        StockObservationFacts {
            inventory: InventoryId(inventory),
            good: Good::Food,
            quantity: Quantity(quantity),
            accessible,
        }
    }
    #[test]
    fn area_observation_preserves_ground_then_cargo_order_and_stamps_every_balance() {
        let observations = observe_stock_facts(
            PersonId(1),
            AreaId(2),
            true,
            SimTime(300),
            [fact(20, 0, false), fact(10, 500, true)],
            [fact(30, 100, false)],
        );
        assert_eq!(
            observations,
            vec![
                StockKnowledge {
                    inventory: InventoryId(20),
                    area: AreaId(2),
                    good: Good::Food,
                    quantity: Quantity(0),
                    accessible: false,
                    observed_at: SimTime(300),
                    learned_at: SimTime(300),
                    source: KnowledgeSource::Observed(PersonId(1))
                },
                StockKnowledge {
                    inventory: InventoryId(10),
                    area: AreaId(2),
                    good: Good::Food,
                    quantity: Quantity(500),
                    accessible: true,
                    observed_at: SimTime(300),
                    learned_at: SimTime(300),
                    source: KnowledgeSource::Observed(PersonId(1))
                },
                StockKnowledge {
                    inventory: InventoryId(30),
                    area: AreaId(2),
                    good: Good::Food,
                    quantity: Quantity(100),
                    accessible: false,
                    observed_at: SimTime(300),
                    learned_at: SimTime(300),
                    source: KnowledgeSource::Observed(PersonId(1))
                },
            ]
        );
    }
    #[test]
    fn road_observation_never_reads_ground_and_dates_only_own_cargo() {
        let ground = [fact(20, 500, true)]
            .into_iter()
            .inspect(|_| panic!("ground stock must not be read while on a road"));
        let observed = observe_stock_facts(
            PersonId(1),
            AreaId(2),
            false,
            SimTime(400),
            ground,
            [fact(30, 100, false)],
        );
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].inventory, InventoryId(30));
        assert_eq!(observed[0].area, AreaId(2));
        assert_eq!(observed[0].quantity, Quantity(100));
        assert!(!observed[0].accessible);
        assert_eq!(observed[0].observed_at, SimTime(400));
        assert_eq!(observed[0].learned_at, SimTime(400));
        assert_eq!(observed[0].source, KnowledgeSource::Observed(PersonId(1)));
        assert!(
            observe_stock_facts(
                PersonId(1),
                AreaId(2),
                false,
                SimTime(400),
                [fact(20, 500, true)],
                []
            )
            .is_empty()
        );
    }
}
