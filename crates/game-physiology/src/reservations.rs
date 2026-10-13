//! Physical intake claims expressed independently of any activity representation.
use game_core::{Good, InventoryId, PersonId, Quantity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservationActivity {
    Other,
    Eat {
        source: InventoryId,
    },
    Care {
        source: InventoryId,
        good: Good,
    },
    Load {
        source: InventoryId,
        quantity: Quantity,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct IntakeDoses {
    pub food: Option<Quantity>,
    pub water: Option<Quantity>,
}
impl IntakeDoses {
    pub fn care(self, good: Good) -> Option<Quantity> {
        match good {
            Good::Food => self.food,
            Good::Water => self.water,
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalReservation {
    pub actor: PersonId,
    pub source: InventoryId,
    pub quantity: Quantity,
}
pub fn stock_reservations(
    activities: impl IntoIterator<Item = (PersonId, ReservationActivity)>,
    doses: IntakeDoses,
    source: Option<InventoryId>,
) -> Vec<PhysicalReservation> {
    activities
        .into_iter()
        .filter_map(|(actor, activity)| {
            let (inventory, quantity) = match activity {
                ReservationActivity::Eat { source } => (source, doses.food.unwrap_or(Quantity(0))),
                ReservationActivity::Care { source, good } => {
                    (source, doses.care(good).unwrap_or(Quantity(0)))
                }
                ReservationActivity::Load { source, quantity } => (source, quantity),
                ReservationActivity::Other => return None,
            };
            source
                .is_none_or(|wanted| inventory == wanted)
                .then_some(PhysicalReservation {
                    actor,
                    source: inventory,
                    quantity,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intake_and_load_reservations_use_their_own_doses_and_filter_source() {
        let doses = IntakeDoses {
            food: Some(Quantity(7)),
            water: Some(Quantity(3)),
        };
        let claims = stock_reservations(
            [
                (
                    PersonId(1),
                    ReservationActivity::Eat {
                        source: InventoryId(1),
                    },
                ),
                (
                    PersonId(2),
                    ReservationActivity::Care {
                        source: InventoryId(1),
                        good: Good::Food,
                    },
                ),
                (
                    PersonId(3),
                    ReservationActivity::Care {
                        source: InventoryId(2),
                        good: Good::Water,
                    },
                ),
                (
                    PersonId(4),
                    ReservationActivity::Load {
                        source: InventoryId(1),
                        quantity: Quantity(9),
                    },
                ),
                (PersonId(5), ReservationActivity::Other),
            ],
            doses,
            Some(InventoryId(1)),
        );
        assert_eq!(
            claims
                .iter()
                .map(|claim| (claim.actor, claim.quantity))
                .collect::<Vec<_>>(),
            vec![
                (PersonId(1), Quantity(7)),
                (PersonId(2), Quantity(7)),
                (PersonId(4), Quantity(9))
            ]
        );
        assert_eq!(doses.care(Good::Herb), None);
        assert_eq!(
            IntakeDoses {
                water: None,
                ..doses
            }
            .care(Good::Water),
            None
        );
    }
}
