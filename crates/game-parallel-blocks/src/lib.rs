mod actions;
mod departure;
mod index;
use game_parallel::World;
use game_parallel_block_data::Block;
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
use std::mem;
pub struct Engine {
    template: World,
    blocks: Vec<Block>,
    food: Vec<u32>,
    grown: Vec<u32>,
    claims: Vec<usize>,
    meals: Vec<usize>,
    people: Vec<PersonSnapshot>,
    positions: Vec<(usize, usize)>,
    packed_jobs: Option<game_parallel_production_packed::PackedJobs>,
    packed_goods: Option<game_parallel_goods_packed::PackedCargo>,
}
impl Engine {
    /// Compile the currently supported physical subset without discarding owners.
    pub fn compile(world: World) -> Result<Self, String> {
        let compiled = game_parallel_block_import::compile(world)?;
        let mut world = compiled.world;
        let blocks = compiled.blocks;
        let food = compiled.food;
        let count = food.len();
        let packed_jobs = game_parallel_block_production::extract(&mut world);
        let mut engine = Self {
            template: world,
            blocks,
            food,
            grown: vec![0; count],
            claims: vec![0; count],
            meals: vec![0; count],
            people: Vec::new(),
            positions: Vec::new(),
            packed_jobs,
            packed_goods: None,
        };
        if game_parallel_block_import::needs_people(&engine.template) {
            engine.rebuild_people();
        }
        let packed_goods_enabled = engine.packed_jobs.is_some() && engine.template.market.is_none();
        engine.packed_goods = game_parallel_block_goods::extract(
            &mut engine.template,
            packed_goods_enabled,
            &engine.people,
        );
        Ok(engine)
    }
    /// Compute into private buffers; an overflow leaves the published generation unchanged.
    pub fn step(&mut self) -> Result<(), &'static str> {
        let next = self.template.tick.checked_add(1).ok_or("tick overflow")?;
        let invocation = self
            .template
            .lineage
            .as_mut()
            .map(|lineage| lineage.prepare_invocation(self.template.tick))
            .transpose()?
            .flatten();
        let guardians = self.template.guardians.as_ref().map(|guardians| {
            let target = self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.current)
                .map(|id| {
                    let region = game_parallel_production_model::person_index(&self.people, id)
                        .and_then(|index| self.people[index].1);
                    (id, region)
                });
            guardians.advance(target, &self.template.routes)
        });
        let institutions = self
            .template
            .institutions
            .as_ref()
            .map(|institutions| institutions.prepare(&self.people, next));
        let meals = if let Some(cargo) = &self.template.cargo {
            game_parallel_block_food_cargo::prepare_meals(cargo, &self.people, &self.template.rules)
                .map_err(|_| "invalid personal cargo")?
        } else {
            Vec::new()
        };
        // Compiled owners are private, validated on import, and mutated only by
        // their checked commands and staged commits. Revalidating all carried
        // goods and jobs each tick would add a population-wide serial boundary.
        if let Some(agriculture) = &self.template.agriculture {
            agriculture
                .validate(&self.template.regions)
                .map_err(|_| "invalid agriculture state")?;
        }
        let mut busy = game_parallel_block_production::busy_for_tick(
            &self.template,
            self.packed_jobs.as_ref(),
        );
        let trade = if let Some(market) = &self.template.market {
            let production = self
                .template
                .production
                .as_ref()
                .ok_or("market requires production")?;
            let cargo = self
                .template
                .goods_cargo
                .as_ref()
                .ok_or("market requires goods cargo")?;
            game_parallel_market_auto::prepare_with_opinions(
                market,
                cargo,
                &self.people,
                &production.goods,
                &busy,
                next,
                self.template.opinions.as_ref(),
            )
            .map_err(|_| "invalid automatic trade")?
        } else {
            None
        };
        if let Some(trade) = &trade {
            busy.extend_from_slice(&trade.actors);
        }
        let work = if let Some(autowork) = &self.template.autowork {
            let state = self
                .template
                .production
                .as_ref()
                .ok_or("autowork requires production")?;
            game_parallel_block_production::prepare_work(
                autowork,
                state,
                self.packed_jobs.as_ref(),
                &self.people,
                &busy,
                self.template.property.as_ref(),
            )
            .map_err(|_| "invalid autonomous work")?
        } else {
            None
        };
        if let Some(work) = &work {
            busy.extend_from_slice(work.actors());
        }
        let social = self
            .template
            .social
            .as_ref()
            .map(|social| {
                game_parallel_social::adapter::prepare_indexed(
                    social,
                    &self.people,
                    &busy,
                    self.template.seed,
                    next,
                )
            })
            .transpose()?;
        let knowledge = self
            .template
            .knowledge
            .as_ref()
            .map(|knowledge| {
                game_parallel_knowledge::adapter::prepare_indexed(
                    knowledge,
                    &self.template.regions,
                    &self.food,
                    &self.people,
                    social.as_ref(),
                    self.template.tick,
                )
            })
            .transpose()?;
        if let Some(social) = &social {
            busy.extend(social.events.iter().flat_map(|event| [event.a, event.b]));
        }
        let autonomy = if let Some(autonomy) = &self.template.autonomy {
            let knowledge = self
                .template
                .knowledge
                .as_ref()
                .ok_or("autonomy requires knowledge")?;
            Some(game_parallel_autonomy::adapter::prepare_indexed(
                autonomy,
                knowledge,
                &self.template.regions,
                &self.people,
                &self.template.routes,
                &busy,
                self.template.tick,
            )?)
        } else {
            None
        };
        if let Some(autonomy) = &autonomy {
            busy.extend(autonomy.decisions.iter().map(|decision| decision.person));
        }
        let production = if let Some(production) = &self.template.production {
            Some(game_parallel_block_production::prepare(
                production,
                self.packed_jobs.as_ref(),
                mem::take(&mut self.people),
                next,
            ))
        } else {
            None
        };
        let farming = if let Some(agriculture) = &self.template.agriculture {
            let mut agriculture = agriculture.clone();
            let mut regions = self.template.regions.clone();
            for (region, &food) in regions.iter_mut().zip(&self.food) {
                region.food = food;
            }
            let people = production
                .as_ref()
                .map_or(self.people.as_slice(), |draft| draft.people());
            game_parallel_agriculture::advance_indexed(
                &mut regions,
                &mut agriculture,
                self.template.rules.food_capacity,
                people,
                &busy,
            );
            Some((agriculture, regions))
        } else {
            None
        };
        let rules = &self.template.rules;
        let farm_food = farming
            .as_ref()
            .map(|(_, regions)| regions.iter().map(|region| region.food).collect::<Vec<_>>());
        let source_food = farm_food.as_deref().unwrap_or(&self.food);
        game_parallel_block_physical::Context {
            blocks: &mut self.blocks,
            food: source_food,
            grown: &mut self.grown,
            claims: &mut self.claims,
            meals: &mut self.meals,
            rules,
            carried_meals: &meals,
        }
        .prepare();
        let shipment = if self.template.shipments.is_empty() {
            None
        } else {
            let food: Vec<_> = self
                .grown
                .par_iter()
                .zip(self.meals.par_iter())
                .map(|(&grown, &meals)| grown - (meals as u32) * rules.food_per_meal)
                .collect();
            Some(game_parallel_block_logistics::advance(
                &self.template.regions,
                &food,
                &self.template.shipments,
                rules,
            ))
        };
        let family = if let Some(families) = &self.template.families {
            let people = production
                .as_ref()
                .map_or(self.people.as_slice(), |draft| draft.people());
            match (game_parallel_block_family::Context {
                blocks: &self.blocks,
                regions: &self.template.regions,
                people,
                positions: &self.positions,
                families,
                seed: self.template.seed,
                tick: next,
                max_health: self.template.rules.max_health,
            })
            .prepare()
            {
                Ok(draft) => Some(draft),
                Err(error) => {
                    if let Some(production) = production {
                        self.people = production.discard();
                    }
                    return Err(error);
                }
            }
        } else {
            None
        };
        if let Some(family) = &family {
            family.mark_deaths(&mut self.blocks);
        }
        // No operation below can fail: publish disjoint blocks and regional totals.
        game_parallel_block_physical::publish(&mut self.blocks);
        self.food
            .par_iter_mut()
            .enumerate()
            .for_each(|(region, food)| {
                *food = shipment.as_ref().map_or_else(
                    || self.grown[region] - (self.meals[region] as u32) * rules.food_per_meal,
                    |(food, _)| food[region],
                )
            });
        let arrived = game_parallel_block_physical::advance_journeys(&mut self.blocks);
        let born = family
            .is_some_and(|family| family.commit(&mut self.blocks, &mut self.template.families));
        if (arrived > 0 || born)
            && self
                .blocks
                .par_windows(2)
                .any(|pair| (pair[0].region, pair[0].ids[0]) > (pair[1].region, pair[1].ids[0]))
        {
            self.blocks
                .par_sort_unstable_by_key(|block| (block.region, block.ids[0]));
        }
        if let Some(production) = production {
            self.people = production.commit(
                self.template
                    .production
                    .as_mut()
                    .expect("prepared production"),
                &mut self.packed_jobs,
            );
            if arrived > 0 || born {
                self.rebuild_people();
            } else {
                self.refresh_people();
            }
        }
        if self.template.production.is_none()
            && game_parallel_block_import::needs_people(&self.template)
        {
            if arrived > 0 || born {
                self.rebuild_people();
            } else {
                self.refresh_people();
            }
        }
        if !meals.is_empty() {
            game_parallel_block_food_cargo::commit_meals(
                self.template.cargo.as_mut().expect("prepared cargo"),
                &meals,
                self.template.rules.food_per_meal,
            );
        }
        if let Some((_, shipments)) = shipment {
            self.template.shipments = shipments;
        }
        if let Some((agriculture, _)) = farming {
            self.template.agriculture = Some(agriculture);
        }
        if let Some(work) = work {
            work.commit_world(&mut self.template, &mut self.packed_jobs);
        }
        if let Some(trade) = trade {
            trade.commit(
                self.template.market.as_mut().expect("prepared market"),
                self.template.goods_cargo.as_mut().expect("prepared cargo"),
                self.template.opinions.as_mut(),
            );
        }
        if let Some(social) = social {
            self.template.social = Some(social);
        }
        if let Some(knowledge) = knowledge {
            self.template.knowledge = Some(knowledge);
        }
        if let Some(invocation) = invocation {
            self.template.lineage.as_mut().unwrap().invocation = Some(invocation);
        }
        game_parallel_institutions::commit_optional(institutions, &mut self.template.institutions);
        if let (Some(lineage), Some(families)) =
            (&mut self.template.lineage, &self.template.families)
        {
            lineage.advance(families, next);
        }
        if let Some(guardians) = guardians {
            self.template.guardians = Some(guardians);
        }
        self.template.tick = next;
        if let Some(draft) = autonomy {
            let (requests, available) =
                game_parallel_autonomy::adapter::requests_indexed(&draft, &self.people);
            let result = if requests.is_empty() {
                Ok(())
            } else {
                self.move_people_autonomous(&requests)
            };
            self.template.autonomy = Some(game_parallel_autonomy::adapter::finish(
                draft,
                &available,
                &result,
                self.template.tick,
            ));
        }
        game_parallel_family_host::enroll_births!(&mut self.template);
        Ok(())
    }
    pub fn tick(&self) -> u64 {
        self.template.tick
    }
    /// Export only committed generations; the returned world may be edited and recompiled.
    pub fn export(&self) -> World {
        let mut world =
            game_parallel_block_export::export(&self.template, &self.blocks, &self.food);
        game_parallel_block_production::export(&mut world, self.packed_jobs.as_ref());
        game_parallel_block_goods::export(&mut world, self.packed_goods.as_ref(), &self.people);
        world
    }
}

#[cfg(test)]
#[path = "../tests/support/physical.rs"]
mod physical_tests;
