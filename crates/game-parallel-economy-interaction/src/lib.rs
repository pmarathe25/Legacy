//! One explicitly chosen meeting, with a personal response from the other participant.
use game_parallel_economy_person::{
    Person,
    travel::{Colocation, Travel},
};
use game_parallel_lot_runtime::{Phase, Work};
use game_parallel_opinions::Opinions;
use game_parallel_social::{Encounter, Social};
mod manual;
pub use manual::{PhysicalChoice, Transfer, recover, transfer};

#[derive(Clone, Copy)]
pub struct Request {
    pub actor: u64,
    pub target: u64,
}
#[derive(Clone, Copy)]
pub enum WorkAction {
    Start { actor: u64, recipe: u32, units: u32 },
    Continue { actor: u64 },
}
impl WorkAction {
    pub fn actor(self) -> u64 {
        match self {
            Self::Start { actor, .. } | Self::Continue { actor } => actor,
        }
    }
}
pub struct Plan {
    pub actors: [u64; 2],
    pub encounter: Option<Encounter>,
}
pub fn prepare(
    request: Option<Request>,
    social: Option<&Social>,
    opinions: Option<&Opinions>,
    people: &[Person],
    busy: &[u64],
    movement: (&[Work], Option<&Travel>),
    tick: u64,
) -> Result<Option<Plan>, String> {
    let (work, travel) = movement;
    let Some(request) = request else {
        return Ok(None);
    };
    if request.actor == request.target {
        return Err("a meeting requires another person".into());
    }
    let social = social.ok_or("social interaction is disabled")?;
    let find = |id| {
        people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &people[i])
    };
    let actor = find(request.actor).ok_or("unknown person")?;
    let target = find(request.target).ok_or("unknown target")?;
    let region = Colocation::new(travel)
        .region((actor.id, actor.region), (target.id, target.region))
        .ok_or("participants must be nearby")?;
    if actor.health == 0
        || target.health == 0
        || target.controlled
        || busy.binary_search(&target.id).is_ok()
        || (!actor.controlled && busy.binary_search(&actor.id).is_ok())
        || work
            .iter()
            .any(|w| w.job.worker == actor.id && w.phase == Phase::Working)
    {
        return Err("participants must be living, local and available".into());
    }
    if !social.participants.iter().any(|p| p.person == actor.id)
        || !social.participants.iter().any(|p| p.person == target.id)
    {
        return Err("both people need social preferences".into());
    }
    let eligible =
        target.health >= social.rules.min_health && target.hunger <= social.rules.max_hunger;
    let willing = eligible
        && social
            .participants
            .iter()
            .any(|p| p.person == target.id && p.willing);
    let pressured = eligible
        && social
            .pressures
            .iter()
            .find(|p| p.person == target.id)
            .is_some_and(|policy| {
                opinions.is_some_and(|opinions| {
                    let (fear, obligation) = opinions.index().feelings(target.id, actor.id);
                    policy.responds(fear, obligation)
                })
            });
    Ok(Some(Plan {
        actors: [actor.id, target.id],
        encounter: (willing || pressured).then_some(Encounter {
            tick,
            region,
            a: actor.id.min(target.id),
            b: actor.id.max(target.id),
            pressured: (!willing).then_some(target.id),
        }),
    }))
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr, $request:expr, $busy:expr, $tick:expr) => {{
        let plan = $crate::prepare(
            $request,
            $world.dynamics.as_ref().and_then(|d| d.social.as_ref()),
            $world.opinions.as_ref(),
            &$world.people,
            &$busy,
            ($world.physical.work(), $world.travel.as_ref()),
            $tick,
        )?;
        if let Some(plan) = &plan {
            game_parallel_economy_work::reserve(&mut $busy, plan.actors.iter().copied());
        }
        plan
    }};
}
#[macro_export]
macro_rules! interaction_methods {
    () => {
        /// Attempt a real meeting and advance one boundary, even if the target declines.
        pub fn meet(&mut self, actor: u64, target: u64) -> Result<bool, String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(actor)
                    && c.lineage.controlled_person(self.tick()) != Some(actor)
            }) {
                return Err("player control is suspended".into());
            }
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                Some($crate::Request { actor, target }),
                None,
                None,
                None,
            )?;
            let accepted = draft
                .dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .is_some_and(|social| {
                    social
                        .events
                        .iter()
                        .any(|e| e.a == actor.min(target) && e.b == actor.max(target))
                });
            self.commit(draft)?;
            Ok(accepted)
        }
    };
}

#[macro_export]
macro_rules! validate_work_world {
    ($world:expr, $action:expr) => {
        if let Some(action) = $action {
            let actor = action.actor();
            let person = $world
                .people
                .binary_search_by_key(&actor, |p| p.id)
                .ok()
                .map(|i| &$world.people[i])
                .ok_or("unknown person")?;
            if !person.controlled || person.health == 0 || person.region.is_none() {
                return Err("work requires a living, local controlled person".into());
            }
            if person.hunger > $world.rules.max_work_hunger {
                return Err("person is too hungry to work".into());
            }
            if $world.family.as_ref().is_some_and(|family| {
                family
                    .members
                    .iter()
                    .any(|m| m.id == actor && m.age < family.rules.min_work_age)
            }) {
                return Err("person is too young to work".into());
            }
            let working = $world.physical.work().iter().any(|w| {
                w.job.worker == actor && w.phase == game_parallel_lot_runtime::Phase::Working
            });
            match action {
                $crate::WorkAction::Start { recipe, units, .. } => {
                    if working
                        || units == 0
                        || !$world.physical.recipes().iter().any(|r| r.id == recipe)
                    {
                        return Err("invalid work start".into());
                    }
                    if $world.autowork.is_none() {
                        return Err("work planning is disabled".into());
                    }
                }
                $crate::WorkAction::Continue { .. } if !working => {
                    return Err("person has no active work".into());
                }
                _ => {}
            }
        }
    };
}

