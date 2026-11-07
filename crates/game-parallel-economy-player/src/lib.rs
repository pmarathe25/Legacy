//! Ruler-bound choices and observations, independent of administrative inspection.
use game_parallel_economy::{
    Departure, Economy, MissionStart, Person, PromiseDecision, PromiseProposal,
};
use game_parallel_economy_campaign::{InvocationWarning, Life, ReleaseDecision};
use game_parallel_economy_combat::{Map, Order, Point};
use game_parallel_economy_knowledge::Record;
use game_parallel_lot_runtime::Phase;
use game_parallel_lots::{Lot, Place};
use game_parallel_production_model::{Good, Recipe};
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub recipe: u32,
    pub remaining: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillView {
    pub recipe: u32,
    pub personal_rate: u64,
    pub effective_rate: u64,
    pub work_credits: u64,
    pub lesson_credits: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkSiteView {
    pub site: game_parallel_economy::WorkSite,
    pub occupied: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebrisView {
    pub site: u64,
    pub lots: Vec<Lot>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalStoreView {
    pub store: game_parallel_economy::LocalStore,
    pub capacity: u64,
    pub lots: Vec<Lot>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FighterView {
    pub person: u64,
    pub side: u32,
    pub position: Point,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattleView {
    pub encounter: u64,
    pub map: Map,
    pub fighters: Vec<FighterView>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreatmentView {
    pub medicine: u32,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conditions {
    /// Conditions used at the next world boundary.
    pub season: Option<game_parallel_economy::Season>,
    pub local_hazard: Option<game_parallel_economy::Hazard>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyView {
    pub person: u64,
    pub possessions: Vec<Lot>,
}
/// Explicit search spectating combines guardian judgment with their actual body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardianView {
    pub person: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub journey: Option<game_parallel_economy::Journey>,
    pub student: Option<u64>,
    pub mentorship: u64,
    pub mentorship_required: u64,
    pub released: bool,
    pub confined: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimalView {
    pub id: u64,
    pub species: u32,
    pub health: u32,
    /// A visible physical hold, not an ownership claim.
    pub held_by: Option<u64>,
    /// This person's own feeling, when they have a configured animal mind.
    pub affinity: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimView {
    pub region: u64,
    pub good: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteObservation {
    pub tick: u64,
    pub person: u64,
    pub site: u64,
    pub destroyed: bool,
    pub salvaged: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub tick: u64,
    pub person: Option<Person>,
    pub journey: Option<game_parallel_economy::Journey>,
    pub held: bool,
    pub nearby: Vec<u64>,
    pub bodies: Vec<BodyView>,
    /// Observable local animals, without their genetics or hidden pregnancy.
    pub animals: Vec<AnimalView>,
    pub possessions: Vec<Lot>,
    pub local_stock: Vec<Lot>,
    /// The ruler's own assertions, including assertions made in other regions.
    pub claims: Vec<ClaimView>,
    /// The ruler's assertions about sites, including sites since destroyed.
    pub site_claims: Vec<u64>,
    pub claim_built_sites: Option<bool>,
    pub site_observations: Vec<SiteObservation>,
    pub food_reports: Vec<Record>,
    pub job: Option<Job>,
    /// Only the controlled person's own goals are visible.
    pub goals: Vec<game_parallel_economy::Goal>,
    pub warning: Option<InvocationWarning>,
    pub past_lives: Vec<Life>,
    pub active_lives: Vec<u64>,
    pub invocation_burden: u64,
    pub invocation_exposure: u64,
    pub expeditions: Vec<game_parallel_missions::Mission>,
    pub promises: Vec<game_parallel_agreements::Contract>,
    pub battle: Option<BattleView>,
    pub pending_work_requests: Vec<game_parallel_autowork::requests::Request>,
    pub food_requests: Vec<game_parallel_economy::FoodRequest>,
    pub sharing_learning: Option<game_parallel_economy::SharingLearning>,
    pub practices: Option<game_parallel_economy::PracticeMind>,
    pub relationships: Vec<game_parallel_economy::Opinion>,
    pub hauling_requests: Vec<game_parallel_economy::HaulingRequest>,
    pub observed_work: Vec<game_parallel_autowork::requests::WorkLink>,
    pub work_memories: Vec<game_parallel_autowork::requests::Memory>,
    /// The controlled person's own selected medicine, not a diagnosis.
    pub treatment: Option<TreatmentView>,
    pub fields: Vec<game_parallel_economy::Field>,
    pub resource_sites: Vec<game_parallel_economy::ResourceNode>,
    pub work_sites: Vec<WorkSiteView>,
    pub debris: Vec<DebrisView>,
    pub local_stores: Vec<LocalStoreView>,
    pub fatigue: Option<(u32, bool)>,
    pub conditions: Option<Conditions>,
    pub isolation: Option<u32>,
    pub purpose_deprivation: Option<u32>,
}
pub struct Player<'a> {
    world: &'a mut Economy,
}
impl<'a> Player<'a> {
    pub fn new(world: &'a mut Economy) -> Self {
        Self { world }
    }
    fn current(&self) -> Result<u64, String> {
        let state = self.world.read();
        let campaign = state.campaign.ok_or("campaign is disabled")?;
        campaign
            .lineage
            .controlled_person(self.world.tick())
            .ok_or("no ruler under player control".into())
    }
    pub fn view(&self) -> Result<View, String> {
        let state = self.world.read();
        let campaign = state.campaign.ok_or("campaign is disabled")?;
        let current = campaign.lineage.controlled_person(self.world.tick());
        let person = current
            .and_then(|id| {
                state
                    .people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .map(|i| &state.people[i])
            })
            .cloned();
        let locations = game_parallel_economy::Colocation::new(state.travel);
        let mut nearby: Vec<_> = person.as_ref().map_or_else(Vec::new, |actor| {
            state
                .people
                .par_iter()
                .filter(|p| {
                    p.id != actor.id
                        && p.health > 0
                        && locations
                            .region((actor.id, actor.region), (p.id, p.region))
                            .is_some()
                })
                .map(|p| p.id)
                .collect()
        });
        let journey = current.and_then(|id| {
            state.travel.and_then(|travel| {
                travel
                    .journeys
                    .par_iter()
                    .find_any(|j| j.person == id)
                    .cloned()
            })
        });
        if nearby.par_windows(2).any(|pair| pair[0] > pair[1]) {
            nearby.par_sort_unstable();
        }
        let bodies = person
            .as_ref()
            .and_then(|p| p.region)
            .map_or_else(Vec::new, |region| {
                state
                    .people
                    .par_iter()
                    .filter(|p| {
                        p.health == 0
                            && p.region == Some(region)
                            && state.confinement.is_none_or(|owner| !owner.held(p.id))
                    })
                    .map(|p| BodyView {
                        person: p.id,
                        possessions: state
                            .physical
                            .custody(Place::Person(p.id))
                            .map_or_else(Vec::new, |c| c.lots.clone()),
                    })
                    .collect()
            });
        let possessions = current
            .and_then(|id| state.physical.custody(Place::Person(id)))
            .map_or_else(Vec::new, |c| c.lots.clone());
        let local_stock = person
            .as_ref()
            .and_then(|p| p.region)
            .map_or_else(Vec::new, |region| {
                let mut lots: Vec<_> = state
                    .physical
                    .goods()
                    .par_iter()
                    .filter_map(|good| {
                        state.physical.custody(Place::Stock {
                            region,
                            good: good.id,
                        })
                    })
                    .flat_map_iter(|container| container.lots.iter().cloned())
                    .collect();
                lots.par_sort_unstable_by_key(|lot| {
                    (
                        lot.good,
                        lot.expires_at.is_none(),
                        lot.expires_at.unwrap_or(0),
                    )
                });
                lots
            });
        let food_reports = current
            .and_then(|id| {
                state
                    .knowledge
                    .and_then(|k| k.minds.par_iter().find_any(|m| m.person == id))
            })
            .map_or_else(Vec::new, |m| m.records.clone());
        let job = current
            .and_then(|id| {
                state
                    .physical
                    .work()
                    .par_iter()
                    .find_any(|w| w.job.worker == id && w.phase == Phase::Working)
            })
            .map(|w| Job {
                id: w.job.id,
                recipe: w.job.recipe,
                remaining: w.job.remaining,
            });
        let warning = current.and_then(|_| {
            campaign
                .lineage
                .invocation
                .as_ref()
                .and_then(|i| i.pending.clone())
        });
        let invocation = current.and(campaign.lineage.invocation.as_ref());
        let expeditions = current.map_or_else(Vec::new, |id| {
            state.missions.map_or_else(Vec::new, |m| {
                m.entries
                    .par_iter()
                    .filter(|m| m.person == id)
                    .cloned()
                    .collect()
            })
        });
        let promises = current.map_or_else(Vec::new, |id| {
            state.agreements.map_or_else(Vec::new, |owner| {
                owner
                    .contracts
                    .par_iter()
                    .filter(|c| c.debtor == id || c.creditor == id)
                    .cloned()
                    .collect()
            })
        });
        let battle = current.and_then(|id| {
            let combat = state.combat?;
            let fight = combat
                .conflict
                .encounters
                .par_iter()
                .find_any(|f| f.fighters.par_iter().any(|p| p.person == id))?;
            let placements = combat
                .tactics
                .battles
                .iter()
                .find(|b| b.encounter == fight.id)?;
            let map = combat
                .tactics
                .maps
                .iter()
                .find(|m| m.region == fight.region)?
                .clone();
            let mut ordered: Vec<_> = placements.people.par_iter().collect();
            if ordered
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                ordered.par_sort_unstable_by_key(|p| p.person);
            }
            let first = ordered.first().and_then(|first| {
                ordered.last().and_then(|last| {
                    (last.person.checked_sub(first.person) == Some((ordered.len() - 1) as u64))
                        .then_some(first.person)
                })
            });
            let fighters = fight
                .fighters
                .par_iter()
                .filter_map(|f| {
                    first
                        .and_then(|first| f.person.checked_sub(first))
                        .and_then(|i| usize::try_from(i).ok())
                        .filter(|&i| i < ordered.len())
                        .or_else(|| {
                            first
                                .is_none()
                                .then(|| ordered.binary_search_by_key(&f.person, |p| p.person).ok())
                                .flatten()
                        })
                        .map(|i| FighterView {
                            person: f.person,
                            side: f.side,
                            position: ordered[i].position,
                        })
                })
                .collect();
            Some(BattleView {
                encounter: fight.id,
                map,
                fighters,
            })
        });
        let requests = state.autowork.and_then(|a| a.requests.as_ref());
        let purpose_deprivation = current.and_then(|id| {
            state.purpose.and_then(|owner| {
                owner
                    .people
                    .par_iter()
                    .find_any(|p| p.person == id)
                    .map(|p| p.deprivation)
            })
        });
        let isolation = current.and_then(|id| {
            let social = state.dynamics?.social.as_ref()?;
            let needs = social.needs.as_ref()?;
            social.participants.iter().any(|p| p.person == id).then(|| {
                needs
                    .people
                    .par_iter()
                    .find_any(|p| p.person == id)
                    .map_or(0, |p| p.isolation)
            })
        });
        let conditions = person.as_ref().and_then(|person| {
            let environment = state.environment?;
            Some(Conditions {
                season: environment.season(state.tick.saturating_add(1)).cloned(),
                local_hazard: person
                    .region
                    .and_then(|region| environment.regions.iter().find(|h| h.region == region))
                    .cloned(),
            })
        });
        let fatigue = current.and_then(|id| {
            state.fatigue.map(|owner| {
                owner
                    .people
                    .par_iter()
                    .find_any(|p| p.id == id)
                    .map_or((0, false), |p| (p.fatigue, p.resting))
            })
        });
        let fields = person
            .as_ref()
            .and_then(|p| p.region)
            .and_then(|region| {
                state.agriculture.map(|a| {
                    a.fields
                        .par_iter()
                        .filter(|field| field.region == region)
                        .cloned()
                        .collect()
                })
            })
            .unwrap_or_default();
        let work_sites = person
            .as_ref()
            .and_then(|p| p.region)
            .and_then(|region| {
                state.work_sites.map(|owner| {
                    use std::collections::{HashMap, HashSet};
                    let local: Vec<_> = owner
                        .sites
                        .par_iter()
                        .filter(|s| s.region == region)
                        .collect();
                    let ids: HashSet<_> = local.par_iter().map(|s| s.id).collect();
                    let used = owner
                        .bindings
                        .par_iter()
                        .filter(|b| ids.contains(&b.site))
                        .fold(HashMap::new, |mut m, b| {
                            *m.entry(b.site).or_insert(0u64) += b.slots;
                            m
                        })
                        .reduce(HashMap::new, |mut a, b| {
                            for (site, slots) in b {
                                *a.entry(site).or_insert(0) += slots;
                            }
                            a
                        });
                    local
                        .par_iter()
                        .map(|site| WorkSiteView {
                            site: (*site).clone(),
                            occupied: used.get(&site.id).copied().unwrap_or(0),
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        let debris = person
            .as_ref()
            .and_then(|p| p.region)
            .and_then(|region| {
                state.construction.map(|owner| {
                    owner
                        .materials
                        .par_iter()
                        .filter(|material| material.debris && material.region == region)
                        .filter_map(|material| {
                            state
                                .physical
                                .custody(Place::Site(material.site))
                                .map(|holder| DebrisView {
                                    site: material.site,
                                    lots: holder.lots.clone(),
                                })
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        let local_stores = person
            .as_ref()
            .and_then(|p| p.region)
            .and_then(|region| {
                state.local_transport.map(|owner| {
                    owner
                        .stores
                        .par_iter()
                        .filter(|store| store.region == region)
                        .filter_map(|store| {
                            state
                                .physical
                                .custody(Place::Inventory(store.id))
                                .map(|holder| LocalStoreView {
                                    store: store.clone(),
                                    capacity: holder.capacity,
                                    lots: holder.lots.clone(),
                                })
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        let resource_sites =
            person
                .as_ref()
                .and_then(|p| p.region)
                .map_or_else(Vec::new, |region| {
                    self.world
                        .resource_nodes()
                        .par_iter()
                        .filter(|node| node.region == region)
                        .cloned()
                        .collect()
                });
        let animals = person
            .as_ref()
            .and_then(|p| p.region)
            .map_or_else(Vec::new, |region| {
                self.world
                    .animals()
                    .par_iter()
                    .filter(|animal| animal.region == region)
                    .map(|animal| AnimalView {
                        id: animal.id,
                        species: animal.species,
                        health: animal.health,
                        held_by: self.world.animal_handler(animal.id),
                        affinity: current
                            .and_then(|person| self.world.animal_affinity(person, animal.id)),
                    })
                    .collect()
            });
        let treatment = current.and_then(|id| {
            state
                .dynamics?
                .treatment
                .iter()
                .find(|policy| policy.person == id)
                .map(|policy| TreatmentView {
                    medicine: policy.medicine,
                    quantity: policy.quantity,
                })
        });
        let pending_work_requests = current
            .and_then(|id| {
                requests.map(|requests| {
                    requests
                        .pending
                        .par_iter()
                        .filter(|r| {
                            r.worker == id
                                || (r.requester == id && nearby.binary_search(&r.worker).is_ok())
                        })
                        .cloned()
                        .collect()
                })
            })
            .unwrap_or_default();
        let observed_work = current
            .and_then(|id| {
                requests.map(|requests| {
                    requests
                        .work_links
                        .par_iter()
                        .filter(|r| r.requester == id && nearby.binary_search(&r.worker).is_ok())
                        .cloned()
                        .collect()
                })
            })
            .unwrap_or_default();
        let work_memories = current
            .and_then(|id| {
                requests.map(|requests| {
                    requests
                        .memories
                        .par_iter()
                        .filter(|r| r.observer == id)
                        .cloned()
                        .collect()
                })
            })
            .unwrap_or_default();
        Ok(View {
            tick: self.world.tick(),
            held: current.is_some_and(|id| state.confinement.is_some_and(|owner| owner.held(id))),
            person,
            journey,
            nearby,
            bodies,
            animals,
            possessions,
            local_stock,
            food_reports,
            job,
            goals: current.map_or_else(Vec::new, |id| {
                state.goals.map_or_else(Vec::new, |owner| {
                    owner
                        .people
                        .par_iter()
                        .filter(|goal| goal.person == id)
                        .cloned()
                        .collect()
                })
            }),
            warning,
            past_lives: current.map_or_else(Vec::new, |_| campaign.lineage.history.clone()),
            active_lives: invocation.map_or_else(Vec::new, |i| i.active.clone()),
            invocation_burden: invocation.map_or(0, |i| i.burden),
            invocation_exposure: invocation.map_or(0, |i| i.exposure),
            expeditions,
            promises,
            battle,
            pending_work_requests,
            food_requests: current.map_or_else(Vec::new, |id| {
                self.world
                    .food_requests()
                    .par_iter()
                    .filter(|r| r.target == id || r.requester == id)
                    .cloned()
                    .collect()
            }),
            claims: current.map_or_else(Vec::new, |id| {
                state
                    .dynamics
                    .and_then(|d| d.claims.as_ref())
                    .map_or_else(Vec::new, |owner| {
                        let mut claims: Vec<_> = owner
                            .claims
                            .par_iter()
                            .filter(|c| c.person == id)
                            .map(|c| ClaimView {
                                region: c.region,
                                good: c.good,
                            })
                            .collect();
                        claims.par_sort_unstable_by_key(|c| (c.region, c.good));
                        claims
                    })
            }),
            site_claims: current.map_or_else(Vec::new, |id| {
                state
                    .dynamics
                    .and_then(|d| d.claims.as_ref())
                    .map_or_else(Vec::new, |owner| {
                        let mut sites: Vec<_> = owner
                            .site_claims
                            .par_iter()
                            .filter(|claim| claim.person == id)
                            .map(|claim| claim.site)
                            .collect();
                        sites.par_sort_unstable();
                        sites
                    })
            }),
            claim_built_sites: current.and_then(|id| {
                state.dynamics.and_then(|d| d.claims.as_ref()).map(|owner| {
                    owner
                        .preferences
                        .par_iter()
                        .find_any(|p| p.person == id)
                        .is_some_and(|p| p.assert_built_sites)
                })
            }),
            site_observations: current.map_or_else(Vec::new, |id| {
                state
                    .dynamics
                    .and_then(|d| d.claims.as_ref())
                    .map_or_else(Vec::new, |owner| {
                        owner
                            .site_events
                            .par_iter()
                            .filter(|event| event.claimant == id)
                            .map(|event| SiteObservation {
                                tick: event.tick,
                                person: event.person,
                                site: event.site,
                                destroyed: event.destroyed,
                                salvaged: event.salvaged,
                            })
                            .collect()
                    })
            }),
            sharing_learning: current
                .and_then(|id| self.world.sharing_learning(id))
                .cloned(),
            practices: current
                .and_then(|id| self.world.personal_practice(id))
                .cloned(),
            relationships: current.map_or_else(Vec::new, |id| {
                self.world
                    .opinions()
                    .par_iter()
                    .filter(|o| o.observer == id)
                    .cloned()
                    .collect()
            }),
            hauling_requests: current.map_or_else(Vec::new, |id| {
                self.world
                    .hauling_requests()
                    .par_iter()
                    .filter(|r| r.requester == id || r.carrier == id)
                    .cloned()
                    .collect()
            }),
            observed_work,
            work_memories,
            treatment,
            fields,
            resource_sites,
            work_sites,
            debris,
            local_stores,
            fatigue,
            conditions,
            isolation,
            purpose_deprivation,
        })
    }
    /// Data-defined goods and production capabilities available to the current person.
    pub fn catalog(&self) -> Result<(Vec<Good>, Vec<Recipe>), String> {
        self.current()?;
        let read = self.world.read();
        Ok((
            read.physical.goods().to_vec(),
            read.physical.recipes().to_vec(),
        ))
    }
    /// The current person's skill and actively invoked capability only.
    pub fn skills(&self) -> Result<Vec<SkillView>, String> {
        let person = self.current()?;
        let read = self.world.read();
        let skills = read.skills.ok_or("skills are disabled")?;
        let index = skills.index();
        let records: std::collections::HashMap<_, _> = skills
            .records
            .par_iter()
            .filter(|record| record.person == person)
            .map(|record| (record.recipe, record.completions))
            .collect();
        let lessons = skills
            .lessons
            .par_iter()
            .filter(|lesson| lesson.student == person)
            .fold(std::collections::HashMap::new, |mut counts, lesson| {
                *counts.entry(lesson.recipe).or_insert(0u64) += 1;
                counts
            })
            .reduce(std::collections::HashMap::new, |mut a, b| {
                for (recipe, count) in b {
                    *a.entry(recipe).or_insert(0u64) += count;
                }
                a
            });
        let invocation = read
            .campaign
            .and_then(|campaign| campaign.lineage.invocation.as_ref())
            .filter(|invocation| invocation.owner == Some(person));
        let mut result: Vec<_> = skills
            .rules
            .par_iter()
            .map(|rule| {
                let personal_rate = index.rate(person, rule.recipe);
                let effective_rate = invocation.map_or(personal_rate, |invocation| {
                    invocation
                        .active
                        .iter()
                        .map(|&life| index.rate(life, rule.recipe))
                        .fold(personal_rate, u64::max)
                });
                SkillView {
                    recipe: rule.recipe,
                    personal_rate,
                    effective_rate,
                    work_credits: records.get(&rule.recipe).copied().unwrap_or(0),
                    lesson_credits: lessons.get(&rule.recipe).copied().unwrap_or(0),
                }
            })
            .collect();
        result.par_sort_unstable_by_key(|row| row.recipe);
        Ok(result)
    }
    /// Work requirements for the same recipe catalog visible to the current person.
    pub fn work_requirements(
        &self,
    ) -> Result<
        (
            Vec<game_parallel_economy::WorkTool>,
            Vec<game_parallel_economy::WorkSiteRequirement>,
        ),
        String,
    > {
        self.current()?;
        let read = self.world.read();
        Ok((
            read.work_tools.to_vec(),
            read.work_sites
                .map_or_else(Vec::new, |s| s.requirements.clone()),
        ))
    }
    /// Authored construction blueprints available to the current person.
    pub fn building_blueprints(
        &self,
    ) -> Result<Vec<game_parallel_economy::BuildingBlueprint>, String> {
        self.current()?;
        Ok(self
            .world
            .read()
            .construction
            .map_or_else(Vec::new, |c| c.blueprints.clone()))
    }
    /// Assemble a site from an actual local or carried building kit.
    pub fn build_site(&mut self, blueprint: u32) -> Result<bool, String> {
        self.world.build_site(self.current()?, blueprint)
    }
    /// Destroy an unoccupied local work site without recovering materials.
    pub fn destroy_site(&mut self, site: u64) -> Result<bool, String> {
        self.world.destroy_site(self.current()?, site)
    }
    /// Recover locally visible debris into the ruler's actual possessions.
    pub fn salvage_site(&mut self, site: u64, good: u32, quantity: u64) -> Result<bool, String> {
        self.world
            .salvage_site(self.current()?, site, good, quantity)
    }
    /// Carry goods into or out of a local store.
    pub fn move_local_store(
        &mut self,
        store: u64,
        good: u32,
        quantity: u64,
        deposit: bool,
    ) -> Result<bool, String> {
        self.world
            .move_local_store(self.current()?, store, good, quantity, deposit)
    }
    /// Explicit guardian-search spectating does not disclose NPC knowledge.
    pub fn guardians(&self) -> Result<Vec<GuardianView>, String> {
        let state = self.world.read();
        let campaign = state.campaign.ok_or("campaign is disabled")?;
        if let Some(guardians) = &campaign.embodied_guardians {
            return guardians
                .guardians
                .iter()
                .map(|guardian| {
                    let person = state
                        .people
                        .binary_search_by_key(&guardian.person, |p| p.id)
                        .ok()
                        .map(|i| &state.people[i])
                        .ok_or("unknown guardian body")?;
                    let journey = state
                        .travel
                        .and_then(|travel| {
                            travel
                                .journeys
                                .binary_search_by_key(&guardian.person, |j| j.person)
                                .ok()
                                .map(|i| &travel.journeys[i])
                        })
                        .cloned();
                    Ok(GuardianView {
                        person: person.id,
                        region: person.region,
                        health: person.health,
                        journey,
                        student: guardian.student,
                        mentorship: guardian.mentorship,
                        mentorship_required: guardian.mentorship_required,
                        released: guardian.released,
                        confined: state.confinement.map(|owner| owner.held(person.id)),
                    })
                })
                .collect();
        }
        let guardians = campaign
            .guardians
            .as_ref()
            .ok_or("guardians are disabled")?;
        Ok(guardians
            .guardians
            .iter()
            .map(|guardian| GuardianView {
                person: guardian.id,
                region: guardian.journey.is_none().then_some(guardian.region),
                health: guardian.health,
                journey: guardian
                    .journey
                    .as_ref()
                    .map(|journey| game_parallel_economy::Journey {
                        person: guardian.id,
                        source: guardian.region,
                        destination: journey.destination,
                        remaining: u64::from(journey.remaining),
                    }),
                student: guardian.student,
                mentorship: guardian.mentorship,
                mentorship_required: guardian
                    .mentorship_required
                    .unwrap_or(guardians.mentorship_ticks),
                released: guardian.released,
                confined: Some(guardian.confined),
            })
            .collect())
    }
    pub fn past_knowledge(&self, life: u64) -> Result<Vec<Record>, String> {
        self.current()?;
        self.world.invoked_knowledge(life)
    }
    pub fn invoke(&mut self, lives: &[u64]) -> Result<(), String> {
        self.current()?;
        self.world.invoke_lives(lives)
    }
    pub fn dismiss_invocation(&mut self) -> Result<(), String> {
        self.current()?;
        self.world.dismiss_invocation()
    }
    pub fn accept_warning(&mut self, revision: u64) -> Result<(), String> {
        self.current()?;
        self.world.accept_invocation_warning(revision)
    }
    pub fn release_guardian(&mut self, guardian: u64) -> Result<ReleaseDecision, String> {
        self.current()?;
        self.world.request_guardian_release(guardian)
    }
    pub fn escape_confinement(&mut self) -> Result<bool, String> {
        self.world.escape_confinement(self.current()?)
    }
    pub fn guard_cell(&mut self, site: u64) -> Result<bool, String> {
        if !self
            .view()?
            .work_sites
            .iter()
            .any(|row| row.site.id == site)
        {
            return Err("cell is not present".into());
        }
        self.world.guard_cell(self.current()?, site)
    }
    pub fn set_capture_yield(&mut self, willing: bool) -> Result<(), String> {
        self.world.set_capture_yield(self.current()?, willing)
    }
    pub fn confine_person(&mut self, target: u64, site: u64) -> Result<bool, String> {
        let view = self.view()?;
        if !(view.bodies.iter().any(|body| body.person == target) || view.nearby.contains(&target))
            || !view.work_sites.iter().any(|row| row.site.id == site)
        {
            return Err("target or cell is not present".into());
        }
        self.world.confine_person(self.current()?, target, site)
    }
    pub fn release_held_person(&mut self, target: u64) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.release_held_person(self.current()?, target)
    }
    pub fn meet(&mut self, target: u64) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.meet(self.current()?, target)
    }
    pub fn teach(&mut self, student: u64, recipe: u32) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&student) {
            return Err("person is not present".into());
        }
        self.world.teach(self.current()?, student, recipe)
    }
    pub fn learn(&mut self, teacher: u64, recipe: u32) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&teacher) {
            return Err("person is not present".into());
        }
        self.world.learn(self.current()?, teacher, recipe)
    }
    pub fn attack(&mut self, target: u64) -> Result<u64, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.attack(self.current()?, target)
    }
    pub fn ask_work(&mut self, worker: u64, recipe: u32, lifetime: u64) -> Result<u64, String> {
        if !self.view()?.nearby.contains(&worker) {
            return Err("person is not present".into());
        }
        self.world
            .ask_work(self.current()?, worker, recipe, lifetime)
    }
    /// Accept or decline a personally received request; accepting attempts real work.
    pub fn respond_work_request(&mut self, request: u64, accept: bool) -> Result<bool, String> {
        let actor = self.current()?;
        let events = self.world.respond_work_request(actor, request, accept)?;
        Ok(accept
            && self
                .world
                .read()
                .physical
                .work()
                .iter()
                .any(|work| work.job.worker == actor && events.started.contains(&work.job.id)))
    }
    pub fn set_goal_help_recognition(&mut self, recipe: u32, enabled: bool) -> Result<(), String> {
        self.world
            .set_goal_help_recognition(self.current()?, recipe, enabled)
    }
    pub fn decide_practice(
        &mut self,
        author: u64,
        sequence: u64,
        approve: bool,
    ) -> Result<(), String> {
        self.world
            .decide_practice(self.current()?, author, sequence, approve)
    }
    pub fn decide_work_practice(
        &mut self,
        author: u64,
        sequence: u64,
        approve: bool,
    ) -> Result<(), String> {
        self.world
            .decide_work_practice(self.current()?, author, sequence, approve)
    }
    pub fn propose_work_practice(
        &mut self,
        worker: u64,
        recipe: u32,
        good: u32,
        quantity: u64,
        lifetime: u64,
    ) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&worker) {
            return Err("person is not present".into());
        }
        self.world
            .propose_work_practice(self.current()?, worker, recipe, good, quantity, lifetime)
    }
    pub fn set_work_practice_choice(&mut self, choice: &str, enabled: bool) -> Result<(), String> {
        self.world
            .set_work_practice_choice(self.current()?, choice, enabled)
    }
    pub fn set_work_practice_reserve(&mut self, reserve: u64) -> Result<(), String> {
        self.world
            .set_work_practice_reserve(self.current()?, reserve)
    }
    pub fn set_work_practice_evaluation(
        &mut self,
        evaluation: Option<game_parallel_economy::WorkPracticeEvaluation>,
    ) -> Result<(), String> {
        self.world
            .set_work_practice_evaluation(self.current()?, evaluation)
    }
    pub fn set_work_outcome_policy(
        &mut self,
        policy: Option<game_parallel_economy::WorkOutcomePolicy>,
    ) -> Result<(), String> {
        self.world.set_work_outcome_policy(self.current()?, policy)
    }
    pub fn set_work_outcome_feelings(
        &mut self,
        kind: &str,
        fear: Option<i64>,
        obligation: Option<i64>,
    ) -> Result<(), String> {
        self.world
            .set_work_outcome_feelings(self.current()?, kind, fear, obligation)
    }
    pub fn report_practice(
        &mut self,
        listener: u64,
        sequence: u64,
        claimed_ready: bool,
    ) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&listener) {
            return Err("person is not present".into());
        }
        self.world
            .report_practice(self.current()?, listener, sequence, claimed_ready)
    }
    pub fn withdraw_practice(&mut self, author: u64, sequence: u64) -> Result<(), String> {
        self.world
            .withdraw_practice(self.current()?, author, sequence)
    }
    pub fn assess_practice_report(
        &mut self,
        speaker: u64,
        sequence: u64,
        belief: game_parallel_economy::PracticeBelief,
    ) -> Result<(), String> {
        self.world
            .assess_practice_report(self.current()?, speaker, sequence, belief)
    }
    pub fn set_practice_require_report(&mut self, enabled: bool) -> Result<(), String> {
        self.world
            .set_practice_require_report(self.current()?, enabled)
    }
    pub fn set_practice_unanswered_belief(
        &mut self,
        belief: Option<game_parallel_economy::PracticeBelief>,
    ) -> Result<(), String> {
        self.world
            .set_practice_unanswered_belief(self.current()?, belief)
    }
    pub fn set_practice_value_learning(
        &mut self,
        value: Option<game_parallel_economy::PracticeValueLearning>,
    ) -> Result<(), String> {
        self.world
            .set_practice_value_learning(self.current()?, value)
    }
    pub fn set_practice_precedent(
        &mut self,
        precedent: Option<game_parallel_economy::PracticePrecedent>,
    ) -> Result<(), String> {
        self.world
            .set_practice_precedent(self.current()?, precedent)
    }
    pub fn set_food_misreport(
        &mut self,
        choice: Option<game_parallel_economy::Misreport>,
    ) -> Result<(), String> {
        self.world.set_food_misreport(self.current()?, choice)
    }
    pub fn food_misreport(&self) -> Result<Option<game_parallel_economy::Misreport>, String> {
        self.world.food_misreport(self.current()?)
    }
    pub fn set_food_report_reception(
        &mut self,
        reception: Option<game_parallel_economy::ReportReception>,
    ) -> Result<(), String> {
        self.world
            .set_food_report_reception(self.current()?, reception)
    }
    pub fn food_report_reception(
        &self,
    ) -> Result<Option<game_parallel_economy::ReportReception>, String> {
        self.world.food_report_reception(self.current()?)
    }
    fn validate_meeting_target(&self, target: Option<u64>) -> Result<(), String> {
        if let Some(target) = target {
            let view = self.view()?;
            if !view.nearby.contains(&target)
                && !view.relationships.iter().any(|row| row.target == target)
            {
                return Err("person is not known".into());
            }
        }
        Ok(())
    }
    pub fn set_social_affinity_weight(
        &mut self,
        target: Option<u64>,
        weight: Option<i32>,
    ) -> Result<(), String> {
        let person = self.current()?;
        self.validate_meeting_target(target)?;
        self.world
            .set_social_affinity_weight(person, target, weight)
    }
    pub fn social_affinity_weight(&self, target: Option<u64>) -> Result<Option<i32>, String> {
        let person = self.current()?;
        self.validate_meeting_target(target)?;
        self.world.social_affinity_weight(person, target)
    }
    pub fn set_promise_trust_reactions(
        &mut self,
        reactions: Option<(i32, i32)>,
    ) -> Result<(), String> {
        self.world
            .set_promise_trust_reactions(self.current()?, reactions)
    }
    pub fn promise_trust_reactions(&self) -> Result<(Option<i32>, Option<i32>), String> {
        self.world.promise_trust_reactions(self.current()?)
    }
    pub fn set_confinement_reaction(
        &mut self,
        kind: game_parallel_economy::ConfinementExperienceKind,
        value: Option<game_parallel_economy::SubjectiveReaction>,
    ) -> Result<(), String> {
        self.world
            .set_confinement_reaction(self.current()?, kind, value)
    }
    pub fn confinement_reaction(
        &self,
    ) -> Result<Option<game_parallel_economy::ConfinementReaction>, String> {
        self.world.confinement_reaction(self.current()?)
    }
    pub fn set_teaching_reaction(
        &mut self,
        role: game_parallel_economy::TeachingRole,
        value: Option<game_parallel_economy::SubjectiveReaction>,
    ) -> Result<(), String> {
        self.world
            .set_teaching_reaction(self.current()?, role, value)
    }
    pub fn teaching_reaction(
        &self,
    ) -> Result<Option<game_parallel_economy::TeachingReaction>, String> {
        self.world.teaching_reaction(self.current()?)
    }
    pub fn set_practice_willingness(&mut self, enabled: bool) -> Result<(), String> {
        self.world
            .set_practice_willingness(self.current()?, enabled)
    }
    pub fn set_practice_asking(&mut self, enabled: bool) -> Result<(), String> {
        self.world.set_practice_asking(self.current()?, enabled)
    }
    pub fn set_practice_invention(&mut self, enabled: bool) -> Result<(), String> {
        self.world.set_practice_invention(self.current()?, enabled)
    }
    pub fn set_practice_approval(&mut self, enabled: bool) -> Result<(), String> {
        self.world.set_practice_approval(self.current()?, enabled)
    }
    pub fn combat_order(&mut self, order: Order) -> Result<(), String> {
        let actor = self.current()?;
        let encounter = self
            .view()?
            .battle
            .ok_or("person is not in combat")?
            .encounter;
        self.world.combat_order(actor, encounter, order)
    }
    pub fn combat_move(&mut self, destination: Point) -> Result<(), String> {
        let actor = self.current()?;
        let encounter = self
            .view()?
            .battle
            .ok_or("person is not in combat")?
            .encounter;
        self.world.combat_move(actor, encounter, destination)
    }
    pub fn start_expedition(&mut self, destination: u64, visit_ticks: u64) -> Result<u64, String> {
        self.world.start_mission(MissionStart {
            person: self.current()?,
            destination,
            visit_ticks,
        })
    }
    pub fn cancel_expedition(&mut self, id: u64) -> Result<(), String> {
        self.world.cancel_mission(self.current()?, id)
    }
    pub fn set_expedition_meeting(&mut self, id: u64, target: u64) -> Result<(), String> {
        self.world.set_mission_meeting(self.current()?, id, target)
    }
    pub fn set_expedition_delivery(
        &mut self,
        id: u64,
        recipient: u64,
        good: u32,
        quantity: u64,
    ) -> Result<(), String> {
        self.world
            .set_mission_delivery(self.current()?, id, recipient, good, quantity)
    }
    pub fn set_expedition_delivery_preference(
        &mut self,
        id: u64,
        preference: Option<game_parallel_economy::MissionDeliveryPreference>,
    ) -> Result<(), String> {
        self.world
            .set_mission_delivery_preference(self.current()?, id, preference)
    }
    pub fn set_expedition_delivery_choices(
        &mut self,
        id: u64,
        willing: bool,
        reserve: u64,
        pressure_threshold: Option<u64>,
    ) -> Result<(), String> {
        self.world.set_mission_delivery_choices(
            self.current()?,
            id,
            willing,
            reserve,
            pressure_threshold,
        )
    }
    pub fn set_expedition_delivery_acquisition(
        &mut self,
        id: u64,
        choice: Option<game_parallel_economy::MissionDeliveryAcquisition>,
    ) -> Result<(), String> {
        self.world
            .set_mission_delivery_acquisition(self.current()?, id, choice)
    }
    pub fn invite_expedition(&mut self, id: u64, target: u64) -> Result<Option<u64>, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.invite_mission(self.current()?, id, target)
    }
    pub fn promise(
        &mut self,
        target: u64,
        good: u32,
        quantity: u64,
        due: u64,
    ) -> Result<u64, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        let ids = self.world.propose_promises(&[PromiseProposal {
            debtor: self.current()?,
            creditor: target,
            good,
            quantity,
            due,
        }])?;
        Ok(ids[0])
    }
    pub fn decide_promise(&mut self, contract: u64, accept: bool) -> Result<(), String> {
        if !self.view()?.promises.iter().any(|c| c.id == contract) {
            return Err("unknown personal promise".into());
        }
        self.world.decide_promises(&[PromiseDecision {
            actor: self.current()?,
            contract,
            accept,
        }])
    }
    pub fn fulfill_promise(&mut self, contract: u64) -> Result<bool, String> {
        self.world.fulfill_promise(self.current()?, contract)
    }
    pub fn start_work(&mut self, recipe: u32, units: u32) -> Result<bool, String> {
        let actor = self.current()?;
        let events = self.world.start_work(actor, recipe, units)?;
        Ok(self
            .world
            .read()
            .physical
            .work()
            .iter()
            .any(|w| w.job.worker == actor && events.started.contains(&w.job.id)))
    }
    pub fn eat(&mut self, good: u32) -> Result<bool, String> {
        self.world.eat(self.current()?, good)
    }
    pub fn treat(&mut self) -> Result<bool, String> {
        self.world.treat(self.current()?)
    }
    pub fn feed_animal(&mut self, animal: u64, good: u32, quantity: u64) -> Result<bool, String> {
        if !self
            .view()?
            .animals
            .iter()
            .any(|row| row.id == animal && row.health > 0)
        {
            return Err("animal is not present".into());
        }
        self.world
            .feed_animal(self.current()?, animal, good, quantity)
    }
    pub fn hunt_animal(&mut self, animal: u64) -> Result<bool, String> {
        if !self.view()?.animals.iter().any(|a| a.id == animal) {
            return Err("animal is not locally visible".into());
        }
        self.world.hunt_animal(self.current()?, animal)
    }
    pub fn handle_animal(&mut self, animal: u64) -> Result<bool, String> {
        if !self
            .view()?
            .animals
            .iter()
            .any(|row| row.id == animal && row.health > 0)
        {
            return Err("animal is not present".into());
        }
        self.world.handle_animal(self.current()?, animal)
    }
    pub fn capture_animal(&mut self, animal: u64) -> Result<bool, String> {
        if !self
            .view()?
            .animals
            .iter()
            .any(|row| row.id == animal && row.health > 0)
        {
            return Err("animal is not present".into());
        }
        self.world.capture_animal(self.current()?, animal)
    }
    pub fn release_animal(&mut self, animal: u64) -> Result<bool, String> {
        if !self
            .view()?
            .animals
            .iter()
            .any(|row| row.id == animal && row.health > 0)
        {
            return Err("animal is not present".into());
        }
        self.world.release_animal(self.current()?, animal)
    }
    pub fn defend_animal(&mut self, animal: u64) -> Result<bool, String> {
        let current = self.current()?;
        if !self
            .view()?
            .animals
            .iter()
            .any(|row| row.id == animal && row.health > 0 && row.held_by == Some(current))
        {
            return Err("animal is not held by the current person".into());
        }
        self.world.defend_animal(current, animal)
    }
    pub fn rest(&mut self) -> Result<(), String> {
        self.world.rest(self.current()?)
    }
    pub fn farm(&mut self, field: u64) -> Result<bool, String> {
        if !self.view()?.fields.iter().any(|f| f.id == field) {
            return Err("field is not local".into());
        }
        self.world.farm_field(self.current()?, field)
    }
    /// Assert a personal claim over a local stock; this does not compel anyone.
    pub fn claim_stock(&mut self, good: u32) -> Result<(), String> {
        let person = self.current()?;
        let region = self
            .view()?
            .person
            .and_then(|p| p.region)
            .ok_or("person is traveling")?;
        self.world.claim_stock(person, region, good)
    }
    /// Assert a personal claim to a local physical site.
    pub fn claim_site(&mut self, site: u64) -> Result<(), String> {
        self.world.claim_site(self.current()?, site)
    }
    /// Withdraw this person's site assertion, even if the site no longer exists.
    pub fn withdraw_site_claim(&mut self, site: u64) -> Result<(), String> {
        self.world.withdraw_site_claim(self.current()?, site)
    }
    /// Choose whether to claim sites this person subsequently builds.
    pub fn set_built_site_claim_choice(&mut self, enabled: bool) -> Result<(), String> {
        self.world
            .set_built_site_claim_choice(self.current()?, enabled)
    }
    /// Withdraw one of the ruler's own assertions, including one made elsewhere.
    pub fn withdraw_claim(&mut self, region: u64, good: u32) -> Result<(), String> {
        self.world.withdraw_claim(self.current()?, region, good)
    }
    pub fn transfer_stock(
        &mut self,
        good: u32,
        quantity: u64,
        taking: bool,
    ) -> Result<bool, String> {
        self.world
            .transfer_stock(self.current()?, good, quantity, taking)
    }
    pub fn recover_possessions(
        &mut self,
        deceased: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        if !self
            .view()?
            .bodies
            .iter()
            .any(|body| body.person == deceased)
        {
            return Err("incapacitated person is not present".into());
        }
        self.world
            .recover_possessions(self.current()?, deceased, good, quantity)
    }
    pub fn take_possessions(
        &mut self,
        target: u64,
        good: u32,
        quantity: u64,
        escalate: bool,
    ) -> Result<game_parallel_economy::TakingOutcome, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world
            .take_possessions(self.current()?, target, good, quantity, escalate)
    }
    pub fn set_taking_response(&mut self, notices: bool, resist: bool) -> Result<(), String> {
        self.world
            .set_taking_response(self.current()?, notices, resist)
    }
    pub fn set_taking_witness(
        &mut self,
        actor: u64,
        rule: Option<game_parallel_economy::TakingWitness>,
    ) -> Result<(), String> {
        self.world.set_taking_witness(self.current()?, actor, rule)
    }
    pub fn taking_witness(
        &self,
        actor: u64,
    ) -> Result<Option<&game_parallel_economy::TakingWitness>, String> {
        Ok(self.world.taking_witness(self.current()?, actor))
    }
    pub fn set_taking_attention(&mut self, actor: u64, notices: bool) -> Result<(), String> {
        let person = self.current()?;
        let mut rule = self.world.taking_witness(person, actor).cloned().unwrap_or(
            game_parallel_economy::TakingWitness {
                person,
                actor,
                ..Default::default()
            },
        );
        rule.notices = notices;
        self.world.set_taking_witness(person, actor, Some(rule))
    }
    pub fn taking_reports(&self) -> Result<Vec<&game_parallel_economy::TakingReport>, String> {
        Ok(self.world.taking_reports(self.current()?))
    }
    pub fn set_taking_report_policy(
        &mut self,
        recipient: u64,
        actor: u64,
        policy: Option<game_parallel_economy::TakingReportPolicy>,
    ) -> Result<(), String> {
        self.world
            .set_taking_report_policy(self.current()?, recipient, actor, policy)
    }
    pub fn set_taking_report(
        &mut self,
        recipient: u64,
        actor: u64,
        claimed_outcome: Option<game_parallel_economy::TakingOutcome>,
    ) -> Result<(), String> {
        let person = self.current()?;
        self.world.set_taking_report_policy(
            person,
            recipient,
            actor,
            Some(game_parallel_economy::TakingReportPolicy {
                person,
                recipient,
                actor,
                willing: true,
                pressure_threshold: None,
                claimed_outcome,
                source: game_parallel_economy::TakingReportSource::Observed,
                heard_from: None,
                details: None,
            }),
        )
    }
    pub fn set_taking_relay(
        &mut self,
        recipient: u64,
        actor: u64,
        claimed_outcome: Option<game_parallel_economy::TakingOutcome>,
    ) -> Result<(), String> {
        let person = self.current()?;
        self.world.set_taking_report_policy(
            person,
            recipient,
            actor,
            Some(game_parallel_economy::TakingReportPolicy {
                person,
                recipient,
                actor,
                willing: true,
                pressure_threshold: None,
                claimed_outcome,
                source: game_parallel_economy::TakingReportSource::Heard,
                heard_from: None,
                details: None,
            }),
        )
    }
    pub fn set_taking_report_details(
        &mut self,
        recipient: u64,
        actor: u64,
        details: game_parallel_economy::TakingReportDetails,
    ) -> Result<(), String> {
        let person = self.current()?;
        let mut policy = self
            .world
            .taking_report_policy(person, recipient, actor)
            .cloned()
            .ok_or("configure a report or relay first")?;
        policy.details = Some(details);
        self.world
            .set_taking_report_policy(person, recipient, actor, Some(policy))
    }
    pub fn taking_report_reception(
        &self,
    ) -> Result<Option<&game_parallel_economy::TakingReception>, String> {
        Ok(self.world.taking_reception(self.current()?))
    }
    pub fn set_taking_report_comparison(
        &mut self,
        matching: bool,
        reaction: Option<game_parallel_economy::SubjectiveReaction>,
    ) -> Result<(), String> {
        let person = self.current()?;
        let mut reception = self.world.taking_reception(person).cloned().unwrap_or(
            game_parallel_economy::TakingReception {
                person,
                ..Default::default()
            },
        );
        if matching {
            reception.matching_toward_speaker = reaction;
        } else {
            reception.different_toward_speaker = reaction;
        }
        self.world.set_taking_reception(person, Some(reception))
    }
    pub fn set_taking_report_stance(
        &mut self,
        stance: game_parallel_economy::TakingReportStance,
    ) -> Result<(), String> {
        let person = self.current()?;
        let mut reception = self.world.taking_reception(person).cloned().unwrap_or(
            game_parallel_economy::TakingReception {
                person,
                ..Default::default()
            },
        );
        reception.stance = stance;
        self.world.set_taking_reception(person, Some(reception))
    }
    pub fn gather(&mut self, node: u64) -> Result<bool, String> {
        if !self
            .view()?
            .resource_sites
            .iter()
            .any(|site| site.id == node)
        {
            return Err("resource site is not present".into());
        }
        self.world.gather(self.current()?, node)
    }
    pub fn give(&mut self, recipient: u64, good: u32, quantity: u64) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&recipient) {
            return Err("person is not present".into());
        }
        self.world.give(self.current()?, recipient, good, quantity)
    }
    pub fn ask_food(&mut self, target: u64, lifetime: u64) -> Result<(), String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.ask_food(self.current()?, target, lifetime)
    }
    pub fn ask_haul(
        &mut self,
        carrier: u64,
        source: u64,
        destination: u64,
        good: u32,
        lifetime: u64,
    ) -> Result<(), String> {
        if !self.view()?.nearby.contains(&carrier) {
            return Err("person is not present".into());
        }
        self.world.ask_haul(
            self.current()?,
            carrier,
            source,
            destination,
            good,
            lifetime,
        )
    }
    pub fn treat_other(
        &mut self,
        patient: u64,
        medicine: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&patient) {
            return Err("person is not present".into());
        }
        self.world
            .treat_other(self.current()?, patient, medicine, quantity)
    }
    pub fn continue_work(&mut self) -> Result<bool, String> {
        let actor = self.current()?;
        let job = self.view()?.job.ok_or("person has no active work")?.id;
        let events = self.world.continue_work(actor)?;
        Ok(events.finished.contains(&job))
    }
    pub fn depart(&mut self, destination: u64) -> Result<(), String> {
        self.world.depart(&[Departure {
            person: self.current()?,
            destination,
        }])
    }
    /// Time can pass during a succession gap or an accepted loss of control.
    pub fn advance(&mut self) -> Result<(), String> {
        self.world.advance()?;
        Ok(())
    }
}
