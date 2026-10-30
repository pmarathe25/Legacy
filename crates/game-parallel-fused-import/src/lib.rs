//! Validated import of physical residents, travelers, and fused work slots.
pub mod export;
mod teachers;
use game_parallel::{Person, World};
use game_parallel_block_data::Journey;
use game_parallel_fused_data::{BLOCK, Block};
use game_parallel_fused_metadata::Store;
use game_parallel_production_receipts::Receipts;
use rayon::prelude::*;
use std::{collections::HashMap, mem};
pub struct Imported {
    pub world: World,
    pub blocks: Vec<Block>,
    pub food: Vec<u32>,
    pub jobs: Store,
    pub waiting: Vec<usize>,
    pub receipts: Receipts,
    pub positions: Vec<(u64, usize, usize)>,
}
pub fn compile(mut world: World) -> Result<Imported, String> {
    if world.institutions.is_some() || world.civic_auto.is_some() {
        return Err("preset institutions are not supported by the parallel simulation".into());
    }
    world.validate()?;
    if let Some(conflict) = &mut world.conflict {
        conflict.encounters.par_iter_mut().for_each(|fight| {
            if fight
                .fighters
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                fight
                    .fighters
                    .par_sort_unstable_by_key(|fighter| fighter.person);
            }
        });
    }
    let (jobs, receipts) = if let Some(production) = world.production.as_mut() {
        (
            Store::new(mem::take(&mut production.jobs))?,
            Receipts::from_events(mem::take(&mut production.events)),
        )
    } else {
        (Store::empty(), Receipts::default())
    };
    let active: HashMap<u64, usize> = jobs
        .chunks()
        .par_iter()
        .enumerate()
        .flat_map_iter(|(block, rows)| {
            rows.iter()
                .enumerate()
                .filter(|(_, row)| !row.waiting)
                .map(move |(offset, row)| (row.worker, block * BLOCK + offset))
        })
        .collect();
    let waiting: Vec<usize> = jobs
        .chunks()
        .par_iter()
        .enumerate()
        .flat_map_iter(|(block, rows)| {
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.waiting)
                .map(move |(offset, _)| block * BLOCK + offset)
        })
        .collect();
    let food = world.regions.iter().map(|region| region.food).collect();
    let region_index: HashMap<_, _> = world
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| (region.id, index))
        .collect();
    let mut blocks = Vec::new();
    for (region_index, region) in world.regions.iter_mut().enumerate() {
        let mut people = mem::take(&mut region.people);
        people.sort_unstable_by_key(|person| person.id);
        for chunk in people.chunks(BLOCK) {
            blocks.push(Block::new(
                region_index,
                region.id,
                chunk,
                None,
                &active,
                &jobs,
            ));
        }
    }
    let mut travelers = mem::take(&mut world.travelers);
    travelers.par_sort_unstable_by_key(|traveler| {
        (
            traveler.source,
            traveler.destination,
            traveler.ticks_remaining,
            traveler.person.id,
        )
    });
    let mut travel_blocks: Vec<_> = travelers
        .par_chunk_by(|a, b| {
            (a.source, a.destination, a.ticks_remaining)
                == (b.source, b.destination, b.ticks_remaining)
        })
        .flat_map(|group| {
            let source = region_index[&group[0].source];
            let journey = Journey {
                source: group[0].source,
                destination: group[0].destination,
                destination_index: region_index[&group[0].destination],
                remaining: group[0].ticks_remaining,
            };
            let active = &active;
            let jobs = &jobs;
            group.par_chunks(BLOCK).map(move |chunk| {
                let people: Vec<Person> = chunk
                    .iter()
                    .map(|traveler| traveler.person.clone())
                    .collect();
                Block::new(source, journey.source, &people, Some(journey), active, jobs)
            })
        })
        .collect();
    blocks.append(&mut travel_blocks);
    blocks.par_sort_unstable_by_key(|block| (block.region, block.ids[0]));
    let positions = positions(&blocks);
    Ok(Imported {
        world,
        blocks,
        food,
        jobs,
        waiting,
        receipts,
        positions,
    })
}
pub fn positions(blocks: &[Block]) -> Vec<(u64, usize, usize)> {
    let mut rows: Vec<_> = blocks
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, block)| {
            block
                .ids
                .iter()
                .enumerate()
                .map(move |(offset, &id)| (id, index, offset))
        })
        .collect();
    if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        rows.par_sort_unstable_by_key(|row| row.0);
    }
    rows
}