#[macro_export]
macro_rules! work_starts {
    ($world:expr, $action:expr, $work:expr, $starts:expr, $claims:expr, $uses:expr) => {{
        if let Some($crate::WorkAction::Start {
            actor,
            recipe,
            units,
        }) = $action
        {
            let next = $work
                .as_ref()
                .and_then(|p| p.work.as_ref())
                .map(|p| p.next_id)
                .unwrap_or(
                    $world
                        .autowork
                        .as_ref()
                        .expect("validated work planner")
                        .next_id,
                );
            let after_live = $world
                .physical
                .work()
                .last()
                .map_or(Some(1), |w| w.job.id.checked_add(1))
                .ok_or("job id overflow")?;
            let id = next.max(after_live).max(1);
            let following = id.checked_add(1).ok_or("job id overflow")?;
            let person = &$world.people[$world
                .people
                .binary_search_by_key(&actor, |p| p.id)
                .expect("validated actor")];
            let start = game_parallel_production_model::Start {
                id,
                worker: actor,
                region: person.region.expect("validated region"),
                recipe,
                units,
            };
            if $claims.is_some() {
                $uses.extend(game_parallel_economy_claims::work(
                    &[start],
                    $world.physical.recipes(),
                )?);
            }
            let mut combined = $starts.to_vec();
            combined.push(start);
            (std::borrow::Cow::Owned(combined), Some(following))
        } else {
            (std::borrow::Cow::Borrowed($starts), None)
        }
    }};
}

#[macro_export]
macro_rules! work_rate {
    ($world:expr, $action:expr, $rates:expr) => {
        if let Some($crate::WorkAction::Continue { actor }) = $action {
            let work = $world
                .physical
                .work()
                .iter()
                .find(|w| {
                    w.job.worker == actor && w.phase == game_parallel_lot_runtime::Phase::Working
                })
                .expect("validated work");
            let mut rate = [(work.job.id, 1)];
            game_parallel_economy_skills::rates(
                $world.skills.as_ref(),
                $world.campaign.as_ref(),
                $world.physical.work(),
                &mut rate,
            );
            $rates.push(rate[0]);
        }
    };
}

#[macro_export]
macro_rules! publish_work {
    ($autowork:expr, $manual_job:expr) => {
        if let (Some(auto), Some(next)) = (&mut $autowork, $manual_job) {
            auto.next_id = auto.next_id.max(next);
        }
    };
}

#[macro_export]
macro_rules! work_methods {
    () => {
        /// Attempt production with actual inputs; denial still advances the world.
        pub fn start_work(
            &mut self,
            actor: u64,
            recipe: u32,
            units: u32,
        ) -> Result<game_parallel_lot_runtime::Events, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                Some($crate::WorkAction::Start {
                    actor,
                    recipe,
                    units,
                }),
                None,
                None,
            )?;
            self.commit(draft)
        }
        /// Spend one boundary working on the controlled person's active job.
        pub fn continue_work(
            &mut self,
            actor: u64,
        ) -> Result<game_parallel_lot_runtime::Events, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                Some($crate::WorkAction::Continue { actor }),
                None,
                None,
            )?;
            self.commit(draft)
        }
    };
}

#[macro_export]
macro_rules! combat_methods {
    () => {
        /// Start an attack; the other person responds according to their own reaction.
        pub fn attack(&mut self, actor: u64, target: u64) -> Result<u64, String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(actor)
                    && c.lineage.controlled_person(self.tick()) != Some(actor)
            }) {
                return Err("player control is suspended".into());
            }
            let view = self.physical.view(self.tick())?;
            let (next, id) = self.combat.as_ref().ok_or("combat is disabled")?.begin(
                &self.people,
                &view,
                self.tick(),
                actor,
                target,
            )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.combat = Some(next);
            self.epoch = epoch;
            Ok(id)
        }
        pub fn combat_order(
            &mut self,
            actor: u64,
            encounter: u64,
            order: game_parallel_economy_combat::Order,
        ) -> Result<(), String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(actor)
                    && c.lineage.controlled_person(self.tick()) != Some(actor)
            }) {
                return Err("player control is suspended".into());
            }
            let view = self.physical.view(self.tick())?;
            let next = self.combat.as_ref().ok_or("combat is disabled")?.order(
                &self.people,
                &view,
                self.tick(),
                game_parallel_economy_combat::Command {
                    encounter,
                    person: actor,
                    order,
                },
            )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.combat = Some(next);
            self.epoch = epoch;
            Ok(())
        }
        pub fn combat_move(
            &mut self,
            actor: u64,
            encounter: u64,
            destination: game_parallel_economy_combat::Point,
        ) -> Result<(), String> {
            if self.campaign.as_ref().is_some_and(|c| {
                c.lineage.current == Some(actor)
                    && c.lineage.controlled_person(self.tick()) != Some(actor)
            }) {
                return Err("player control is suspended".into());
            }
            let view = self.physical.view(self.tick())?;
            let next = self
                .combat
                .as_ref()
                .ok_or("combat is disabled")?
                .move_person(
                    &self.people,
                    &view,
                    self.tick(),
                    game_parallel_economy_combat::Move {
                        encounter,
                        person: actor,
                        destination,
                    },
                )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.combat = Some(next);
            self.epoch = epoch;
            Ok(())
        }
    };
}
