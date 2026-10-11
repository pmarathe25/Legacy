use crate::*;
use game_core::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptChange {
    Transfer {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Consume {
        inventory: InventoryId,
        quantity: Quantity,
    },
    Produce {
        outputs: Vec<(InventoryId, Quantity)>,
    },
    Born {
        person: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyReceipt {
    pub event: u64,
    pub change: ReceiptChange,
}
impl Runtime {
    /// `balances` contains touched or newly created stocks only. The host's
    /// inventory-count fast path avoids enumerating unchanged stock history.
    pub fn prepare_receipts(
        &self,
        store: &Store,
        receipts: &[PropertyReceipt],
        balances: &BTreeMap<InventoryId, Quantity>,
        through: u64,
    ) -> Result<PreparedProperty, Error> {
        if through < self.last_event {
            return Err(Error::Unavailable);
        }
        let mut previous = self.last_event;
        for receipt in receipts {
            if receipt.event <= previous || receipt.event > through {
                return Err(Error::Unavailable);
            }
            previous = receipt.event;
        }
        let mut change = PreparedProperty::new();
        change.cursor = Some((self.last_event, through));
        for (inventory, closing) in balances
            .iter()
            .filter(|(id, _)| !portions(store, **id).is_some())
        {
            let mut opening = i128::from(closing.0);
            for receipt in receipts {
                let delta = match &receipt.change {
                    ReceiptChange::Transfer {
                        source,
                        destination,
                        quantity,
                    } => {
                        i128::from(quantity.0)
                            * (i128::from(source == inventory)
                                - i128::from(destination == inventory))
                    }
                    ReceiptChange::Consume {
                        inventory: id,
                        quantity,
                    } if id == inventory => i128::from(quantity.0),
                    ReceiptChange::Produce { outputs } => -outputs
                        .iter()
                        .filter(|(id, _)| id == inventory)
                        .try_fold(0i128, |total, (_, q)| {
                            total.checked_add(i128::from(q.0)).ok_or(Error::Overflow)
                        })?,
                    _ => 0,
                };
                opening = opening.checked_add(delta).ok_or(Error::Overflow)?;
            }
            let opening = u64::try_from(opening).map_err(|_| Error::Unavailable)?;
            change.set_portions(
                store,
                *inventory,
                if opening == 0 {
                    BTreeMap::new()
                } else {
                    BTreeMap::from([(ClaimId(0), opening)])
                },
            );
        }
        for receipt in receipts {
            match &receipt.change {
                ReceiptChange::Transfer {
                    source,
                    destination,
                    quantity,
                } => {
                    let parts = change
                        .current_portions(store, *source)
                        .ok_or(Error::Unavailable)?;
                    let (left, moved) = split_portions(parts, quantity.0)?;
                    change.set_portions(store, *source, left);
                    let mut target = change
                        .current_portions(store, *destination)
                        .cloned()
                        .ok_or(Error::Unavailable)?;
                    for (marker, q) in moved {
                        let total = target.entry(marker).or_default();
                        *total = total.checked_add(q).ok_or(Error::Overflow)?;
                    }
                    change.set_portions(store, *destination, target);
                }
                ReceiptChange::Consume {
                    inventory,
                    quantity,
                } => {
                    let parts = change
                        .current_portions(store, *inventory)
                        .ok_or(Error::Unavailable)?;
                    let (left, _) = split_portions(parts, quantity.0)?;
                    change.set_portions(store, *inventory, left);
                }
                ReceiptChange::Produce { outputs } => {
                    for (inventory, quantity) in outputs {
                        if quantity.0 == 0 {
                            continue;
                        }
                        let mut parts = change
                            .current_portions(store, *inventory)
                            .cloned()
                            .ok_or(Error::Unavailable)?;
                        let total = parts.entry(ClaimId(0)).or_default();
                        *total = total.checked_add(quantity.0).ok_or(Error::Overflow)?;
                        change.set_portions(store, *inventory, parts);
                    }
                }
                ReceiptChange::Born { person } => {
                    if person.0 == 0 {
                        return Err(Error::Unavailable);
                    }
                    change.born.insert(*person);
                }
            }
        }
        for (inventory, quantity) in balances {
            let total = change
                .current_portions(store, *inventory)
                .ok_or(Error::Unavailable)?
                .values()
                .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?;
            if total != quantity.0 {
                return Err(Error::Unavailable);
            }
        }
        Ok(change)
    }
}