/// Extract compact owner state after the shared world has been validated.
pub struct Owners {
    pub autonomy: Option<game_parallel_fused_autonomy::Runtime>,
    pub social: Option<game_parallel_social_edges::Runtime>,
    pub knowledge: Option<game_parallel_knowledge_columns::Runtime>,
    pub agreements: Option<game_parallel_agreements::runtime::Runtime>,
    pub family: Option<game_parallel_fused_family::Runtime>,
    pub goods: Option<game_parallel_goods_packed::PackedCargo>,
    pub conflict: Option<game_parallel_fused_conflict::packed::Runtime>,
}
pub fn instance() -> Result<u64, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| "kernel identity overflow".into())
}
pub fn owners(
    world: &mut World,
    blocks: &mut [Block],
    positions: &[(u64, usize, usize)],
) -> Result<Owners, String> {
    game_parallel_fused_nutrition::import(world.nutrition.as_ref(), blocks);
    if let Some(fatigue) = &world.fatigue {
        game_parallel_fused_needs::fatigue::import(fatigue, blocks);
    }
    if let Some(disease) = &world.disease {
        game_parallel_fused_disease::import(disease, blocks);
    }
    let conflict = world
        .conflict
        .as_mut()
        .map(game_parallel_fused_conflict::packed::Runtime::compile)
        .transpose()?;
    let goods = game_parallel_fused_goods::extract(world, blocks, positions);
    let family = world
        .families
        .as_ref()
        .map(game_parallel_fused_family::Runtime::compile_validated)
        .transpose()?;
    let agreements = world
        .agreements
        .as_ref()
        .map(game_parallel_agreements::runtime::Runtime::compile);
    let knowledge = world
        .knowledge
        .as_ref()
        .map(|k| game_parallel_knowledge_columns::Runtime::compile_validated(k, world.tick))
        .transpose()?;
    let social = world
        .social
        .as_ref()
        .map(|s| {
            game_parallel_social_edges::Runtime::compile_validated(
                &s.relationships,
                s.rules.clone(),
                world.tick,
            )
        })
        .transpose()?;
    let autonomy = world
        .autonomy
        .as_ref()
        .map(|a| game_parallel_fused_autonomy::compile(a, &world.routes, world.tick))
        .transpose()?;
    Ok(Owners {
        autonomy,
        social,
        knowledge,
        agreements,
        goods,
        conflict,
        family,
    })
}

/// Publish learning or clear last tick's transient skill events.
pub fn commit_skills(
    target: &mut Option<game_parallel::Skills>,
    draft: Option<game_parallel::Skills>,
) {
    if let Some(skills) = draft {
        *target = Some(skills);
    } else if let Some(skills) = target {
        skills.events.clear();
    }
}

/// Publish optional learning and disease owners after all fallible stages.
pub fn commit_health(
    world: &mut World,
    skills: Option<game_parallel::Skills>,
    aspirations: (Option<game_parallel::Goals>, Option<game_parallel::Dreams>),
    purpose: Option<game_parallel::Purpose>,
    disease: Option<Vec<game_parallel::DiseaseEvent>>,
    disease_dirty: &mut bool,
    fatigue_dirty: &mut bool,
) {
    commit_skills(&mut world.skills, skills);
    let (goals, dreams) = aspirations;
    if let Some(dreams) = dreams {
        world.dreams = Some(dreams);
    } else if let Some(dreams) = &mut world.dreams {
        dreams.events.clear();
    }
    if let Some(goals) = goals {
        world.goals = Some(goals);
    } else if let Some(goals) = &mut world.goals {
        goals.events.clear();
    }
    if let Some(purpose) = purpose {
        world.purpose = Some(purpose);
    }
    if let (Some(events), Some(disease)) = (disease, &mut world.disease) {
        disease.cases.clear();
        disease.events = events;
        *disease_dirty = true;
    }
    if let Some(fatigue) = &mut world.fatigue {
        fatigue.people.clear();
        *fatigue_dirty = true;
    }
}

