use super::assets::Assets;
use super::*;
use game_scenarios::Definition;
use std::collections::BTreeSet;
pub(super) fn initialize(
    host: &mut Host,
    d: &Definition,
    assets: Assets,
    seed: u64,
    player: bool,
) -> Result<()> {
    let regions = assets.regions.clone();
    assets.install(host)?;
    let mut names = BTreeMap::new();
    let mut areas = BTreeMap::new();
    let mut people = BTreeMap::new();
    let mut inventories = BTreeMap::new();
    for area in &d.areas {
        let id = host.dispatch(game_land::CreateArea {
            name: area.name.clone(),
        })?;
        areas.insert(area.key.clone(), id);
        names.insert(area.key.clone(), id.0);
        if let Some(climate) = regions.get(&area.key) {
            host.dispatch(game_environment::ConfigureArea {
                area: id,
                climate: climate.clone(),
            })?;
        }
    }
    for link in &d.links {
        host.dispatch(game_land::Link {
            from: areas[&link.from],
            to: areas[&link.to],
            duration: link.duration,
        })?;
    }
    for (key, p) in d.actors() {
        let person = host.dispatch(game_physiology::Create {
            kind: p.kind,
            born: if d.fixture {
                SimTime(0)
            } else {
                game_worldgen::birth_time(seed, &key, host.now())
            },
        })?;
        host.dispatch(game_transport::Place {
            person,
            area: areas[&p.area],
        })?;
        host.dispatch(game_traits::Set {
            person,
            profile: p.traits.clone(),
        })?;
        host.dispatch(game_relationships::Initialize {
            person,
            beliefs: p.beliefs.clone(),
            values: p.values.clone(),
        })?;
        if d.social_life.is_some() {
            let name = p.name.as_ref().unwrap_or(&p.key);
            host.dispatch(game_social_life::Register {
                person,
                name: if p.count == 1 {
                    name.clone()
                } else {
                    format!("{name} {}", key.rsplit('-').next().unwrap())
                },
            })?;
        }
        people.insert(key.clone(), person);
        names.insert(key, person.0);
    }
    for bond in &d.bonds {
        host.dispatch(game_relationships::SetRelationship {
            observer: people[&bond.observer],
            other: people[&bond.other],
            relationship: bond.relationship.clone(),
        })?;
    }
    let ruler = people[&d.ruler];
    if player {
        host.dispatch(game_lineage::Begin { person: ruler })?;
    }
    for (key, p) in d.actors() {
        if p.kind == game_physiology::Kind::Guardian {
            let person = people[&key];
            host.dispatch(game_guardians::Register { person })?;
            for area in areas.values() {
                host.dispatch(game_guardians::KnowDanger {
                    guardian: person,
                    area: *area,
                    danger: 0,
                })?;
            }
        }
    }
    for inventory in &d.inventories {
        let custody = if let Some(area) = &inventory.area {
            game_cargo::Custody::Area(areas[area])
        } else {
            game_cargo::Custody::Carried(people[inventory.carried_by.as_ref().unwrap()])
        };
        let id = host.dispatch(game_cargo::Create {
            custody,
            capacity: inventory.capacity,
        })?;
        inventories.insert(inventory.key.clone(), id);
        names.insert(inventory.key.clone(), id.0);
        let owner = people[&inventory.owner];
        host.dispatch(game_property::ClaimInventory {
            claimant: owner,
            inventory: id,
            recognized_by: BTreeSet::from([owner]),
        })?;
        for key in d.select(&inventory.access)? {
            host.dispatch(game_property::Permit {
                issuer: owner,
                person: people[&key],
                inventory: id,
                allow: true,
            })?;
        }
        for (key, base) in &inventory.goods {
            if *base > 0 {
                let good = host.get::<game_objects::Objects>()?.resolve(key)?;
                host.dispatch(game_cargo::Deposit {
                    inventory: id,
                    good,
                    quantity: *base,
                })?;
            }
        }
        for (key, multiplier) in &inventory.per_person {
            let quantity = multiplier
                .checked_mul(people.len() as u64)
                .ok_or(Error::Overflow)?;
            if quantity > 0 {
                let good = host.get::<game_objects::Objects>()?.resolve(key)?;
                host.dispatch(game_cargo::Deposit {
                    inventory: id,
                    good,
                    quantity,
                })?;
            }
        }
        for key in d.select(&inventory.known_to)? {
            let person = people[&key];
            host.dispatch(game_knowledge::ObserveStore {
                person,
                inventory: id,
            })?;
        }
    }
    for plot in &d.plots {
        let id = host.dispatch(game_agriculture::Create(game_agriculture::Plot {
            area: areas[&plot.area],
            steward: people[&plot.steward],
            crop: plot.crop.clone(),
            ..Default::default()
        }))?;
        names.insert(plot.key.clone(), id.0);
    }
    for patch in &d.patches {
        let good = host
            .get::<game_objects::Objects>()?
            .resolve(&patch.object)?;
        let id = host.dispatch(game_ecology::Create(game_ecology::Patch {
            area: areas[&patch.area],
            good,
            stock: patch.stock,
            capacity: patch.capacity,
            renew: patch.renew,
        }))?;
        names.insert(patch.key.clone(), id);
        if let Some(minimum_warmth) = patch.renewal_warmth {
            host.dispatch(game_ecology::SetRenewal {
                patch: id,
                minimum_warmth,
                minimum_rain: patch.renewal_rain,
            })?;
        }
    }
    for person in people.values() {
        host.dispatch(game_characters::Register {
            person: *person,
            mind: Default::default(),
        })?;
    }
    for institution in &d.institutions {
        let members = d
            .select(&institution.members)?
            .into_iter()
            .map(|key| people[&key])
            .collect();
        let procedure = institution
            .leader
            .as_ref()
            .map_or(game_institutions::Procedure::Majority, |key| {
                game_institutions::Procedure::Leader(people[key])
            });
        let offices = institution
            .offices
            .iter()
            .map(|(office, key)| (office.clone(), people[key]))
            .collect();
        let id = host.dispatch(game_institutions::Found(game_institutions::Institution {
            name: institution.name.clone(),
            jurisdiction: areas[&institution.area],
            members,
            procedure,
            offices,
            rules: vec![],
        }))?;
        names.insert(institution.key.clone(), id.0);
    }
    if let Some(c) = &d.combat {
        let fighters = c
            .fighters
            .iter()
            .map(|f| {
                (
                    people[&f.person],
                    game_conflict::Fighter {
                        team: f.team,
                        point: game_conflict::Point { x: f.x, y: f.y },
                        health: host.get::<game_physiology::Bodies>().unwrap().people
                            [&people[&f.person]]
                            .health,
                    },
                )
            })
            .collect();
        let controlled = c
            .fighters
            .iter()
            .filter(|f| f.controlled)
            .map(|f| people[&f.person])
            .collect();
        let id = host.dispatch(game_conflict::Start(game_conflict::Battle {
            area: areas[&c.area],
            map: game_conflict::Map::generate(seed, c.width, c.obstacles)?,
            fighters,
            controlled,
            orders: BTreeMap::new(),
            round: 0,
            interval: c.interval,
            movement: c.movement,
            reach: c.reach,
            damage: c.damage,
            scheduled: None,
            finished: false,
        }))?;
        names.insert(c.key.clone(), id);
    }
    if let Some(r) = &d.reincarnation {
        let health = host.get::<game_physiology::Bodies>()?.people[&ruler].health;
        host.dispatch(game_physiology::Damage {
            person: ruler,
            amount: health,
        })?;
        host.advance(host.now().after(r.delay)?, 32)?;
        let successor = host.dispatch(game_lifecycle::Birth {
            mother: people[&r.mother],
            father: people[&r.father],
            attendant: people[&r.attendant],
            area: areas[&r.area],
        })?;
        names.insert(d.ruler.clone(), successor.0);
        host.dispatch(game_traits::Set {
            person: successor,
            profile: r.traits.clone(),
        })?;
        for key in &r.access {
            let id = inventories[key];
            let owner = people[&d.inventories.iter().find(|i| i.key == *key).unwrap().owner];
            host.dispatch(game_property::Permit {
                issuer: owner,
                person: successor,
                inventory: id,
                allow: true,
            })?;
            host.dispatch(game_knowledge::ObserveStore {
                person: successor,
                inventory: id,
            })?;
        }
        host.dispatch(game_characters::Register {
            person: successor,
            mind: Default::default(),
        })?;
    }
    // The identity resource is filled by the content loader after this binder returns.
    host.dispatch(SetNames(names))?;
    host.dispatch(game_physiology::Start)?;
    if d.autonomy {
        host.dispatch(game_characters::Start)?;
    }
    if d.renewal {
        host.dispatch(game_ecology::StartRenewal)?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct SetNames(BTreeMap<String, u64>);
impl Request for SetNames {
    const KEY: &'static str = "openings.names";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<SetNames>(|tx, r| {
        require(
            tx.get::<Opening>()?.scenario.is_empty(),
            "opening already initialized",
        )?;
        tx.edit::<Opening>()?.names = r.0;
        Ok(())
    })
}
