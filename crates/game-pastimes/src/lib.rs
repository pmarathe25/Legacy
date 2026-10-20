//! Authored activities use real supplies and time; completed experiences shape personal interests.
mod motivation;
use game_core::*;
use game_sim::*;
use game_storage::Table;
pub use motivation::Satisfaction;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub name: String,
    pub inputs: BTreeMap<GoodId, u64>,
    #[serde(default)]
    pub marks: bool,
    pub requires: BTreeMap<GoodId, u64>,
    #[serde(default)]
    pub abilities: BTreeMap<String, u64>,
    pub duration: SimDuration,
    #[serde(alias = "interval")]
    pub satisfaction_span: SimDuration,
    pub affinity: String,
    pub concept: String,
    pub skill: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pastimes {
    pub enabled: bool,
    pub activities: Arc<BTreeMap<String, Activity>>,
    pub stamps: Vec<game_content::Stamp>,
    pub experiences: Table<PersonId, Table<String, u64>>,
    #[serde(default)]
    pub satisfaction: Table<PersonId, Table<String, Satisfaction>>,
    work: Table<PersonId, Work>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    activity: String,
    inscription: Option<game_inscription::Imprint>,
    inventory: InventoryId,
    job: ScheduleId,
    area: AreaId,
}
impl Resource for Pastimes {
    const KEY: &'static str = "pastimes.interests";
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        if Arc::ptr_eq(&self.activities, &previous.activities) {
            Ok(())
        } else {
            self.validate()
        }
    }
    fn validate(&self) -> Result<()> {
        require(
            self.activities.iter().all(|(k, a)| {
                !k.is_empty()
                    && !a.name.is_empty()
                    && a.duration.0 > 0
                    && a.satisfaction_span.0 > 0
                    && !a.affinity.is_empty()
                    && !a.concept.is_empty()
                    && !a.skill.is_empty()
                    && a.inputs.values().chain(a.requires.values()).all(|n| *n > 0)
            }),
            "invalid pastimes",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Install {
    pub activities: BTreeMap<String, Activity>,
    pub stamps: Vec<game_content::Stamp>,
}
impl Request for Install {
    const KEY: &'static str = "pastimes.install";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "pastimes.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Pursue {
    pub person: PersonId,
    pub activity: String,
    pub inventory: InventoryId,
}
impl Request for Pursue {
    const KEY: &'static str = "pastimes.pursue";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "pastimes.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "pastimes.cancel";
    type Response = ();
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Experienced {
    pub person: PersonId,
    pub activity: String,
    pub completed: bool,
    pub inscription: Option<game_inscription::Imprint>,
}
impl Event for Experienced {
    const KEY: &'static str = "pastimes.experienced";
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    name: String,
    #[serde(default)]
    inputs: BTreeMap<String, u64>,
    #[serde(default)]
    marks: bool,
    #[serde(default)]
    requires: BTreeMap<String, u64>,
    #[serde(default)]
    abilities: BTreeMap<String, u64>,
    duration: SimDuration,
    #[serde(alias = "interval")]
    satisfaction_span: SimDuration,
    affinity: String,
    concept: String,
    skill: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definitions {
    activities: BTreeMap<String, Spec>,
}
pub fn load(store: &mut game_content::Store, objects: &game_objects::Objects) -> Result<Install> {
    let mut activities = BTreeMap::new();
    let mut stamps = vec![];
    for path in store.scan("pastimes")? {
        let doc: game_content::Document<Definitions> = store.load(&path)?;
        stamps.push(doc.stamp()?);
        for (key, s) in doc.data.activities {
            let resolve = |rows: BTreeMap<String, u64>| {
                rows.into_iter()
                    .map(|(k, n)| Ok((objects.resolve(&k)?, n)))
                    .collect::<Result<_>>()
            };
            let a = Activity {
                name: s.name,
                inputs: resolve(s.inputs)?,
                marks: s.marks,
                requires: resolve(s.requires)?,
                abilities: s.abilities,
                duration: s.duration,
                satisfaction_span: s.satisfaction_span,
                affinity: s.affinity,
                concept: s.concept,
                skill: s.skill,
            };
            require(activities.insert(key, a).is_none(), "duplicate pastime")?;
        }
    }
    Pastimes {
        activities: Arc::new(activities.clone()),
        ..Default::default()
    }
    .validate()?;
    Ok(Install { activities, stamps })
}
fn available(tx: &Transaction<'_>, person: PersonId, inventory: InventoryId, a: &Activity) -> bool {
    let Ok(area) = game_transport::area(tx.get().unwrap(), person) else {
        return false;
    };
    game_vitality::can_manipulate(tx, person)
        && (!a.marks
            || tx.registered::<game_inscription::Make>()
                && game_inscription::supplies(tx, person, inventory).is_some())
        && a.abilities
            .iter()
            .all(|(key, minimum)| game_vitality::ability(tx, person, key) >= *minimum)
        && game_cargo::locality(tx, inventory).is_ok_and(|at| at == area)
        && tx
            .get::<game_cargo::Cargo>()
            .unwrap()
            .0
            .get(&inventory)
            .is_some_and(|i| {
                a.inputs.iter().chain(&a.requires).all(|(g, _)| {
                    i.amount(*g)
                        >= a.inputs
                            .get(g)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(a.requires.get(g).copied().unwrap_or(0))
                })
            })
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let _measurement = game_diagnostics::span("leisure planning");
    if !tx.get::<Pastimes>()?.enabled
        || tx.get::<game_lineage::Lineage>()?.current == Some(person)
        || tx.get::<Actions>()?.0.contains_key(&person)
        || game_physiology::capable(tx, person).is_err()
    {
        return Ok(());
    }
    let activities = tx.get::<Pastimes>()?.activities.clone();
    let mut options: Vec<_> = activities
        .iter()
        .enumerate()
        .map(|(i, (k, a))| {
            (
                std::cmp::Reverse(motivation::score(tx, person, a, i)),
                k.as_str(),
            )
        })
        .collect();
    options.sort_unstable();
    // Leisure draws only on the participant's own possessions, not everyone's shared stock.
    let inventories: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(person).collect();
    for (score, activity) in options {
        if score.0 <= 0 {
            continue;
        }
        for inventory in &inventories {
            if available(tx, person, *inventory, &activities[activity])
                && tx
                    .attempt(Pursue {
                        person,
                        activity: activity.to_owned(),
                        inventory: *inventory,
                    })
                    .is_ok()
            {
                return Ok(());
            }
        }
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("pastimes", 2)?;
    b.resource::<Pastimes>()?;
    b.request::<Install>(|tx, r| {
        require(
            !tx.get::<Pastimes>()?.enabled && tx.get::<Pastimes>()?.work.is_empty(),
            "pastimes in use",
        )?;
        for a in r.activities.values() {
            require(
                tx.get::<game_traits::Traits>()?
                    .definitions
                    .contains_key(&a.affinity),
                "unknown pastime affinity",
            )?;
            require(
                a.abilities.keys().all(|key| {
                    tx.get::<game_traits::Traits>()
                        .unwrap()
                        .definitions
                        .contains_key(key)
                }),
                "unknown pastime ability",
            )?;
            for good in a.inputs.keys().chain(a.requires.keys()) {
                tx.get::<game_objects::Objects>()?.by_id(*good)?;
            }
        }
        let s = tx.edit::<Pastimes>()?;
        s.activities = Arc::new(r.activities);
        s.stamps = r.stamps;
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Pastimes>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Pursue>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let a = tx
            .get::<Pastimes>()?
            .activities
            .get(&r.activity)
            .cloned()
            .ok_or_else(|| Error::Missing("pastime".into()))?;
        require(
            available(tx, r.person, r.inventory, &a),
            "pastime needs local physical supplies",
        )?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "leisure".into(),
        })?;
        for (good, quantity) in a.inputs {
            if tx
                .get::<game_objects::Objects>()?
                .by_id(good)?
                .food
                .is_some()
            {
                tx.call(game_physiology::ConsumeEdible {
                    person: r.person,
                    inventory: r.inventory,
                    good,
                    quantity,
                })?;
                if game_physiology::capable(tx, r.person).is_err() {
                    tx.emit(
                        Experienced {
                            person: r.person,
                            activity: r.activity.clone(),
                            completed: false,
                            inscription: None,
                        },
                        vec![r.person],
                    )?;
                    return Ok(tx.now());
                }
            } else {
                tx.call(game_cargo::Consume {
                    inventory: r.inventory,
                    good,
                    quantity,
                })?;
            }
        }
        let inscription = if a.marks {
            Some(tx.call(game_inscription::Make {
                person: r.person,
                inventory: r.inventory,
                label: "Composition".into(),
            })?)
        } else {
            None
        };
        let until = tx.now().after(SimDuration(
            a.duration
                .0
                .saturating_mul(inscription.as_ref().map_or(1, |i| i.time_factor)),
        ))?;
        tx.start_action(
            r.person,
            "pastime",
            Some(until),
            Cancel { person: r.person },
        )?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        let area = game_transport::area(tx.get()?, r.person)?;
        let s = tx.edit::<Pastimes>()?;
        s.work.insert(
            r.person,
            Work {
                activity: r.activity,
                inscription,
                inventory: r.inventory,
                job,
                area,
            },
        );
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let work = tx.edit::<Pastimes>()?.work.remove(&r.person).unwrap();
        let a = tx.get::<Pastimes>()?.activities[&work.activity].clone();
        let completed = work
            .inscription
            .as_ref()
            .is_none_or(|i| game_inscription::intact(tx, r.person, i))
            && game_transport::area(tx.get()?, r.person).is_ok_and(|area| area == work.area)
            && game_cargo::locality(tx, work.inventory).is_ok_and(|area| area == work.area)
            && tx
                .get::<game_cargo::Cargo>()?
                .0
                .get(&work.inventory)
                .is_some_and(|i| a.requires.iter().all(|(g, n)| i.amount(*g) >= *n));
        tx.finish_action(r.person, "pastime")?;
        if completed {
            let n = tx
                .edit::<Pastimes>()?
                .experiences
                .entry(r.person)
                .or_default()
                .entry(work.activity.clone())
                .or_default();
            *n = n.saturating_add(1);
            let prior = tx
                .get::<Pastimes>()?
                .satisfaction
                .get(&r.person)
                .and_then(|r| r.get(&a.concept))
                .map_or(0, |s| s.remaining(tx.now()));
            let now = tx.now();
            tx.edit::<Pastimes>()?
                .satisfaction
                .entry(r.person)
                .or_default()
                .insert(
                    a.concept.clone(),
                    Satisfaction {
                        at: now,
                        amount: prior.saturating_add(1600),
                        span: a.satisfaction_span,
                    },
                );
            tx.call(game_relationships::LearnValue {
                person: r.person,
                concept: a.concept,
                outcome: 1,
            })?;
            tx.call(game_skills::Practice {
                person: r.person,
                skill: a.skill,
                effort: a.duration.0 / MINUTE.0 + 1,
                attribution: work.job.0,
            })?;
        }
        tx.emit(
            Experienced {
                person: r.person,
                activity: work.activity,
                completed,
                inscription: work.inscription,
            },
            vec![r.person],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(w) = tx.edit::<Pastimes>()?.work.remove(&r.person) {
            tx.unschedule(w.job);
        }
        Ok(())
    })?;
    game_choice::provider(b, "pastimes.leisure", |tx, e| {
        if game_choice::collecting(tx, e.person) {
            motivation::propose(tx, e.person)
        } else {
            consider(tx, e.person)
        }
    })?;
    b.subscribe::<game_physiology::Died>("pastimes.death", |tx, e| {
        tx.edit::<Pastimes>()?.satisfaction.remove(&e.person);
        Ok(())
    })?;
    b.validate_save(|h| {
        for (p, w) in &h.get::<Pastimes>()?.work {
            require(
                h.get::<Actions>()?
                    .0
                    .get(p)
                    .is_some_and(|a| a.kind == "pastime")
                    && h.get::<Pastimes>()?.activities.contains_key(&w.activity)
                    && h.get::<game_cargo::Cargo>()?.0.contains_key(&w.inventory),
                "invalid saved pastime",
            )?;
        }
        Ok(())
    });
    Ok(())
}
