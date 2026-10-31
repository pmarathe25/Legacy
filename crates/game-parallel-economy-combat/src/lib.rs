//! Tactical encounters over the economic world's authoritative people.
pub use game_parallel_conflict::{Command, Order};
use game_parallel_conflict::{Conflict, Encounter, Fighter};
use game_parallel_conflict_packed::{Plan, Round};
use game_parallel_economy_person::Person;
use game_parallel_lot_runtime::{Phase, Work};
use game_parallel_lots::{Place, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::PersonSnapshot;
use game_parallel_tactics::{Battle, Index, Placement, Tactics};
pub use game_parallel_tactics::{Map, Move, Point, Rules};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Defend,
    Fight,
    Retreat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    pub person: u64,
    pub response: Response,
    /// An autonomous fighter can decide to retreat as their health falls.
    #[serde(default)]
    pub retreat_at_or_below: Option<u32>,
    /// This individual may move toward their chosen opponent when out of reach.
    #[serde(default)]
    pub pursue: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub damage: u32,
    pub map_rules: Rules,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub protection: Vec<ProtectionConfig>,
    #[serde(default)]
    pub weapons: Vec<WeaponConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionConfig {
    pub good: String,
    pub reduction: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponConfig {
    pub good: String,
    pub bonus: u32,
}
impl Config {
    pub fn build(
        self,
        regions: &[u64],
        seed: u64,
        lookup: impl Fn(&str) -> Option<u32>,
    ) -> Result<Combat, String> {
        let weapons = self
            .weapons
            .into_iter()
            .map(|p| {
                Ok(Weapon {
                    good: lookup(&p.good)
                        .ok_or_else(|| format!("unknown weapon good {}", p.good))?,
                    bonus: p.bonus,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let protection = self
            .protection
            .into_iter()
            .map(|p| {
                Ok(Protection {
                    good: lookup(&p.good)
                        .ok_or_else(|| format!("unknown protective good {}", p.good))?,
                    reduction: p.reduction,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut combat = initial(regions, seed, self.damage, self.map_rules, self.reactions)?;
        combat.protection = protection;
        combat.weapons = weapons;
        Ok(combat)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protection {
    pub good: u32,
    pub reduction: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weapon {
    pub good: u32,
    pub bonus: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Combat {
    pub conflict: Conflict,
    pub tactics: Tactics,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub protection: Vec<Protection>,
    #[serde(default)]
    pub weapons: Vec<Weapon>,
}
pub struct Runtime {
    pub state: Combat,
    plan: Plan,
    index: Index,
    actors: Vec<u64>,
    goods: HashSet<u32>,
}
pub struct Draft {
    pub state: Combat,
    round: Round,
    replacement: Option<(Option<Plan>, Index, Option<Vec<u64>>)>,
    moved: bool,
}

/// Generate persistent maps once when creating a new authored world.
pub fn initial(
    regions: &[u64],
    seed: u64,
    damage: u32,
    rules: Rules,
    reactions: Vec<Reaction>,
) -> Result<Combat, String> {
    if damage == 0 {
        return Err("invalid combat damage".into());
    }
    let maps = regions
        .par_iter()
        .map(|&region| Map::generate(region, seed, rules.clone()).map_err(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Combat {
        conflict: Conflict {
            damage,
            next_id: 1,
            encounters: vec![],
            events: vec![],
        },
        tactics: Tactics {
            maps,
            battles: vec![],
        },
        reactions,
        protection: vec![],
        weapons: vec![],
    })
}

fn clone_state(state: &Combat) -> Combat {
    Combat {
        conflict: Conflict {
            damage: state.conflict.damage,
            next_id: state.conflict.next_id,
            encounters: state
                .conflict
                .encounters
                .par_iter()
                .map(|e| Encounter {
                    id: e.id,
                    region: e.region,
                    fighters: e.fighters.par_iter().cloned().collect(),
                })
                .collect(),
            events: vec![],
        },
        tactics: Tactics {
            maps: state.tactics.maps.par_iter().cloned().collect(),
            battles: state
                .tactics
                .battles
                .par_iter()
                .map(|b| Battle {
                    encounter: b.encounter,
                    region: b.region,
                    people: b.people.par_iter().cloned().collect(),
                })
                .collect(),
        },
        reactions: state.reactions.par_iter().cloned().collect(),
        protection: state.protection.par_iter().cloned().collect(),
        weapons: state.weapons.par_iter().cloned().collect(),
    }
}

fn regions(view: &View) -> HashSet<u64> {
    view.containers
        .par_iter()
        .filter_map(|c| match c.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect()
}
impl Runtime {
    pub fn compile(
        state: Combat,
        people: &[Person],
        view: &View,
        tick: u64,
        goods: &[game_parallel_production_model::Good],
    ) -> Result<Self, String> {
        let goods: HashSet<_> = goods.iter().map(|good| good.id).collect();
        Self::compile_known(state, people, view, tick, &goods)
    }
    fn compile_known(
        state: Combat,
        people: &[Person],
        view: &View,
        tick: u64,
        goods: &HashSet<u32>,
    ) -> Result<Self, String> {
        let ids: HashSet<_> = people.par_iter().map(|p| p.id).collect();
        let regions = regions(view);
        let mut protective: Vec<_> = state.protection.par_iter().map(|p| p.good).collect();
        protective.par_sort_unstable();
        let mut weapons: Vec<_> = state.weapons.par_iter().map(|p| p.good).collect();
        weapons.par_sort_unstable();
        if protective.par_windows(2).any(|p| p[0] == p[1])
            || state
                .protection
                .par_iter()
                .any(|p| p.reduction == 0 || !goods.contains(&p.good))
            || weapons.par_windows(2).any(|p| p[0] == p[1])
            || state
                .weapons
                .par_iter()
                .any(|p| p.bonus == 0 || !goods.contains(&p.good))
        {
            return Err("invalid protective goods".into());
        }
        state.conflict.validate(&ids, &regions, tick)?;
        state
            .tactics
            .validate_world(&state.conflict, &regions, tick)?;
        let mut reactions: Vec<_> = state.reactions.par_iter().map(|r| r.person).collect();
        reactions.par_sort_unstable();
        if reactions.par_windows(2).any(|p| p[0] == p[1])
            || reactions.par_iter().any(|p| !ids.contains(p))
        {
            return Err("invalid personal combat reaction".into());
        }
        if state.conflict.encounters.par_iter().any(|fight| {
            fight.fighters.par_iter().any(|f| {
                people
                    .binary_search_by_key(&f.person, |p| p.id)
                    .ok()
                    .is_none_or(|i| people[i].health == 0 || people[i].region != Some(fight.region))
            })
        }) {
            return Err("combatant must be alive and local".into());
        }
        let plan = Plan::compile(&state.conflict).map_err(str::to_owned)?;
        let index = state.tactics.index().map_err(str::to_owned)?;
        let actors = state.conflict.actors();
        Ok(Self {
            state,
            plan,
            index,
            actors,
            goods: goods.clone(),
        })
    }
    pub fn actors(&self) -> &[u64] {
        &self.actors
    }
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        personal: &[Person],
        tick: u64,
        seed: u64,
        view: &View,
    ) -> Draft {
        let mut retreating: Vec<_> = self
            .state
            .reactions
            .par_iter()
            .filter_map(|reaction| {
                let threshold = reaction.retreat_at_or_below?;
                let i = personal
                    .binary_search_by_key(&reaction.person, |p| p.id)
                    .ok()?;
                let person = &personal[i];
                (!person.controlled
                    && person.health > 0
                    && person.health <= threshold
                    && self.actors.binary_search(&person.id).is_ok())
                .then_some(person.id)
            })
            .collect();
        retreating.par_sort_unstable();
        let mut pursuers: Vec<_> = self
            .state
            .reactions
            .par_iter()
            .filter(|r| {
                r.pursue
                    && matches!(r.response, Response::Fight)
                    && retreating.binary_search(&r.person).is_err()
                    && self.actors.binary_search(&r.person).is_ok()
            })
            .map(|r| r.person)
            .collect();
        pursuers.par_sort_unstable();
        let moves = game_parallel_tactical_choices::prepare(
            &self.state.conflict,
            &self.state.tactics,
            personal,
            &pursuers,
            seed,
            tick.saturating_sub(1),
        );
        let mut round = self.plan.prepare_with_filter(
            tick,
            |id, region| {
                people
                    .binary_search_by_key(&id, |p| p.0)
                    .ok()
                    .is_some_and(|i| people[i].1 == Some(region) && people[i].2 > 0)
            },
            |a, b| retreating.binary_search(&a).is_err() && self.index.can_attack_between(a, b),
        );
        if !self.state.protection.is_empty() {
            let protection: std::collections::HashMap<_, _> = self
                .state
                .protection
                .iter()
                .map(|p| (p.good, p.reduction))
                .collect();
            round.protect(|person| {
                view.container(Place::Person(person)).map_or(0, |holder| {
                    holder
                        .goods
                        .iter()
                        .filter(|item| item.quantity > 0)
                        .filter_map(|item| protection.get(&item.good).copied())
                        .max()
                        .unwrap_or(0)
                })
            });
        }
        if !self.state.weapons.is_empty() {
            let weapons: std::collections::HashMap<_, _> = self
                .state
                .weapons
                .iter()
                .map(|w| (w.good, w.bonus))
                .collect();
            let armor: std::collections::HashMap<_, _> = self
                .state
                .protection
                .iter()
                .map(|p| (p.good, p.reduction))
                .collect();
            let defense: HashSet<_> = self
                .state
                .conflict
                .encounters
                .par_iter()
                .flat_map_iter(|fight| fight.fighters.iter())
                .filter(|fighter| matches!(fighter.order, Order::Defend))
                .map(|fighter| fighter.person)
                .collect();
            let protection: std::collections::HashMap<_, _> = round
                .topology
                .targets
                .par_iter()
                .map(|target| {
                    let reduction =
                        view.container(Place::Person(target.person))
                            .map_or(0, |holder| {
                                holder
                                    .goods
                                    .iter()
                                    .filter(|item| item.quantity > 0)
                                    .filter_map(|item| armor.get(&item.good).copied())
                                    .max()
                                    .unwrap_or(0)
                            });
                    (target.person, reduction)
                })
                .collect();
            round.vary_damage(|attacker, target| {
                let bonus = view.container(Place::Person(attacker)).map_or(0, |holder| {
                    holder
                        .goods
                        .iter()
                        .filter(|item| item.quantity > 0)
                        .filter_map(|item| weapons.get(&item.good).copied())
                        .max()
                        .unwrap_or(0)
                });
                let damage = self.state.conflict.damage.saturating_add(bonus);
                let damage = if defense.contains(&target) {
                    (damage / 2).max(1)
                } else {
                    damage
                };
                damage.saturating_sub(protection.get(&target).copied().unwrap_or(0))
            });
        }
        let mut state = clone_state(&self.state);
        game_parallel_tactical_choices::apply(&mut state.tactics, &moves, tick.saturating_sub(1));
        if !retreating.is_empty() {
            state.conflict.encounters.par_iter_mut().for_each(|fight| {
                fight
                    .fighters
                    .par_iter_mut()
                    .filter(|fighter| retreating.binary_search(&fighter.person).is_ok())
                    .for_each(|fighter| fighter.order = Order::Retreat)
            });
        }
        state.conflict.events = round.expand();
        Draft {
            state,
            round,
            replacement: None,
            moved: !moves.is_empty(),
        }
    }
    pub fn publish(&mut self, draft: Draft) {
        self.state = draft.state;
        if let Some((plan, index, actors)) = draft.replacement {
            if let Some(plan) = plan {
                self.plan = plan;
            }
            self.index = index;
            if let Some(actors) = actors {
                self.actors = actors;
            }
        }
    }
    /// Starting an attack does not require the target's agreement.
    pub fn begin(
        &self,
        people: &[Person],
        view: &View,
        tick: u64,
        actor: u64,
        target: u64,
    ) -> Result<(Self, u64), String> {
        if actor == target {
            return Err("attack requires another person".into());
        }
        let find = |id| {
            people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &people[i])
        };
        let attacker = find(actor).ok_or("unknown attacker")?;
        let victim = find(target).ok_or("unknown target")?;
        let region = attacker.region.ok_or("attacker is traveling")?;
        if attacker.health == 0 || victim.health == 0 || victim.region != Some(region) {
            return Err("fighters must be living and local".into());
        }
        let map = self
            .state
            .tactics
            .maps
            .iter()
            .find(|m| m.region == region)
            .ok_or("no tactical map for region")?;
        let mut positions = Vec::new();
        for x in 0..=map.rules.width.min(63) {
            for y in 0..=map.rules.height.min(63) {
                let point = Point { x, y };
                if map.open(point) {
                    positions.push(point);
                }
                if positions.len() == 2 {
                    break;
                }
            }
            if positions.len() == 2 {
                break;
            }
        }
        if positions.len() < 2 {
            return Err("no open tactical starting positions".into());
        }
        let response = self
            .state
            .reactions
            .iter()
            .find(|r| r.person == target)
            .map_or(&Response::Defend, |r| &r.response);
        let target_order = match response {
            Response::Defend => Order::Defend,
            Response::Fight => Order::Attack { target: actor },
            Response::Retreat => Order::Retreat,
        };
        let mut state = self.state.clone();
        let id = state.conflict.next_id;
        let snapshots: Vec<_> = people
            .par_iter()
            .map(|p| (p.id, p.region, p.health, p.hunger))
            .collect();
        state.conflict.begin(
            &snapshots,
            &[],
            Encounter {
                id,
                region,
                fighters: vec![
                    Fighter {
                        person: actor,
                        side: 1,
                        order: Order::Attack { target },
                    },
                    Fighter {
                        person: target,
                        side: 2,
                        order: target_order,
                    },
                ],
            },
        )?;
        state.tactics.battles.push(Battle {
            encounter: id,
            region,
            people: vec![
                Placement {
                    person: actor,
                    position: positions[0],
                    last_move: None,
                },
                Placement {
                    person: target,
                    position: positions[1],
                    last_move: None,
                },
            ],
        });
        Ok((
            Self::compile_known(state, people, view, tick, &self.goods)?,
            id,
        ))
    }
    pub fn order(
        &self,
        people: &[Person],
        view: &View,
        tick: u64,
        command: game_parallel_conflict::Command,
    ) -> Result<Self, String> {
        let mut state = self.state.clone();
        state.conflict.order(&[command])?;
        Self::compile_known(state, people, view, tick, &self.goods)
    }
    pub fn move_person(
        &self,
        people: &[Person],
        view: &View,
        tick: u64,
        command: game_parallel_tactics::Move,
    ) -> Result<Self, String> {
        let mut state = self.state.clone();
        state.tactics = state
            .tactics
            .move_batch(&[command], tick)
            .map_err(str::to_owned)?;
        Self::compile_known(state, people, view, tick, &self.goods)
    }
}
impl Draft {
    /// Distinct pairs that actually exchanged strikes in this round.
    pub fn contacts(&self) -> Vec<(u64, u64)> {
        let mut pairs: Vec<_> = self
            .state
            .conflict
            .events
            .par_iter()
            .map(|event| {
                (
                    event.attacker.min(event.target),
                    event.attacker.max(event.target),
                )
            })
            .collect();
        if pairs.par_windows(2).any(|pair| pair[0] > pair[1]) {
            pairs.par_sort_unstable();
        }
        pairs
            .par_chunk_by(|a, b| a == b)
            .map(|group| group[0])
            .collect()
    }
    pub fn apply(&self, people: &mut [Person]) {
        let damage = self.round.damage();
        people.par_iter_mut().for_each(|p| {
            if let Ok(i) = damage.binary_search_by_key(&p.id, |d| d.target) {
                p.health = p.health.saturating_sub(damage[i].amount);
            }
        });
    }
    pub fn learn_harm(&self, opinions: &mut Option<Opinions>) {
        if let Some(opinions) = opinions.as_mut().filter(|o| o.tracks_harm()) {
            let harm: Vec<_> = self
                .state
                .conflict
                .events
                .par_iter()
                .map(|e| (e.target, e.attacker, e.damage))
                .collect();
            opinions.learn_harm(&harm);
        }
    }
    pub fn finish(&mut self, people: &[Person]) -> Result<(), String> {
        let changed = self.state.conflict.encounters.par_iter().any(|fight| {
            fight.fighters.par_iter().any(|fighter| {
                matches!(fighter.order, Order::Retreat)
                    || people
                        .binary_search_by_key(&fighter.person, |p| p.id)
                        .ok()
                        .is_none_or(|i| people[i].health == 0)
            })
        });
        if changed {
            game_parallel_conflict::finish(&mut self.state.conflict, |id| {
                people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0)
            });
            self.state.tactics.sync(&self.state.conflict);
            self.replacement = Some((
                Some(Plan::compile(&self.state.conflict).map_err(str::to_owned)?),
                self.state.tactics.index().map_err(str::to_owned)?,
                Some(self.state.conflict.actors()),
            ));
        } else if self.moved {
            self.replacement = Some((
                None,
                self.state.tactics.index().map_err(str::to_owned)?,
                None,
            ));
        }
        Ok(())
    }
}
/// Interrupt existing work when a person is drawn into a fight.
pub fn interrupt(
    actors: &[u64],
    work: &[Work],
    rates: &mut Vec<(u64, u64)>,
    abandon: &mut Vec<(u64, u64)>,
) {
    if actors.is_empty() {
        return;
    }
    rates.retain(|(id, _)| {
        work.binary_search_by_key(id, |w| w.job.id)
            .ok()
            .is_none_or(|i| actors.binary_search(&work[i].job.worker).is_err())
    });
    let mut existing: Vec<_> = abandon.iter().map(|r| r.0).collect();
    existing.par_sort_unstable();
    abandon.par_extend(
        work.par_iter()
            .filter(|w| {
                w.phase == Phase::Working
                    && actors.binary_search(&w.job.worker).is_ok()
                    && existing.binary_search(&w.job.id).is_err()
            })
            .map(|w| (w.job.id, w.job.worker)),
    );
}

#[macro_export]
macro_rules! compile_world {
    ($state:expr, $physical:expr, $view:expr) => {
        $state
            .combat
            .take()
            .map(|combat| {
                $crate::Runtime::compile(
                    combat,
                    &$state.people,
                    &$view,
                    $physical.tick(),
                    $physical.goods(),
                )
            })
            .transpose()?
    };
}
#[macro_export]
macro_rules! finish_world {
    ($combat:expr, $person:expr) => {
        if let Some(combat) = &mut $combat {
            combat.finish(&$person.people)?;
        }
    };
}
#[macro_export]
macro_rules! learn_world {
    ($combat:expr, $opinions:expr) => {
        if let Some(combat) = &$combat {
            combat.learn_harm(&mut $opinions);
        }
    };
}
#[macro_export]
macro_rules! publish_world {
    ($world:expr, $draft:expr) => {
        if let (Some(combat), Some(next)) = (&mut $world.combat, $draft) {
            combat.publish(next);
        }
    };
}
#[macro_export]
macro_rules! validate_manual {
    ($combat:expr, $request:expr, $work:expr, $meal:expr, $physical:expr) => {
        if let Some(combat) = &$combat {
            let actor = $request
                .map(|r| r.actor)
                .or_else(|| $work.map(|w| w.actor()))
                .or_else(|| $meal.map(|m| m.person))
                .or_else(|| $physical.map(|p| p.person()));
            if actor.is_some_and(|id| combat.actors().binary_search(&id).is_ok()) {
                return Err("combatants cannot perform this action during a fight".into());
            }
        }
    };
}
