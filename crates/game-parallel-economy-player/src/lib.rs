//! Ruler-bound choices and observations, independent of administrative inspection.
use game_parallel_economy::{
    Departure, Economy, MissionStart, Person, PromiseDecision, PromiseProposal,
};
use game_parallel_economy_campaign::{Guardian, InvocationWarning, Life, ReleaseDecision};
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
pub struct WorkSiteView {
    pub site: game_parallel_economy::WorkSite,
    pub occupied: u64,
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
pub struct InfectionView {
    pub strain: u32,
    pub name: String,
    pub medicine: Option<(u32, u64)>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub tick: u64,
    pub person: Option<Person>,
    pub journey: Option<game_parallel_economy::Journey>,
    pub nearby: Vec<u64>,
    pub bodies: Vec<BodyView>,
    pub possessions: Vec<Lot>,
    pub local_stock: Vec<Lot>,
    pub food_reports: Vec<Record>,
    pub job: Option<Job>,
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
    pub relationships: Vec<game_parallel_economy::Opinion>,
    pub hauling_requests: Vec<game_parallel_economy::HaulingRequest>,
    pub observed_work: Vec<game_parallel_autowork::requests::WorkLink>,
    pub work_memories: Vec<game_parallel_autowork::requests::Memory>,
    pub infection: Option<InfectionView>,
    pub fields: Vec<game_parallel_economy::Field>,
    pub resource_sites: Vec<game_parallel_economy::ResourceNode>,
    pub work_sites: Vec<WorkSiteView>,
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
                    .filter(|p| p.health == 0 && p.region == Some(region))
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
        let infection = current.and_then(|id| {
            let dynamics = state.dynamics?;
            let disease = dynamics.disease.as_ref()?;
            let case = disease.cases.par_iter().find_any(|c| c.person == id)?;
            let strain = disease.strains.iter().find(|s| s.id == case.strain)?;
            let medicine = dynamics
                .treatment
                .iter()
                .find(|p| {
                    p.person == id
                        && disease
                            .treatments
                            .iter()
                            .any(|t| t.strain == case.strain && t.medicine == p.medicine)
                })
                .map(|p| (p.medicine, p.quantity));
            Some(InfectionView {
                strain: strain.id,
                name: strain.name.clone(),
                medicine,
            })
        });
        let pending_work_requests = current
            .and_then(|id| {
                requests.map(|requests| {
                    requests
                        .pending
                        .par_iter()
                        .filter(|r| r.requester == id && nearby.binary_search(&r.worker).is_ok())
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
            person,
            journey,
            nearby,
            bodies,
            possessions,
            local_stock,
            food_reports,
            job,
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
            sharing_learning: current
                .and_then(|id| self.world.sharing_learning(id))
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
            infection,
            fields,
            resource_sites,
            work_sites,
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
    /// Explicit guardian-search spectating does not disclose NPC knowledge.
    pub fn guardians(&self) -> Result<Vec<Guardian>, String> {
        let state = self.world.read();
        Ok(state
            .campaign
            .ok_or("campaign is disabled")?
            .guardians
            .as_ref()
            .ok_or("guardians are disabled")?
            .guardians
            .clone())
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
    pub fn meet(&mut self, target: u64) -> Result<bool, String> {
        if !self.view()?.nearby.contains(&target) {
            return Err("person is not present".into());
        }
        self.world.meet(self.current()?, target)
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
    pub fn rest(&mut self) -> Result<(), String> {
        self.world.rest(self.current()?)
    }
    pub fn farm(&mut self, field: u64) -> Result<bool, String> {
        if !self.view()?.fields.iter().any(|f| f.id == field) {
            return Err("field is not local".into());
        }
        self.world.farm_field(self.current()?, field)
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
            return Err("deceased person is not present".into());
        }
        self.world
            .recover_possessions(self.current()?, deceased, good, quantity)
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
