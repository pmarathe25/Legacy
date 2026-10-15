//! Actual pregnancies and births, families, aging inputs, and death consequences.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub mother: PersonId,
    pub father: PersonId,
    pub caregiver: PersonId,
    pub genes: game_genetics::Alleles,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pregnancy {
    pub mother: PersonId,
    pub father: PersonId,
    pub caregiver: PersonId,
    pub due: SimTime,
    pub job: ScheduleId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Population {
    pub families: Table<PersonId, Family>,
    pub pregnancies: Table<u64, Pregnancy>,
    pub gestation: SimDuration,
    pub adult_age: SimDuration,
}
impl Default for Population {
    fn default() -> Self {
        Self {
            families: Table::new(),
            pregnancies: Table::new(),
            gestation: SimDuration(DAY.0 * 270),
            adult_age: SimDuration(DAY.0 * 365 * 18),
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
    pub caregiver: PersonId,
    pub area: AreaId,
}
impl Request for Birth {
    const KEY: &'static str = "lifecycle.birth";
    type Response = PersonId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Conceive {
    pub mother: PersonId,
    pub father: PersonId,
    pub caregiver: PersonId,
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
pub struct Born {
    pub person: PersonId,
    pub at: SimTime,
    pub area: AreaId,
}
impl Event for Born {
    const KEY: &'static str = "lifecycle.born";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("lifecycle", 1)?;
    b.resource::<Population>()?;
    b.validate_save(|h| {
        for (person, f) in &h.get::<Population>()?.families {
            let p = &h.get::<game_physiology::Bodies>()?.people;
            require(
                [*person, f.mother, f.father, f.caregiver]
                    .iter()
                    .all(|id| p.contains_key(id)),
                "unknown family member",
            )?;
        }
        Ok(())
    });
    b.request::<Birth>(|tx, r| {
        game_physiology::capable(tx, r.mother)?;
        for person in [r.father, r.caregiver] {
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
                caregiver: r.caregiver,
                genes: inherited,
            },
        );
        tx.emit(
            Born {
                person,
                at: tx.now(),
                area: r.area,
            },
            vec![r.mother, r.caregiver],
        )?;
        Ok(person)
    })?;
    b.request::<Conceive>(|tx, r| {
        require(r.mother != r.father, "parents must differ")?;
        for person in [r.mother, r.father] {
            game_physiology::capable(tx, person)?;
            let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
            require(
                tx.now().0 - body.born.0 >= tx.get::<Population>()?.adult_age.0,
                "parent is immature",
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
                caregiver: r.caregiver,
                due,
                job,
            },
        );
        Ok(id)
    })?;
    b.request::<Deliver>(|tx, r| {
        let p = tx
            .edit::<Population>()?
            .pregnancies
            .remove(&r.pregnancy)
            .unwrap();
        if game_physiology::capable(tx, p.mother).is_ok()
            && let Ok(area) = game_transport::area(tx.get()?, p.mother)
        {
            tx.call(Birth {
                mother: p.mother,
                father: p.father,
                caregiver: p.caregiver,
                area,
            })?;
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
