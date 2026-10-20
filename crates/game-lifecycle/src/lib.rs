//! Actual pregnancies and births, families, aging inputs, and death consequences.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub mother: PersonId,
    pub father: PersonId,
    pub attendant: PersonId,
    pub genes: game_genetics::Alleles,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pregnancy {
    pub mother: PersonId,
    pub father: PersonId,
    pub attendant: PersonId,
    pub due: SimTime,
    pub job: ScheduleId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Population {
    pub families: Table<PersonId, Family>,
    pub pregnancies: Table<u64, Pregnancy>,
    pub gestation: SimDuration,
    pub adult_age: SimDuration,
    #[serde(default)]
    pub recovery: Table<PersonId, SimTime>,
}
impl Default for Population {
    fn default() -> Self {
        Self {
            families: Table::new(),
            pregnancies: Table::new(),
            gestation: SimDuration(DAY.0 * 270),
            adult_age: SimDuration(DAY.0 * 365 * 18),
            recovery: Table::new(),
        }
    }
}
impl Resource for Population {
    fn allocated_ids(&self) -> Vec<u64> {
        self.pregnancies.keys().copied().collect()
    }
    const KEY: &'static str = "lifecycle.population";
    fn validate(&self) -> Result<()> {
        require(self.gestation.0 > 0, "invalid gestation")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Birth {
    pub mother: PersonId,
    pub father: PersonId,
    pub attendant: PersonId,
    pub area: AreaId,
}
impl Request for Birth {
    const KEY: &'static str = "lifecycle.birth";
    type Response = PersonId;
}
/// Reproductive aging follows the body's inherited natural lifespan, independently of attachment.
pub fn fertile(body: &game_physiology::Body, adult_age: SimDuration, at: SimTime) -> bool {
    let age = at.0.saturating_sub(body.born.0);
    body.alive
        && body.kind == game_physiology::Kind::Mortal
        && age >= adult_age.0
        && body
            .life_limit
            .is_some_and(|end| age < end.0.saturating_sub(body.born.0) / 3 * 2)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Gestational,
    Gamete,
}
/// Missing phenotypes in legacy/isolated worlds retain their previous physical possibilities.
pub fn reproductive(tx: &Transaction<'_>, person: PersonId, role: Role) -> Result<bool> {
    let population = tx.get::<Population>()?;
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    if !fertile(body, population.adult_age, tx.now())
        || population
            .recovery
            .get(&person)
            .is_some_and(|at| *at > tx.now())
    {
        return Ok(false);
    }
    let traits = tx.get::<game_traits::Traits>().ok();
    Ok(traits
        .filter(|t| t.definitions.contains_key("reproductive_role"))
        .is_none_or(|t| {
            let phenotype = t.ability(person, "reproductive_role");
            match role {
                Role::Gestational => phenotype <= 1000,
                Role::Gamete => phenotype >= 1000,
            }
        }))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Conceive {
    pub mother: PersonId,
    pub father: PersonId,
    pub attendant: PersonId,
}
impl Request for Conceive {
    const KEY: &'static str = "lifecycle.conceive";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
struct Deliver {
    pregnancy: u64,
}
impl Request for Deliver {
    const KEY: &'static str = "lifecycle.deliver";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Conceived {
    pub mother: PersonId,
    pub father: PersonId,
    pub due: SimTime,
}
impl Event for Conceived {
    const KEY: &'static str = "lifecycle.conceived";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Born {
    pub person: PersonId,
    pub at: SimTime,
    pub area: AreaId,
}
impl Event for Born {
    const KEY: &'static str = "lifecycle.born";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("lifecycle", 2)?;
    b.resource::<Population>()?;
    b.validate_save(|h| {
        for (person, f) in &h.get::<Population>()?.families {
            let p = &h.get::<game_physiology::Bodies>()?.people;
            require(
                [*person, f.mother, f.father, f.attendant]
                    .iter()
                    .all(|id| p.contains_key(id)),
                "unknown family member",
            )?;
        }
        Ok(())
    });
    b.request::<Birth>(|tx, r| {
        game_physiology::capable(tx, r.mother)?;
        for person in [r.father, r.attendant] {
            require(
                tx.get::<game_physiology::Bodies>()?
                    .people
                    .contains_key(&person),
                "unknown family member",
            )?;
        }
        require(
            game_transport::area(tx.get()?, r.mother)? == r.area,
            "mother is elsewhere",
        )?;
        let person = tx.call(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: tx.now(),
        })?;
        tx.call(game_transport::Place {
            person,
            area: r.area,
        })?;
        let genes = |p| {
            tx.get::<Population>()
                .unwrap()
                .families
                .get(&p)
                .map_or(game_genetics::Alleles([500, 500]), |f| f.genes)
        };
        let inherited =
            game_genetics::inherit(tx.seed(), person, genes(r.mother), genes(r.father), 10);
        tx.edit::<Population>()?.families.insert(
            person,
            Family {
                mother: r.mother,
                father: r.father,
                attendant: r.attendant,
                genes: inherited,
            },
        );
        let recovery = game_vitality::ability(tx, r.mother, "recovery").max(100);
        let until = tx
            .now()
            .after(SimDuration((42 * DAY.0).saturating_mul(1000) / recovery))?;
        tx.edit::<Population>()?.recovery.insert(r.mother, until);
        tx.emit(
            Born {
                person,
                at: tx.now(),
                area: r.area,
            },
            vec![r.mother, r.attendant],
        )?;
        Ok(person)
    })?;
    b.request::<Conceive>(|tx, r| {
        require(r.mother != r.father, "parents must differ")?;
        require(
            reproductive(tx, r.mother, Role::Gestational)?
                && reproductive(tx, r.father, Role::Gamete)?,
            "incompatible reproductive capacities or incomplete recovery",
        )?;
        for person in [r.mother, r.father] {
            game_physiology::capable(tx, person)?;
            let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
            require(
                fertile(body, tx.get::<Population>()?.adult_age, tx.now()),
                "parent is outside their reproductive lifespan",
            )?;
        }
        require(
            game_transport::area(tx.get()?, r.mother)?
                == game_transport::area(tx.get()?, r.father)?,
            "parents are elsewhere",
        )?;
        require(
            !tx.get::<Population>()?
                .pregnancies
                .values()
                .any(|p| p.mother == r.mother),
            "already pregnant",
        )?;
        let id = tx.allocate()?;
        let due = tx.now().after(tx.get::<Population>()?.gestation)?;
        let job = tx.schedule(due, 15, Deliver { pregnancy: id })?;
        tx.edit::<Population>()?.pregnancies.insert(
            id,
            Pregnancy {
                mother: r.mother,
                father: r.father,
                attendant: r.attendant,
                due,
                job,
            },
        );
        for person in [r.mother, r.father] {
            if tx.registered::<game_vitality::Experience>() {
                let reward = game_vitality::disposition(tx, person, "reproductive-drive") / 20;
                tx.call(game_vitality::Experience {
                    person,
                    stress: -(reward / 4) as i32,
                    satisfaction: reward as i32,
                })?;
            }
        }
        tx.emit(
            Conceived {
                mother: r.mother,
                father: r.father,
                due,
            },
            vec![r.mother, r.father],
        )?;
        Ok(id)
    })?;
    b.request::<Deliver>(|tx, r| {
        let Some(p) = tx
            .get::<Population>()?
            .pregnancies
            .get(&r.pregnancy)
            .cloned()
        else {
            return Ok(());
        };
        if game_physiology::capable(tx, p.mother).is_err() {
            tx.edit::<Population>()?.pregnancies.remove(&r.pregnancy);
        } else if let Ok(area) = game_transport::area(tx.get()?, p.mother) {
            tx.edit::<Population>()?.pregnancies.remove(&r.pregnancy);
            // Records who was present at delivery; this creates no care commitment.
            // Responsibility is a care opportunity, not compulsory care or affection.
            let attendant = if game_physiology::capable(tx, p.attendant).is_ok()
                && game_transport::area(tx.get()?, p.attendant).ok() == Some(area)
            {
                p.attendant
            } else {
                p.mother
            };
            tx.call(Birth {
                mother: p.mother,
                father: p.father,
                attendant,
                area,
            })?;
        } else {
            // Travel cannot silently erase a pregnancy. Deliver at the next settled location.
            let next = match tx.get::<game_transport::Positions>()?.0.get(&p.mother) {
                Some(game_transport::Location::Transit { arrives, .. }) => {
                    (*arrives).max(tx.now().after(MINUTE)?)
                }
                _ => tx.now().after(HOUR)?,
            };
            let job = tx.schedule(
                next,
                15,
                Deliver {
                    pregnancy: r.pregnancy,
                },
            )?;
            tx.edit::<Population>()?
                .pregnancies
                .get_mut(&r.pregnancy)
                .unwrap()
                .job = job;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("lifecycle.possessions", |tx, event| {
        tx.call(game_cargo::DropCarried {
            person: event.person,
        })?;
        Ok(())
    })
}
