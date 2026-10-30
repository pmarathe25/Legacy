//! Stage physical and owner outputs for a fused generation.
mod dreams;
mod methods;
use game_parallel::{Agriculture, Shipment, World};
use game_parallel_fused_data::Block;
use game_parallel_fused_decisions::Draft as Decisions;
use game_parallel_fused_metadata::Store;
use game_parallel_fused_work_experience::memory as work_memory;
use game_parallel_production_receipts::Receipts;
use rayon::prelude::*;
use std::collections::HashMap;
pub struct Draft {
    pub food_stores: Option<game_parallel_fused_food_aid::stores::Draft>,
    pub food_deposits: Option<game_parallel_fused_food_deposit::Draft>,
    pub store_builds: Option<game_parallel_fused_store_build::Draft>,
    pub diet: Option<game_parallel_fused_diet::Draft>,
    pub work_memory: Option<work_memory::Draft>,
    pub food: Vec<u32>,
    pub updates: HashMap<(u64, u32), u64>,
    pub agriculture: Option<Agriculture>,
    pub shipments: Option<Vec<Shipment>>,
    pub carried_meals: Vec<u64>,
    pub acquired_food: Vec<u64>,
    pub social_food: game_parallel_fused_cargo::SocialFoodDraft,
    pub family: Option<game_parallel_fused_family::Draft>,
    pub waiting: Vec<usize>,
    pub receipts: Receipts,
    pub delivered: usize,
    pub skills: Option<game_parallel::Skills>,
    pub goals: Option<game_parallel::Goals>,
    pub dreams: Option<game_parallel::Dreams>,
    pub purpose: Option<game_parallel::Purpose>,
    pub disease: Option<Vec<game_parallel::DiseaseEvent>>,
}
pub struct Context<'a> {
    pub family: Option<&'a mut game_parallel_fused_family::Runtime>,
    pub world: &'a World,
    pub blocks: &'a mut [Block],
    pub positions: &'a [(u64, usize, usize)],
    pub jobs: &'a Store,
    pub waiting: &'a [usize],
    pub food: &'a [u32],
}
impl Context<'_> {
    pub fn prepare(&mut self, next: u64, decisions: &mut Decisions) -> Result<Draft, &'static str> {
        let Self {
            family,
            world,
            blocks,
            positions,
            jobs,
            waiting,
            food,
        } = self;
        let rules = &world.rules;
        let max_work_hunger = world
            .production
            .as_ref()
            .map_or(0, |production| production.max_work_hunger);
        let game_parallel_fused_food::FoodPlan {
            stores,
            deposits,
            builds,
            diet,
            carried_meals,
            meal_debits,
            seekers,
            care_fetch_children,
            due_seekers,
            actors: food_actors,
            aid: food_aid,
            resolved: resolved_food_requests,
            automatic: automatic_food_requests,
        } = game_parallel_fused_food::prepare(
            world,
            blocks,
            positions,
            decisions.caregiving.as_ref(),
            &decisions.planned,
            &decisions.farm_restricted,
            next,
        )?;
        let mut farm_busy = Vec::new();
        let planned = if world.agriculture.is_some()
            && (seekers.is_some()
                || !food_actors.is_empty()
                || !decisions.farm_restricted.is_empty())
        {
            farm_busy.extend_from_slice(&decisions.planned);
            farm_busy.extend_from_slice(&decisions.farm_restricted);
            if let Some(seekers) = &seekers {
                farm_busy.extend_from_slice(seekers);
            }
            farm_busy.extend_from_slice(&food_actors);
            if farm_busy.par_windows(2).any(|p| p[0] > p[1]) {
                farm_busy.par_sort_unstable();
            }
            farm_busy.as_slice()
        } else {
            decisions.planned.as_slice()
        };
        let farming = game_parallel_fused_agriculture::prepare(world, blocks, food, planned, next);
        let source_food = farming.as_ref().map_or(*food, |(_, food)| food.as_slice());
        let growth = world
            .environment
            .as_ref()
            .and_then(|environment| environment.season(next))
            .map_or(rules.food_growth, |season| {
                season.food_growth(rules.food_growth)
            });
        let grown: Vec<_> = source_food
            .par_iter()
            .map(|food| food.saturating_add(growth).min(rules.food_capacity))
            .collect();
        let has_work = world.skills.is_some()
            && !jobs.is_empty()
            && blocks.par_iter().any(|block| {
                block
                    .work
                    .iter()
                    .any(|&handle| handle != game_parallel_fused_data::NONE)
            });
        let skills_index = world
            .skills
            .as_ref()
            .filter(|skills| has_work && (!skills.records.is_empty() || !skills.lessons.is_empty()))
            .map(|skills| skills.index());
        let fatigue_rules = world.fatigue.as_ref().map(|fatigue| &fatigue.rules);
        let mut busy: Vec<_> = if fatigue_rules.is_some() {
            world
                .conflict
                .as_ref()
                .into_iter()
                .flat_map(|conflict| conflict.actors())
                .chain(
                    world
                        .care
                        .as_ref()
                        .into_iter()
                        .flat_map(|care| care.actors()),
                )
                .collect()
        } else {
            Vec::new()
        };
        if fatigue_rules.is_some() {
            busy.extend_from_slice(&food_actors);
            if let Some(seekers) = &seekers {
                busy.extend_from_slice(seekers);
            }
        }
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        if let Some(index) = skills_index {
            let invoked = world
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.invocation.as_ref())
                .filter(|invocation| !invocation.active.is_empty());
            if let Some(invocation) = invoked {
                game_parallel_fused_needs::prepare_with_rate(
                    blocks,
                    rules,
                    max_work_hunger,
                    fatigue_rules,
                    &busy,
                    (&carried_meals, seekers.as_deref(), &care_fetch_children),
                    |handle, person| {
                        let recipe = jobs.row(handle).recipe;
                        let own = index.rate(person, recipe);
                        if invocation.owner == Some(person) {
                            invocation
                                .active
                                .iter()
                                .fold(own, |rate, life| rate.max(index.rate(*life, recipe)))
                        } else {
                            own
                        }
                    },
                );
            } else {
                game_parallel_fused_needs::prepare_with_rate(
                    blocks,
                    rules,
                    max_work_hunger,
                    fatigue_rules,
                    &busy,
                    (&carried_meals, seekers.as_deref(), &care_fetch_children),
                    |handle, person| index.rate(person, jobs.row(handle).recipe),
                );
            }
        } else {
            game_parallel_fused_needs::prepare(
                blocks,
                rules,
                max_work_hunger,
                fatigue_rules,
                &busy,
                (&carried_meals, seekers.as_deref(), &care_fetch_children),
            );
        }
        if let Some(diet) = &diet {
            diet.apply_hunger(blocks, positions, rules.hunger_per_tick);
        }
        let completions: Vec<_> = if world.skills.is_some() || world.goals.is_some() {
            blocks
                .par_iter()
                .flat_map_iter(|block| {
                    block.finished.iter().map(|&handle| {
                        let job = jobs.row(handle);
                        game_parallel::SkillCompletion {
                            job: job.id,
                            worker: job.worker,
                            recipe: job.recipe,
                            finished_at: next,
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let work_memory = work_memory::prepare(world, blocks, positions, jobs);
        let teaching = world.skills.as_ref().and_then(|skills| {
            if skills.teachers.is_empty() {
                return None;
            }
            let pairs: Vec<_> = decisions
                .society
                .social
                .as_ref()
                .map_or_else(Vec::new, |social| {
                    social
                        .events
                        .par_iter()
                        .map(|event| (event.a, event.b))
                        .collect()
                });
            skills.plan_teaching(&pairs, decisions.lineage.controlled, next)
        });
        let mut skills = world
            .skills
            .as_ref()
            .filter(|_| !completions.is_empty())
            .map(|skills| skills.prepare(&completions, next));
        if let Some(teaching) = teaching {
            let staged = skills.get_or_insert_with(|| {
                let mut staged = world.skills.as_ref().expect("enabled skills").clone();
                staged.events.clear();
                staged
            });
            teaching.commit(staged);
        }
        let goals = world
            .goals
            .as_ref()
            .map(|goals| {
                let completed: Vec<_> = completions
                    .par_iter()
                    .map(|row| game_parallel::GoalCompletion {
                        person: row.worker,
                        recipe: row.recipe,
                    })
                    .collect();
                goals.prepare(&completed, &decisions.goal_blocked, next)
            })
            .transpose()?
            .flatten();
        let purpose = world.purpose.as_ref().map(|purpose| {
            game_parallel_fused_purpose::prepare(purpose, goals.as_ref(), blocks, positions, next)
        });
        let mut disease = world
            .disease
            .as_ref()
            .map(|disease| {
                let encounters = decisions
                    .society
                    .social
                    .as_ref()
                    .map_or(&[][..], |social| social.events.as_slice());
                game_parallel_fused_disease::prepare(
                    disease, blocks, positions, encounters, world.seed, world.tick,
                )
            })
            .transpose()?;
        let rotation = world
            .cargo
            .as_ref()
            .filter(|cargo| cargo.personal_food_access)
            .map(|_| (world.seed, next));
        let allocation = game_parallel_fused_needs::food::allocate(blocks, &grown, rules, rotation);
        let mut acquired_food = if seekers.is_some() {
            game_parallel_fused_needs::food::acquired(blocks, &allocation)
        } else {
            Vec::new()
        };
        if let Some(stores) = &stores {
            acquired_food.extend_from_slice(&stores.acquired);
            if acquired_food.par_windows(2).any(|p| p[0] > p[1]) {
                acquired_food.par_sort_unstable();
            }
        }
        let output = game_parallel_fused_output::prepare(
            blocks,
            jobs,
            waiting,
            world.production.as_ref(),
            next,
        )?;
        game_parallel_fused_needs::finalize(
            blocks,
            rules,
            &allocation,
            fatigue_rules,
            world.disease.as_ref(),
            world.tick,
            seekers.is_some(),
        );
        game_parallel_fused_diet::apply_due(world, blocks, positions, next, rules.max_health);
        game_parallel_fused_nutrition::prepare(world, blocks, &allocation, diet.as_ref(), next);
        if let Some((person, damage)) = decisions.lineage.strain {
            let index = positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "invocation owner missing")?;
            let (_, block, offset) = positions[index];
            blocks[block].next_health[offset] =
                blocks[block].next_health[offset].saturating_sub(damage);
        }
        game_parallel_fused_needs::environment::apply(
            blocks,
            &world.regions,
            world.environment.as_ref(),
            world
                .production
                .as_ref()
                .map_or(&[], |production| production.inventories.as_slice()),
            next,
        );
        if let Some(purpose) = &purpose {
            game_parallel_fused_purpose::apply(purpose, blocks, positions, next);
        }
        if let Some(caregiving) = &decisions.caregiving {
            game_parallel_fused_dependent_care::apply(
                caregiving,
                world
                    .caregiving
                    .as_ref()
                    .expect("prepared caregiving")
                    .rules
                    .neglect_damage,
                blocks,
                positions,
            );
        }
        game_parallel_fused_conflict::packed::apply(decisions.conflict.as_ref(), blocks, positions);
        if let Some(care) = &mut decisions.care {
            care.apply(blocks, positions, rules.max_health);
            if let (Some(state), Some(events)) = (&world.disease, &mut disease) {
                let patients: Vec<_> = care
                    .state
                    .events
                    .par_iter()
                    .filter(|event| !event.interrupted)
                    .map(|event| event.patient)
                    .collect();
                game_parallel_fused_disease::treat(
                    state,
                    blocks,
                    care.state.medicine,
                    &patients,
                    next,
                    events,
                );
            }
        }
        let family = game_parallel_fused_family::prepare_optional(
            world,
            family.as_deref_mut(),
            blocks,
            positions,
            jobs,
            next,
            decisions.lineage.lifespan_loss,
        )?;
        let (dreams, goals) = dreams::prepare(world, goals, blocks, positions, next)?;
        game_parallel_fused_conflict::packed::finish(
            decisions.conflict.as_mut(),
            world.conflict.as_ref(),
            blocks,
            positions,
        );
        if let Some(care) = &mut decisions.care {
            care.finish(blocks, positions);
        }
        let mut food: Vec<_> = grown
            .par_iter()
            .zip(allocation.meals.par_iter())
            .map(|(&food, &meals)| food - (meals as u32) * rules.food_per_meal)
            .collect();
        let shipments = if world.shipments.is_empty() {
            None
        } else {
            let (updated_food, shipments) = game_parallel_block_logistics::advance(
                &world.regions,
                &food,
                &world.shipments,
                rules,
            );
            food = updated_food;
            Some(shipments)
        };
        Ok(Draft {
            food_stores: stores,
            food_deposits: deposits,
            store_builds: builds,
            diet,
            work_memory,
            food,
            updates: output.updates,
            agriculture: farming.map(|(agriculture, _)| agriculture),
            shipments,
            carried_meals: meal_debits.unwrap_or(carried_meals),
            acquired_food,
            social_food: game_parallel_fused_cargo::SocialFoodDraft {
                gifts: food_aid.map_or_else(Vec::new, |aid| aid.gifts),
                resolved: resolved_food_requests,
                automatic: automatic_food_requests,
                due_seekers,
            },
            family,
            waiting: output.waiting,
            receipts: output.receipts,
            delivered: output.delivered,
            skills,
            goals,
            dreams,
            purpose,
            disease,
        })
    }
}
