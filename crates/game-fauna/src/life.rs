use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pregnancy {
    due: SimTime,
    father: u64,
    genome: Arc<Genome>,
    generation: u32,
}
impl Pregnancy {
    pub(super) fn valid(&self, at: SimTime) -> bool {
        self.due >= at
            && self
                .genome
                .values()
                .all(|g| g.0.iter().all(|v| *v <= 16000))
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Advance {
    after: Option<u64>,
    through: Option<u64>,
}
impl Request for Advance {
    const KEY: &'static str = "fauna.advance";
    type Response = ();
}
fn offspring(tx: &mut Transaction<'_>, mother: u64, body: &Animal, mate: &Pregnancy) -> Result<()> {
    let id = tx.allocate()?;
    tx.edit::<Fauna>()?.highest = id;
    let genome = body
        .genome
        .iter()
        .enumerate()
        .map(|(i, (key, m))| {
            let father = mate.genome.get(key).copied().unwrap_or(*m);
            (
                key.clone(),
                game_genetics::inherit_bounded(
                    random(tx.seed(), id, i as u64),
                    PersonId(id),
                    *m,
                    father,
                    20,
                    16000,
                ),
            )
        })
        .collect();
    let child = Animal {
        species: body.species.clone(),
        area: body.area,
        born: tx.now(),
        health: 100,
        hunger: 0,
        genome: Arc::new(genome),
        parents: [Some(mother), Some(mate.father)],
        generation: body.generation.max(mate.generation).saturating_add(1),
        caregiver: body.caregiver,
        familiarity: Table::new(),
        pregnancy: None,
    };
    tx.edit::<Fauna>()?.animals.insert(id, child.clone());
    tx.edit::<Fauna>()?.births += 1;
    tx.emit(
        Born {
            animal: id,
            body: child,
        },
        vec![],
    )
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Start>(|tx, _| {
        require(!tx.get::<Fauna>()?.running, "fauna already active")?;
        tx.edit::<Fauna>()?.running = true;
        tx.schedule(
            tx.now().after(DAY)?,
            31,
            Advance {
                after: None,
                through: None,
            },
        )?;
        Ok(())
    })?;
    b.request::<Advance>(|tx, r| {
        let through = r
            .through
            .or_else(|| tx.get::<Fauna>().unwrap().animals.keys().last().copied());
        let batch: Vec<_> = tx
            .get::<Fauna>()?
            .animals
            .scan_after(r.after.as_ref())
            .take_while(|(id, _)| through.is_some_and(|last| **id <= last))
            .take(129)
            .map(|(id, _)| *id)
            .collect();
        for id in batch.iter().take(128).copied() {
            let mut body = tx.get::<Fauna>()?.animals[&id].clone();
            let species = tx.get::<Fauna>()?.species[&body.species].clone();
            body.hunger = body.hunger.saturating_add(
                species
                    .intake
                    .saturating_mul(10)
                    .saturating_mul(body.size(&species, tx.now()))
                    / species.body_mass.max(1),
            );
            let patches: Vec<_> = tx
                .get::<game_ecology::Ecology>()?
                .in_area(body.area)
                .filter(|id| {
                    let p = &tx.get::<game_ecology::Ecology>().unwrap().patches[id];
                    tx.get::<game_objects::Objects>()
                        .unwrap()
                        .by_id(p.good)
                        .is_ok_and(|o| o.food.is_some() && o.tags.contains("plant"))
                })
                .take(8)
                .collect();
            for patch in patches {
                if body.hunger == 0 {
                    break;
                }
                let eaten = tx.call(game_ecology::Browse {
                    patch,
                    quantity: body.hunger.div_ceil(10),
                })?;
                body.hunger = body.hunger.saturating_sub(eaten * 10);
            }
            let warmth = game_environment::at(tx, body.area)?.warmth;
            let tolerance = (body.ability("hardiness") / 200).min(100) as i32;
            if body.hunger > 100
                || warmth < species.minimum_warmth - tolerance
                || warmth > species.maximum_warmth + tolerance
            {
                body.health = body.health.saturating_sub(5);
            }
            let age = tx.now().0.saturating_sub(body.born.0) / DAY.0;
            let old = age
                > species
                    .lifespan_days
                    .saturating_mul(game_vitality::factor(body.ability("longevity")))
                    / 1000;
            if body.health == 0 || old {
                tx.edit::<Fauna>()?.animals.remove(&id);
                tx.edit::<Fauna>()?.deaths += 1;
                tx.emit(
                    Died {
                        animal: id,
                        area: body.area,
                    },
                    vec![],
                )?;
                continue;
            }
            if let Some(mate) = body.pregnancy.clone()
                && mate.due <= tx.now()
            {
                offspring(tx, id, &body, &mate)?;
                body.pregnancy = None;
            }
            if body.pregnancy.is_none()
                && body.ability("reproductive_role") < 1000
                && age >= species.maturity_days
                && body.hunger < 30
            {
                let mate = tx
                    .get::<Fauna>()?
                    .animals
                    .iter()
                    .find(|(other, a)| {
                        **other != id
                            && a.area == body.area
                            && a.species == body.species
                            && a.ability("reproductive_role") >= 1000
                            && tx.now().0.saturating_sub(a.born.0) >= species.maturity_days * DAY.0
                    })
                    .map(|(id, a)| (*id, a.clone()));
                if let Some((father, mate)) = mate {
                    body.pregnancy = Some(Pregnancy {
                        due: tx
                            .now()
                            .after(SimDuration(species.gestation_days * DAY.0))?,
                        father,
                        genome: mate.genome,
                        generation: mate.generation,
                    });
                }
            }
            tx.edit::<Fauna>()?.animals.insert(id, body);
        }
        if batch.len() > 128 {
            tx.schedule(
                tx.now(),
                31,
                Advance {
                    after: batch.get(127).copied(),
                    through,
                },
            )?;
        } else {
            tx.schedule(
                tx.now().after(DAY)?,
                31,
                Advance {
                    after: None,
                    through: None,
                },
            )?;
        }
        Ok(())
    })
}