impl Owners {
    pub fn enroll_births(&mut self, world: &mut World) {
        let Some(family) = self.family.as_ref() else {
            return;
        };
        let births = family.births();
        if let Some(cargo) = world
            .cargo
            .as_mut()
            .filter(|cargo| cargo.personal_food_access)
            && let Some(age) = cargo.auto_seek_age
        {
            cargo
                .food_enrollments
                .par_extend(
                    births
                        .par_iter()
                        .map(|&person| game_parallel::FoodEnrollment {
                            person,
                            at: world.tick.saturating_add(age),
                        }),
                );
        }
        if world
            .personality
            .as_ref()
            .is_some_and(|owner| owner.inherit_births)
            || world
                .skills
                .as_ref()
                .is_some_and(|owner| owner.inherit_teachers)
            || world
                .caregiving
                .as_ref()
                .is_some_and(|owner| owner.inherit_births)
            || world
                .cargo
                .as_ref()
                .is_some_and(|owner| owner.inherits_birth_preferences())
        {
            let parents = births
                .par_iter()
                .map(|&person| {
                    let slot = family.store.locate(person).expect("newborn family record");
                    let metadata = &family.store.metadata[slot];
                    (person, metadata.parents, metadata.parent_count)
                })
                .collect::<Vec<_>>();
            if let Some(personality) = &mut world.personality {
                personality.enroll_births(&parents);
            }
            if let Some(skills) = &mut world.skills {
                teachers::enroll(skills, &parents, world.tick, family.rules.adult_age);
            }
            if let Some(care) = &mut world.caregiving {
                care.inherit_preferences(&parents);
            }
            if let Some(cargo) = &mut world.cargo {
                game_parallel_fused_store_build::inherit_birth_preferences(cargo, &parents);
            }
        }
        if !family.rules.enroll_births {
            return;
        }
        game_parallel_social_pressure::inherit_birth_pressure!(world, family, births);
        if let Some(purpose) = &mut world.purpose {
            purpose.people.par_extend(births.par_iter().map(|&person| {
                game_parallel::PurposePerson {
                    person,
                    deprivation: 0,
                    eligible_at: world.tick.saturating_add(family.rules.adult_age),
                }
            }));
        }
        if let Some(knowledge) = &mut self.knowledge {
            knowledge
                .enroll(&births)
                .expect("validated newborn identities");
        }
        if let Some(autonomy) = &mut self.autonomy {
            game_parallel_fused_autonomy::enroll(
                autonomy,
                &births,
                world.tick,
                family.rules.adult_age,
            );
        }
        let mut inactive_autonomy = None;
        let mut ignored = None;
        game_parallel_family_host::enrollment::Context {
            families: None,
            social: &mut world.social,
            knowledge: &mut ignored,
            autonomy: &mut inactive_autonomy,
            tick: world.tick,
        }
        .enroll_ids(&births, family.rules.adult_age);
    }
}

impl Owners {
    pub fn commit_knowledge(
        &mut self,
        draft: game_parallel_knowledge_columns::Draft,
        world: &mut World,
    ) {
        self.knowledge
            .as_mut()
            .expect("enabled knowledge runtime")
            .commit(draft)
            .expect("validated knowledge generation");
        world
            .knowledge
            .as_mut()
            .expect("enabled knowledge")
            .minds
            .clear();
    }
    pub fn borrowed_memories(
        &self,
        world: &World,
    ) -> Result<Vec<(u64, game_parallel::KnowledgeRecord)>, String> {
        let invocation = world
            .lineage
            .as_ref()
            .and_then(|l| l.invocation.as_ref())
            .ok_or("invocation is disabled")?;
        let mut records = Vec::new();
        if let Some(knowledge) = &self.knowledge {
            for &life in &invocation.active {
                if let Some(memory) = knowledge.records(life) {
                    records.extend(memory.iter().cloned().map(|record| (life, record)));
                }
            }
        }
        Ok(records)
    }
}

impl Owners {
    pub fn commit_social(
        &mut self,
        draft: game_parallel_social_edges::Selected,
        world: &mut World,
    ) {
        self.social
            .as_mut()
            .expect("enabled social runtime")
            .commit(draft.edges)
            .expect("validated social generation");
        let social = world.social.as_mut().expect("enabled social");
        social.relationships.clear();
        social.events = draft.events;
        social.needs = draft.needs;
    }
}
