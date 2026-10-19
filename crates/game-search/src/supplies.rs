use super::*;
use game_information::{Ask, Information, Topic, Want};
use game_knowledge::{StoreLocation, known, location_key};
#[derive(Clone, Serialize, Deserialize)]
pub struct FindGoods {
    pub person: PersonId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for FindGoods {
    const KEY: &'static str = "search.find-goods";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsTrails(pub Table<PersonId, Table<GoodId, BTreeSet<AreaId>>>);
impl Resource for GoodsTrails {
    const KEY: &'static str = "search.goods";
    fn validate(&self) -> Result<()> {
        require(
            self.0
                .values()
                .all(|row| row.len() <= 32 && row.values().all(|v| v.len() <= 64)),
            "invalid goods search trail",
        )
    }
}
/// Optional acquisition fallback for owners whose local work planning found no solution.
pub fn find(
    tx: &mut Transaction<'_>,
    person: PersonId,
    good: GoodId,
    quantity: u64,
) -> Result<game_choice::Outcome> {
    if tx.registered::<FindGoods>() {
        tx.call(FindGoods {
            person,
            good,
            quantity,
        })
    } else {
        Ok(game_choice::Outcome::Reconsider)
    }
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.resource::<GoodsTrails>()?;
    b.subscribe::<game_physiology::Died>("search.death", |tx, e| {
        tx.edit::<GoodsTrails>()?.0.remove(&e.person);
        tx.edit::<Searches>()?.0.remove(&e.person);
        Ok(())
    })?;
    b.request::<FindGoods>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            game_vitality::can_manipulate(tx, r.person),
            "supply search exceeds motor capacity",
        )?;
        require(r.quantity > 0, "supply search needs a positive quantity")?;
        let here = game_transport::area(tx.get()?, r.person)?;
        let mut sources = vec![];
        if let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&r.person) {
            for (key, fact) in facts
                .iter()
                .filter(|(key, fact)| Topic::Supplies(r.good).matches(key, fact.value))
            {
                let (area, inventory) = if let Some(id) = key
                    .strip_prefix("store:")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    let id = InventoryId(id);
                    let Some(location) = known::<StoreLocation>(facts, &location_key(id)) else {
                        continue;
                    };
                    if location.carrier == Some(r.person) {
                        continue;
                    }
                    (location.area, Some(id))
                } else {
                    let Some(patch) = fact.value.decode::<game_ecology::Patch>() else {
                        continue;
                    };
                    (patch.area, None)
                };
                let Ok(route) = game_knowledge::route(facts, here, area) else {
                    continue;
                };
                sources.push((route.duration, area, inventory, route.areas.get(1).copied()));
            }
        }
        sources.sort_unstable();
        for (_, area, inventory, next) in sources {
            if area != here {
                if let Some(destination) = next {
                    tx.call(game_transport::Move {
                        person: r.person,
                        destination,
                    })?;
                    return Ok(game_choice::Outcome::Performed);
                }
            } else if let Some(inventory) = inventory {
                // Inspect only after reaching the remembered place. A moved or depleted
                // store leaves evidence, rather than revealing where it went.
                let _ = tx.attempt(game_knowledge::ObserveStore {
                    person: r.person,
                    inventory,
                });
                let quantity = tx
                    .get::<game_knowledge::Knowledge>()?
                    .0
                    .get(&r.person)
                    .and_then(|facts| {
                        known::<game_knowledge::Stock>(facts, &game_knowledge::store_key(inventory))
                    })
                    .and_then(|stock| stock.get(&r.good).copied())
                    .unwrap_or(0)
                    .min(r.quantity);
                if quantity == 0 {
                    continue;
                }
                let packs: Vec<_> = tx
                    .get::<game_cargo::Cargo>()?
                    .0
                    .iter()
                    .filter(|(_, i)| i.custody == game_cargo::Custody::Carried(r.person))
                    .map(|(id, _)| *id)
                    .collect();
                let reluctance = game_property::ClaimCosts::new(tx, r.person).cost(inventory);
                // Expected social consequences compete with this ordinary supply goal.
                if reluctance > 60 {
                    continue;
                }
                for pack in packs {
                    if tx
                        .attempt(game_property::Take {
                            actor: r.person,
                            unauthorized: true,
                            transfer: game_cargo::Transfer {
                                from: inventory,
                                to: pack,
                                good: r.good,
                                quantity,
                            },
                        })
                        .is_ok()
                    {
                        tx.call(game_knowledge::ObserveStore {
                            person: r.person,
                            inventory: pack,
                        })?;
                        tx.call(game_knowledge::ObserveStore {
                            person: r.person,
                            inventory,
                        })?;
                        return Ok(game_choice::Outcome::Performed);
                    }
                }
            }
        }
        if !tx.registered::<Want>() {
            return Ok(game_choice::Outcome::Reconsider);
        }
        let topic = Topic::Supplies(r.good);
        let new_interest = tx
            .get::<Information>()?
            .interests
            .get(&r.person)
            .is_none_or(|row| row.iter().all(|i| i.topic != topic));
        tx.call(Want {
            person: r.person,
            topic: topic.clone(),
            importance: 60,
        })?;
        let asked = tx.get::<Information>()?.interests[&r.person]
            .iter()
            .find(|i| i.topic == topic)
            .unwrap()
            .asked
            .clone();
        let peer = tx
            .get::<game_social_life::SocialLife>()
            .ok()
            .and_then(|s| s.residents.get(&here))
            .into_iter()
            .flat_map(|row| row.keys())
            .filter(|p| asked.len() < 64 && **p != r.person && !asked.contains(p))
            .take(16)
            .max_by_key(|other| {
                (
                    game_social_life::affinity(tx, r.person, **other),
                    std::cmp::Reverse(**other),
                )
            })
            .copied();
        if let Some(other) = peer {
            tx.call(game_choice::Undertake {
                person: r.person,
                kind: "ask-for-supplies".into(),
                duration: SimDuration(2 * MINUTE.0),
                request: Pending::new(Ask {
                    person: r.person,
                    other,
                    topic,
                })?,
            })?;
            return Ok(game_choice::Outcome::Performed);
        }
        let trails = tx.edit::<GoodsTrails>()?.0.entry(r.person).or_default();
        if trails.len() >= 32 && !trails.contains_key(&r.good) {
            return Ok(new_interest.into());
        }
        let visited = trails.entry(r.good).or_default();
        if visited.len() >= 64 {
            return Ok(new_interest.into());
        }
        visited.insert(here);
        let visited = visited.clone();
        let next = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&r.person)
            .and_then(|facts| {
                game_knowledge::with_prefix(facts, "roads:")
                    .filter_map(|(_, fact)| fact.value.decode::<game_land::Area>())
                    .flat_map(|area| area.links.keys().copied().collect::<Vec<_>>())
                    .filter(|area| !visited.contains(area))
                    .filter_map(|area| {
                        let route = game_knowledge::route(facts, here, area).ok()?;
                        Some((route.duration, area, *route.areas.get(1)?))
                    })
                    .min()
            });
        if let Some((_, _, destination)) = next {
            tx.call(game_transport::Move {
                person: r.person,
                destination,
            })?;
            return Ok(game_choice::Outcome::Performed);
        }
        // Recognizing a new information need is useful once. On later attempts,
        // the chooser can prefer consulting a record or pursuing another goal.
        Ok(new_interest.into())
    })
}
