//! Physical marks on finite surfaces, independent of language and literacy.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    Pigment,
    Incision,
    Impression,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imprint {
    pub item: u64,
    pub method: Method,
    pub time_factor: u64,
    pub tool: Option<(InventoryId, GoodId)>,
}
#[derive(Clone, Debug)]
pub struct Supplies {
    pub item: Option<u64>,
    pub surface: GoodId,
    pub tool: Option<(InventoryId, GoodId)>,
    pub method: Method,
    pub time_factor: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surfaces(pub game_storage::Table<u64, ()>);
impl Resource for Surfaces {
    const KEY: &'static str = "inscription.surfaces";
}
fn rating(object: &game_objects::Object, prefix: &str) -> Option<u64> {
    object
        .tags
        .iter()
        .find_map(|s| s.strip_prefix(prefix)?.parse().ok())
}
/// Surfaces must be physically present; a tool may be carried separately.
pub fn supplies(
    tx: &Transaction<'_>,
    person: PersonId,
    inventory: InventoryId,
) -> Option<Supplies> {
    if !game_vitality::can_manipulate(tx, person) {
        return None;
    }
    let cargo = tx.get::<game_cargo::Cargo>().ok()?;
    let objects = tx.get::<game_objects::Objects>().ok()?;
    let items = tx.get::<game_items::Items>().ok()?;
    let surfaces = tx.get::<Surfaces>().ok()?;
    let area = game_transport::area(tx.get().ok()?, person).ok()?;
    if game_cargo::locality(tx, inventory).ok() != Some(area) {
        return None;
    }
    let stock = cargo.0.get(&inventory)?;
    let tools: Vec<_> = cargo
        .carried(person)
        .chain(std::iter::once(inventory))
        .flat_map(|i| {
            cargo.0[&i]
                .goods
                .iter()
                .filter(|(_, q)| **q > 0)
                .map(move |(g, _)| (i, *g))
        })
        .collect();
    let mut options = vec![];
    for (surface, quantity) in &stock.goods {
        let object = objects.by_id(*surface).ok()?;
        let Some(hardness) = rating(object, "mark-surface:") else {
            continue;
        };
        let identified = items.held.get(&inventory).and_then(|r| r.get(surface));
        let plain = quantity.saturating_sub(identified.map_or(0, |r| r.len() as u64));
        let item = if plain > 0 {
            None
        } else {
            let Some(id) =
                identified.and_then(|r| r.iter().find(|id| !surfaces.0.contains_key(*id)))
            else {
                continue;
            };
            Some(*id)
        };
        if object.tags.contains("mark-impressible") {
            options.push(Supplies {
                item,
                surface: *surface,
                tool: None,
                method: Method::Impression,
                time_factor: 2,
            });
        }
        for (i, good) in &tools {
            if *good == *surface {
                continue;
            }
            let tool = objects.by_id(*good).ok()?;
            let method = if tool.tags.contains("mark-pigment") {
                Some((Method::Pigment, 1))
            } else if rating(tool, "mark-cutting:").is_some_and(|r| r >= hardness) {
                Some((Method::Incision, 2 + hardness / 200))
            } else {
                None
            };
            if let Some((method, time_factor)) = method {
                options.push(Supplies {
                    item,
                    surface: *surface,
                    tool: Some((*i, *good)),
                    method,
                    time_factor,
                });
            }
        }
    }
    options
        .into_iter()
        .min_by_key(|o| (o.time_factor, o.surface, o.tool))
}
pub fn intact(tx: &Transaction<'_>, person: PersonId, imprint: &Imprint) -> bool {
    game_vitality::can_manipulate(tx, person)
        && (imprint.method != Method::Incision
            || imprint.tool.is_some_and(|(inventory, good)| {
                game_cargo::locality(tx, inventory).ok()
                    == game_transport::area(tx.get().unwrap(), person).ok()
                    && tx
                        .get::<game_cargo::Cargo>()
                        .is_ok_and(|c| c.0.get(&inventory).is_some_and(|i| i.amount(good) > 0))
            }))
        && game_items::exposed(tx, person, imprint.item, Some(person))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Make {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub label: String,
}
impl Request for Make {
    const KEY: &'static str = "inscription.make";
    type Response = Imprint;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("inscription", 1)?;
    b.resource::<Surfaces>()?;
    b.request::<Make>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let method = supplies(tx, r.person, r.inventory)
            .ok_or_else(|| Error::Rejected("no compatible surface and marking method".into()))?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "making marks".into(),
        })?;
        if let Some((inventory, good)) = method.tool {
            tx.call(game_property::Use {
                actor: r.person,
                inventory,
                purpose: "making marks".into(),
            })?;
            if method.method == Method::Pigment {
                tx.call(game_cargo::Consume {
                    inventory,
                    good,
                    quantity: 1,
                })?;
            }
        }
        let item = if let Some(item) = method.item {
            item
        } else {
            tx.call(game_items::Mark {
                person: r.person,
                inventory: r.inventory,
                good: method.surface,
                label: r.label,
            })?
        };
        tx.edit::<Surfaces>()?.0.insert(item, ());
        Ok(Imprint {
            item,
            method: method.method,
            time_factor: method.time_factor,
            tool: method.tool,
        })
    })
}
