//! Encounter lifecycle and initiative actions over canonical Conflict rows.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Wait,
    Defend,
    Surrender,
    Attack(PersonId),
    Capture(PersonId),
    Release(PersonId),
    Retreat(AreaId),
    Breach(u32),
    MissionOrder { leader: PersonId },
}
#[derive(Clone, Debug)]
pub struct TurnFacts {
    pub actor: PersonId,
    pub independent: bool,
    pub body: TargetBody,
    pub force_can_act: bool,
    pub force_allowed: bool,
    pub target_body: Option<TargetBody>,
    pub binding_available: bool,
    pub release_available: bool,
    pub mission_order_valid: bool,
    pub retreat_route_valid: bool,
    pub damage: u64,
    pub practice_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Moved {
        actor: PersonId,
        position: Point,
    },
    Struck {
        actor: PersonId,
        target: PersonId,
        damage: u64,
        practice: game_skills::PracticeDelta,
    },
    Surrendered {
        actor: PersonId,
    },
    Capture {
        target: PersonId,
        context: ConfinementContext,
    },
    Release {
        target: PersonId,
    },
    MissionOrder,
    ObstacleDamaged {
        map: CombatMapId,
        obstacle: u32,
        damage: u64,
    },
}
pub struct PreparedCombat {
    before: Encounter,
    after: Encounter,
    map_change: Option<(Map, Map)>,
    effects: Vec<Effect>,
    rules: Rules,
    new_members: Vec<PersonId>,
    reads: ReadSet,
}
pub struct StagedCombat {
    reads: ReadGuard,
    rules: Rules,
    effects: Vec<Effect>,
}
impl PreparedCombat {
    pub fn with_factual_reads(mut self, reads: ReadSet) -> Self {
        self.reads.extend(reads);
        self
    }
    pub fn with_search(
        mut self,
        store: &Store,
        actor: PersonId,
        search: SearchState,
    ) -> Result<Self, Error> {
        if self.before.order.get(self.before.cursor) != Some(&actor)
            || self.after.cursor != self.before.cursor.checked_add(1).ok_or(Error::Overflow)?
            || !search.valid(map(store, self.after.map).ok_or(Error::Unavailable)?)
        {
            return Err(Error::Unavailable);
        }
        self.after
            .fighters
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .search = search;
        Ok(self)
    }
    pub fn projected_encounter(&self) -> &Encounter {
        &self.after
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub fn stage_into(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedCombat, Error> {
        self.stage_with_receivers(world, store, batch, None, None)
    }
    pub fn stage_with_receivers(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        injury: Option<&game_physiology::InjuryReceipt>,
        confinement: Option<&ConfinementGuard>,
    ) -> Result<StagedCombat, Error> {
        self.stage_with_practice(world, store, batch, injury, confinement, None)
    }
    pub fn stage_with_practice(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        injury: Option<&game_physiology::InjuryReceipt>,
        confinement: Option<&ConfinementGuard>,
        practice: Option<&game_skills::StagedPractice>,
    ) -> Result<StagedCombat, Error> {
        self.stage_with_release(
            world,
            store,
            batch,
            injury,
            confinement,
            practice,
            None,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stage_with_release(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        injury: Option<&game_physiology::InjuryReceipt>,
        confinement: Option<&ConfinementGuard>,
        practice: Option<&game_skills::StagedPractice>,
        release: Option<&crate::force::Receipt>,
        release_motion: Option<&game_transport::MotionFinalization>,
    ) -> Result<StagedCombat, Error> {
        self.stage_with_death(
            world,
            store,
            batch,
            injury,
            confinement,
            practice,
            release,
            release_motion,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stage_with_death(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        injury: Option<&game_physiology::InjuryReceipt>,
        confinement: Option<&ConfinementGuard>,
        practice: Option<&game_skills::StagedPractice>,
        release: Option<&crate::force::Receipt>,
        release_motion: Option<&game_transport::MotionFinalization>,
        death_motion: Option<&game_transport::MotionFinalization>,
    ) -> Result<StagedCombat, Error> {
        if world.rules != self.rules || encounter(store, self.before.id) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        let mut reads = self.reads;
        let mut admitted = ReadFootprint::new();
        if let Some(motion) = death_motion {
            motion.validate_footprint(store, batch)?;
            let injury = injury.ok_or(Error::Unavailable)?;
            if !injury.lethal() {
                return Err(Error::Unavailable);
            }
            let target = self
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Struck { target, damage, .. }
                        if injury.matches(*target, *damage) && motion.deaths_actor(*target) =>
                    {
                        Some(*target)
                    }
                    _ => None,
                })
                .ok_or(Error::Unavailable)?;
            for id in motion.location_ids() {
                admitted.protect::<Location>(id);
            }
            admitted.protect::<game_physiology::Body>(StableId(target.0));
        }
        if let Some(release) = release {
            release.validate_batch(store, batch)?;
            let target = self
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Release { target } => Some(*target),
                    _ => None,
                })
                .ok_or(Error::Unavailable)?;
            if release.released_targets().collect::<Vec<_>>() != [target]
                || batch
                    .projected::<Captivity>(store, StableId(target.0))?
                    .is_some()
            {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Captivity>(StableId(target.0));
            if let Some(motion) = release_motion {
                motion.validate_footprint(store, batch)?;
                if !motion.releases_confinement(target) {
                    return Err(Error::Unavailable);
                }
                for id in motion.location_ids() {
                    admitted.protect::<Location>(id);
                }
            } else if game_transport::group(store, target).is_some() {
                return Err(Error::Unavailable);
            }
        } else if release_motion.is_some() {
            return Err(Error::Unavailable);
        }
        if let Some(practice) = practice {
            practice.validate_batch(store, batch)?;
            let actor = self
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Struck {
                        actor,
                        practice: delta,
                        ..
                    } if practice.matches(delta) => Some(*actor),
                    _ => None,
                })
                .ok_or(Error::Unavailable)?;
            if batch.touches::<game_skills::Skills>(StableId(actor.0)) {
                admitted.protect::<game_skills::Skills>(StableId(actor.0));
            }
        }
        if let Some(injury) = injury {
            injury.validate(store, batch)?;
            let target = self
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Struck { target, damage, .. } if injury.matches(*target, *damage) => {
                        Some(*target)
                    }
                    _ => None,
                })
                .ok_or(Error::Unavailable)?;
            admitted.protect::<game_physiology::Body>(StableId(target.0));
        }
        if let Some(confinement) = confinement {
            confinement.validate(store, batch)?;
            let target = self
                .effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::Capture { target, context }
                        if confinement.matches_capture(
                            self.before.order[self.before.cursor],
                            *target,
                            self.after.id,
                            context,
                        ) =>
                    {
                        Some(*target)
                    }
                    _ => None,
                })
                .ok_or(Error::Unavailable)?;
            admitted.protect::<Captivity>(StableId(target.0));
        }
        let encounter_id = self.after.id;
        let id = StableId(self.before.id.0);
        reads.expect_row(id, Some(self.before));
        if batch.touches::<Encounter>(id) {
            return Err(Error::Unavailable);
        }
        admitted.protect::<Encounter>(id);
        batch.insert(id, self.after);
        if let Some((before, after)) = self.map_change {
            let id = StableId(before.id.0);
            reads.expect_row(id, Some(before));
            if batch.touches::<Map>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Map>(id);
            batch.insert(id, after);
        }
        for actor in self.new_members {
            let id = StableId(actor.0);
            reads.read::<Participation>(store, id);
            if batch.touches::<Participation>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Participation>(id);
            batch.insert(
                id,
                Participation {
                    encounter: encounter_id,
                },
            );
        }
        Ok(StagedCombat {
            reads: reads.bind(store, batch, &admitted)?,
            rules: self.rules,
            effects: self.effects,
        })
    }
    pub fn commit(self, world: &World, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let staged = self.stage_into(world, store, &mut batch)?;
        staged.validate(world, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(staged.effects)
    }
}
impl StagedCombat {
    pub fn into_guard(
        self,
        world: &World,
        store: &Store,
        batch: &Batch,
    ) -> Result<ReadGuard, Error> {
        self.validate(world, store, batch)?;
        Ok(self.reads)
    }
    pub fn validate(&self, world: &World, store: &Store, batch: &Batch) -> Result<(), Error> {
        if self.rules != world.rules {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store, batch)
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
}
fn patch(
    world: &World,
    store: &Store,
    before: &Encounter,
    after: Encounter,
    effects: Vec<Effect>,
) -> PreparedCombat {
    let mut reads = ReadSet::new();
    reads.read::<Map>(store, StableId(before.map.0));
    for actor in after.fighters.keys() {
        reads.read::<game_physiology::Body>(store, StableId(actor.0));
        reads.read::<game_skills::Skills>(store, StableId(actor.0));
        reads.read::<Captivity>(store, StableId(actor.0));
        let mut anchor = StableId(actor.0);
        let mut seen = BTreeSet::new();
        while seen.insert(anchor) {
            match reads.read::<Location>(store, anchor) {
                Some(Location::Carried { carrier }) => anchor = StableId(carrier.0),
                _ => break,
            }
        }
    }
    PreparedCombat {
        before: before.clone(),
        after,
        map_change: None,
        effects,
        rules: world.rules.clone(),
        new_members: Vec::new(),
        reads,
    }
}
fn synchronize(store: &Store, encounter: &mut Encounter) -> Result<(), Error> {
    let positions = encounter
        .fighters
        .keys()
        .map(|actor| {
            let (anchor, _) = game_spatial::resolved_anchor(store, StableId(actor.0))?;
            let position = encounter
                .fighters
                .get(&PersonId(anchor.0))
                .ok_or(Error::Unavailable)?
                .position;
            Ok((*actor, position))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    for (actor, position) in positions {
        encounter.fighters.get_mut(&actor).unwrap().position = position;
    }
    Ok(())
}
impl World {
    pub fn prepare_turn(
        &self,
        store: &Store,
        id: EncounterId,
        movement: Option<Point>,
        action: Action,
        facts: TurnFacts,
    ) -> Result<PreparedCombat, Error> {
        self.rules.validate()?;
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        if !facts.independent
            || !(self
                .rules
                .can_fight(facts.body, captivity(store, facts.actor).is_some())
                || facts.force_can_act)
            || !facts.force_allowed
        {
            return Err(Error::Unavailable);
        }
        let position = self.turn_position(store, id, facts.actor, movement)?;
        let actor = facts.actor;
        let (anchor, _) = game_spatial::resolved_anchor(store, StableId(actor.0))?;
        let carried = anchor.0 != actor.0;
        if carried && movement.is_some_and(|point| point != before.fighters[&actor].position) {
            return Err(Error::Unavailable);
        }
        let scene = map(store, before.map).ok_or(Error::Unavailable)?;
        let reachable = |target| self.reachable(store, id, position, target);
        match action {
            Action::Attack(target)
                if target == actor
                    || !reachable(target)
                    || facts.target_body.is_none_or(|body| !body.alive) =>
            {
                return Err(Error::Unavailable);
            }
            Action::Capture(target)
                if !self.capture_possible(
                    store,
                    id,
                    actor,
                    position,
                    target,
                    facts.target_body.ok_or(Error::Unavailable)?,
                    facts.binding_available,
                ) =>
            {
                return Err(Error::Unavailable);
            }
            Action::Release(target) if !reachable(target) || !facts.release_available => {
                return Err(Error::Unavailable);
            }
            Action::MissionOrder { leader }
                if !facts.mission_order_valid || leader != actor && !reachable(leader) =>
            {
                return Err(Error::Unavailable);
            }
            Action::Retreat(destination)
                if carried
                    || !facts.retreat_route_valid
                    || !scene.exits.iter().any(|(area, p)| {
                        *area == destination
                            && scene.clear(position, *p)
                            && distance(position, *p) <= u128::from(self.rules.reach).pow(2)
                    }) =>
            {
                return Err(Error::Unavailable);
            }
            Action::Breach(obstacle) => {
                let obstacle = scene
                    .obstacles
                    .get(obstacle as usize)
                    .filter(|o| o.index == obstacle && o.integrity > 0)
                    .ok_or(Error::Unavailable)?;
                let edge = Point {
                    x: position.x.clamp(obstacle.minimum.x, obstacle.maximum.x),
                    y: position.y.clamp(obstacle.minimum.y, obstacle.maximum.y),
                };
                if distance(position, edge) > u128::from(self.rules.reach).pow(2) {
                    return Err(Error::Unavailable);
                }
            }
            _ => {}
        }
        let mut after = before.clone();
        let mut effects = Vec::new();
        let fighter = after.fighters.get_mut(&actor).ok_or(Error::Unavailable)?;
        if fighter.position != position {
            effects.push(Effect::Moved { actor, position });
        }
        fighter.position = position;
        synchronize(store, &mut after)?;
        let mut map_change = None;
        match action {
            Action::Attack(target) => {
                let strike = self.rules.strike(
                    actor,
                    facts.damage,
                    after.fighters[&target].defending,
                    facts.practice_ms,
                );
                effects.push(Effect::Struck {
                    actor,
                    target,
                    damage: strike.damage,
                    practice: strike.practice,
                });
            }
            Action::Defend => after.fighters.get_mut(&actor).unwrap().defending = true,
            Action::Surrender => {
                after.fighters.get_mut(&actor).unwrap().surrendered = true;
                effects.push(Effect::Surrendered { actor });
            }
            Action::Capture(target) => {
                effects.push(Effect::Capture {
                    target,
                    context: ConfinementContext {
                        map: after.map,
                        area: after.area,
                        location: after.location.clone(),
                        target_position: after.fighters[&target].position,
                    },
                });
            }
            Action::Release(target) => effects.push(Effect::Release { target }),
            Action::MissionOrder { .. } => effects.push(Effect::MissionOrder),
            Action::Retreat(destination) => {
                for fighter in after.fighters.values_mut() {
                    let (anchor, _) =
                        game_spatial::resolved_anchor(store, StableId(fighter.actor.0))?;
                    if anchor.0 == actor.0 {
                        fighter.withdrawn = true;
                        fighter.retreat = Some(destination);
                    }
                }
            }
            Action::Breach(obstacle) => {
                let mut changed = scene.clone();
                let obstacle_row = &mut changed.obstacles[obstacle as usize];
                let damage = obstacle_row.integrity.min(self.rules.damage);
                obstacle_row.integrity -= damage;
                effects.push(Effect::ObstacleDamaged {
                    map: scene.id,
                    obstacle,
                    damage,
                });
                map_change = Some((scene.clone(), changed));
            }
            Action::Wait => {}
        }
        after.cursor = after.cursor.checked_add(1).ok_or(Error::Overflow)?;
        let mut prepared = patch(self, store, before, after, effects);
        prepared.map_change = map_change;
        Ok(prepared)
    }
    pub fn prepare_begin_round(
        &self,
        store: &Store,
        id: EncounterId,
        fatigue: &[(PersonId, u64)],
    ) -> Result<PreparedCombat, Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let mut after = before.clone();
        after.order = self.initiative_order(store, id, fatigue)?;
        after.cursor = 0;
        for fighter in after.fighters.values_mut() {
            fighter.defending = false;
        }
        let mut prepared = patch(self, store, before, after, Vec::new());
        for actor in before.fighters.keys() {
            prepared.reads.read::<Profile>(store, StableId(actor.0));
        }
        Ok(prepared)
    }
    pub fn prepare_observation(
        &self,
        store: &Store,
        id: EncounterId,
        actor: PersonId,
        alive: &BTreeSet<PersonId>,
    ) -> Result<PreparedCombat, Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let scene = map(store, before.map).ok_or(Error::Unavailable)?;
        let fighter = before.fighters.get(&actor).ok_or(Error::Unavailable)?;
        let seen: Vec<_> = before
            .fighters
            .values()
            .filter(|other| other.actor != actor && scene.clear(fighter.position, other.position))
            .map(|other| {
                (
                    other.actor,
                    other.position,
                    !other.withdrawn && alive.contains(&other.actor),
                )
            })
            .collect();
        let mut after = before.clone();
        let known = &mut after.fighters.get_mut(&actor).unwrap().known;
        known.retain(|id, point| {
            !scene.clear(fighter.position, *point) || seen.iter().any(|(other, _, _)| other == id)
        });
        for (actor, position, alive) in seen {
            if alive {
                known.insert(actor, position);
            } else {
                known.remove(&actor);
            }
        }
        Ok(patch(self, store, before, after, Vec::new()))
    }
    pub fn synchronize_carried(&self, store: &mut Store, id: EncounterId) -> Result<(), Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let mut after = before.clone();
        synchronize(store, &mut after)?;
        patch(self, store, before, after, Vec::new()).commit(self, store)?;
        Ok(())
    }
    pub fn skip_turn(&self, store: &mut Store, id: EncounterId) -> Result<(), Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let mut after = before.clone();
        after.cursor = after.cursor.checked_add(1).ok_or(Error::Overflow)?;
        patch(self, store, before, after, Vec::new()).commit(self, store)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParticipantFacts {
    pub actor: PersonId,
    pub team: u32,
    pub body: TargetBody,
    pub independent: bool,
    pub force_can_act: bool,
    pub force_passive: bool,
    pub in_combat: bool,
}
#[derive(Clone, Debug)]
pub struct EncounterOpening {
    location: Location,
    area: AreaId,
    ends: SimTime,
    event_bound: usize,
    reads: ReadSet,
    rules: Rules,
    participants: Vec<(PersonId, u32)>,
    projected_locations: BTreeMap<StableId, Location>,
}
impl EncounterOpening {
    pub fn location(&self) -> &Location {
        &self.location
    }
    pub fn area(&self) -> AreaId {
        self.area
    }
    pub fn event_bound(&self) -> usize {
        self.event_bound
    }
}
#[derive(Clone, Copy, Debug)]
pub struct AdmissionFacts {
    pub actor: PersonId,
    pub team: u32,
    pub body: TargetBody,
    pub in_combat: bool,
    pub same_place: bool,
    pub arrived: bool,
    pub departed: bool,
    pub from: AreaId,
}
impl World {
    pub fn prepare_opening(
        &self,
        store: &Store,
        participants: &[ParticipantFacts],
        now: SimTime,
    ) -> Result<EncounterOpening, Error> {
        self.prepare_opening_projection(store, participants, now, None)
    }
    pub fn prepare_opening_after_motion(
        &self,
        store: &Store,
        participants: &[ParticipantFacts],
        now: SimTime,
        motion: &game_transport::PreparedMotion,
    ) -> Result<EncounterOpening, Error> {
        self.prepare_opening_projection(store, participants, now, Some(motion))
    }
    fn prepare_opening_projection(
        &self,
        store: &Store,
        participants: &[ParticipantFacts],
        now: SimTime,
        motion: Option<&game_transport::PreparedMotion>,
    ) -> Result<EncounterOpening, Error> {
        self.rules.validate()?;
        if !self.rules.capacity_valid()
            || !(2..=self.rules.encounter_limit).contains(&participants.len())
            || participants
                .iter()
                .map(|p| p.team)
                .collect::<BTreeSet<_>>()
                .len()
                < 2
        {
            return Err(Error::InvalidFixture(
                "encounter needs opposing participants".into(),
            ));
        }
        let mut reads = ReadSet::new();
        let mut ids = BTreeSet::new();
        let mut place = None;
        let mut location = None;
        let mut projected_locations = BTreeMap::new();
        let mut anchors = BTreeSet::new();
        for facts in participants {
            reads.read::<game_physiology::Body>(store, StableId(facts.actor.0));
            reads.read::<Profile>(store, StableId(facts.actor.0));
            reads.read::<Captivity>(store, StableId(facts.actor.0));
            reads.read::<Participation>(store, StableId(facts.actor.0));
            let mut anchor = StableId(facts.actor.0);
            let mut seen = BTreeSet::new();
            while seen.insert(anchor) {
                match reads.read::<Location>(store, anchor) {
                    Some(Location::Carried { carrier }) => anchor = StableId(carrier.0),
                    _ => break,
                }
            }

            let (anchor, at) = game_spatial::resolved_anchor_with(StableId(facts.actor.0), |id| {
                reads.read::<Location>(store, id);
                let row = motion.map_or_else(
                    || game_spatial::location(store, id),
                    |motion| motion.projected_location(store, PersonId(id.0)),
                );
                if motion.is_some()
                    && let Some(row) = row
                {
                    projected_locations.insert(id, row.clone());
                }
                row
            })?;
            anchors.insert(PersonId(anchor.0));
            let at = at.clone();
            if !matches!(at, Location::AtArea(_) | Location::StoppedOnRoad { .. }) {
                return Err(Error::Unavailable);
            }
            let key = game_transport::position_from_location(&at, now)?.key();
            let can_act = facts.independent
                && (self
                    .rules
                    .can_fight(facts.body, captivity(store, facts.actor).is_some())
                    || facts.force_can_act);
            if place.is_some_and(|p| p != key)
                || !ids.insert(facts.actor)
                || !(can_act || facts.force_passive)
                || facts.in_combat
                || (!facts.independent && !facts.force_passive)
            {
                return Err(Error::Unavailable);
            }
            place = Some(key);
            location = Some(at);
        }
        for anchor in anchors {
            if !ids.contains(&anchor) {
                return Err(Error::Unavailable);
            }
        }
        let location = location.ok_or(Error::Unavailable)?;
        if let Some(map) = map_for_location(store, &location, now)?
            && participants
                .iter()
                .enumerate()
                .any(|(i, p)| map.deployment(i, p.team).is_none())
        {
            return Err(Error::Unavailable);
        }
        let area = match place.ok_or(Error::Unavailable)? {
            game_transport::PositionKey::Area(area) => area,
            game_transport::PositionKey::Road { from, .. } => from,
        };
        let ends = SimTime(
            now.0
                .checked_add(self.rules.round.0)
                .ok_or(Error::Overflow)?,
        );
        Ok(EncounterOpening {
            location,
            reads,
            rules: self.rules.clone(),
            participants: participants
                .iter()
                .map(|facts| (facts.actor, facts.team))
                .collect(),
            projected_locations,
            area,
            ends,
            event_bound: participants
                .len()
                .checked_mul(8)
                .and_then(|n| n.checked_add(2))
                .ok_or(Error::Overflow)?,
        })
    }
    fn initialize_encounter(
        &self,
        store: &Store,
        id: EncounterId,
        opening: &EncounterOpening,
        scene: &Map,
        participants: &[(PersonId, u32)],
        now: SimTime,
    ) -> Result<Encounter, Error> {
        let map_id = scene.id;
        let mut fighters = BTreeMap::new();
        for (i, (actor, team)) in participants.iter().enumerate() {
            let position = captivity(store, *actor)
                .filter(|c| c.map == map_id)
                .map(|c| c.position)
                .or_else(|| scene.deployment(i, *team))
                .ok_or(Error::Unavailable)?;
            fighters.insert(
                *actor,
                Fighter {
                    actor: *actor,
                    joins_round: 0,
                    team: *team,
                    position,
                    surrendered: false,
                    withdrawn: false,
                    defending: false,
                    retreat: None,
                    known: BTreeMap::new(),
                    search: SearchState::default(),
                },
            );
        }
        let mut encounter = Encounter {
            id,
            area: opening.area,
            location: opening.location.clone(),
            map: map_id,
            round: 0,
            started: now,
            ends: opening.ends,
            fighters,
            order: Vec::new(),
            cursor: 0,
            active: true,
        };
        let positions = encounter
            .fighters
            .keys()
            .map(|actor| {
                let (anchor, _) = game_spatial::resolved_anchor_with(StableId(actor.0), |id| {
                    opening
                        .projected_locations
                        .get(&id)
                        .or_else(|| game_spatial::location(store, id))
                })?;
                Ok((
                    *actor,
                    encounter
                        .fighters
                        .get(&PersonId(anchor.0))
                        .ok_or(Error::Unavailable)?
                        .position,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        for (actor, position) in positions {
            encounter.fighters.get_mut(&actor).unwrap().position = position;
        }
        Ok(encounter)
    }
    pub fn mission_admission(
        &self,
        store: &Store,
        id: EncounterId,
        facts: AdmissionFacts,
    ) -> Result<(Point, u64), Error> {
        let e = encounter(store, id).ok_or(Error::Unavailable)?;
        if !e.active
            || e.fighters.len() >= self.rules.encounter_limit
            || e.fighters.contains_key(&facts.actor)
            || !e
                .fighters
                .values()
                .any(|fighter| fighter.team == facts.team)
            || !self
                .rules
                .can_fight(facts.body, captivity(store, facts.actor).is_some())
            || facts.in_combat
            || !facts.same_place
            || !facts.arrived
            || !facts.departed
        {
            return Err(Error::Unavailable);
        }
        let (anchor, _) = game_spatial::resolved_anchor(store, StableId(facts.actor.0))?;
        if anchor.0 != facts.actor.0 && !e.fighters.contains_key(&PersonId(anchor.0)) {
            return Err(Error::Unavailable);
        }
        let scene = map(store, e.map).ok_or(Error::Unavailable)?;
        let point = scene
            .exits
            .iter()
            .find(|(area, _)| *area == facts.from)
            .map(|(_, p)| *p)
            .unwrap_or(Point {
                x: 0,
                y: scene.height / 2,
            });
        if !scene.clear(point, point) {
            return Err(Error::Unavailable);
        }
        Ok((point, e.round.checked_add(1).ok_or(Error::Overflow)?))
    }
    pub fn admission_round(&self, store: &Store, id: EncounterId) -> Result<u64, Error> {
        encounter(store, id)
            .ok_or(Error::Unavailable)?
            .round
            .checked_add(1)
            .ok_or(Error::Overflow)
    }
    pub fn prepare_admission(
        &self,
        store: &Store,
        id: EncounterId,
        actor: PersonId,
        team: u32,
        position: Point,
        joins_round: u64,
    ) -> Result<PreparedCombat, Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let scene = map(store, before.map).ok_or(Error::Unavailable)?;
        if !before.active
            || before.fighters.len() >= self.rules.encounter_limit
            || before.fighters.contains_key(&actor)
            || !scene.clear(position, position)
            || joins_round != before.round.checked_add(1).ok_or(Error::Overflow)?
        {
            return Err(Error::Unavailable);
        }
        let mut after = before.clone();
        after.fighters.insert(
            actor,
            Fighter {
                actor,
                joins_round,
                team,
                position,
                surrendered: false,
                withdrawn: false,
                defending: false,
                retreat: None,
                known: BTreeMap::new(),
                search: SearchState::default(),
            },
        );
        synchronize(store, &mut after)?;
        let mut prepared = patch(self, store, before, after, Vec::new());
        prepared.new_members.push(actor);
        Ok(prepared)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RoundFighterFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub force_can_act: bool,
    pub carried: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetreatMovement {
    Carried,
    Road,
    Area,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundRetreat {
    pub actor: PersonId,
    pub destination: AreaId,
    pub movement: RetreatMovement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundOutcome {
    pub round: u64,
    pub ended: bool,
    pub retreats: Vec<RoundRetreat>,
    pub idle: Vec<PersonId>,
}
pub struct PreparedRound {
    prepared: PreparedCombat,
    outcome: RoundOutcome,
}
impl PreparedRound {
    pub fn outcome(&self) -> &RoundOutcome {
        &self.outcome
    }
    pub fn commit(self, world: &World, store: &mut Store) -> Result<RoundOutcome, Error> {
        self.prepared.commit(world, store)?;
        Ok(self.outcome)
    }
}
impl World {
    pub fn prepare_finish_round(
        &self,
        store: &Store,
        id: EncounterId,
        now: SimTime,
        facts: &[RoundFighterFacts],
    ) -> Result<PreparedRound, Error> {
        let before = encounter(store, id).ok_or(Error::Unavailable)?;
        let facts: BTreeMap<_, _> = facts.iter().map(|f| (f.actor, *f)).collect();
        let mut after = before.clone();
        let mut retreats = Vec::new();
        for fighter in after.fighters.values_mut() {
            if let Some(destination) = fighter.retreat.take() {
                let fact = facts.get(&fighter.actor).ok_or(Error::Unavailable)?;
                retreats.push(RoundRetreat {
                    actor: fighter.actor,
                    destination,
                    movement: if fact.carried {
                        RetreatMovement::Carried
                    } else if matches!(before.location, Location::StoppedOnRoad { .. }) {
                        RetreatMovement::Road
                    } else {
                        RetreatMovement::Area
                    },
                });
            }
        }
        let mut teams = BTreeSet::new();
        for fighter in before.fighters.values() {
            let fact = facts.get(&fighter.actor).ok_or(Error::Unavailable)?;
            if !fighter.withdrawn
                && fact.alive
                && (captivity(store, fighter.actor).is_none()
                    || fact.force_can_act && !fighter.surrendered)
            {
                teams.insert(fighter.team);
            }
        }
        let ended = teams.len() <= 1;
        let idle = if ended {
            before.fighters.keys().copied().collect()
        } else {
            Vec::new()
        };
        if ended {
            after.active = false;
        } else {
            after.started = now;
            after.ends = SimTime(
                now.0
                    .checked_add(self.rules.round.0)
                    .ok_or(Error::Overflow)?,
            );
            after.round = after.round.checked_add(1).ok_or(Error::Overflow)?;
        }
        Ok(PreparedRound {
            prepared: patch(self, store, before, after, Vec::new()),
            outcome: RoundOutcome {
                round: before.round,
                ended,
                retreats,
                idle,
            },
        })
    }
    pub fn awaiting_input(
        &self,
        store: &Store,
        actor: PersonId,
        controlled: bool,
    ) -> Option<EncounterId> {
        let e = actor_encounter(store, actor)?;
        (e.active && !controlled && e.order.get(e.cursor) == Some(&actor)).then_some(e.id)
    }
    pub fn current_encounter(
        &self,
        store: &Store,
        actor: PersonId,
        alive: bool,
        same_place: bool,
    ) -> Option<EncounterId> {
        let e = actor_encounter(store, actor)?;
        (alive && same_place && !e.fighters.get(&actor)?.withdrawn).then_some(e.id)
    }
    pub fn npc_turn(
        &self,
        store: &Store,
        id: EncounterId,
        ruler: PersonId,
        controlled: bool,
        can_act: bool,
    ) -> Result<NpcTurn, Error> {
        let e = encounter(store, id).ok_or(Error::Unavailable)?;
        let Some(actor) = e.order.get(e.cursor).copied() else {
            return Ok(NpcTurn::Finished);
        };
        let fighter = e.fighters.get(&actor).ok_or(Error::Unavailable)?;
        if !can_act || fighter.withdrawn || fighter.surrendered {
            return Ok(NpcTurn::Skip);
        }
        if actor == ruler && !controlled {
            return Ok(NpcTurn::AwaitInput);
        }
        Ok(NpcTurn::Act(actor))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcTurn {
    Finished,
    Skip,
    AwaitInput,
    Act(PersonId),
}

pub struct PreparedStart {
    encounter: Encounter,
    scene: Map,
    reads: ReadSet,
    rules: Rules,
    projected_locations: BTreeMap<StableId, Location>,
}
impl PreparedStart {
    pub fn encounter(&self) -> &Encounter {
        &self.encounter
    }
    pub fn stage_into(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        self.stage_projection(world, store, batch, None)
    }
    pub fn stage_after_motion(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        motion: &game_transport::MotionFinalization,
    ) -> Result<ReadGuard, Error> {
        self.stage_projection(world, store, batch, Some(motion))
    }
    fn stage_projection(
        self,
        world: &World,
        store: &Store,
        batch: &mut Batch,
        motion: Option<&game_transport::MotionFinalization>,
    ) -> Result<ReadGuard, Error> {
        if self.rules != world.rules {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        if let Some(motion) = motion {
            motion.validate_footprint(store, batch)?;
            let locations: BTreeSet<_> = motion.location_ids().collect();
            for (id, expected) in &self.projected_locations {
                if batch.projected::<Location>(store, *id)? != Some(expected) {
                    return Err(Error::Unavailable);
                }
                if batch.touches::<Location>(*id) {
                    if !locations.contains(id) {
                        return Err(Error::Unavailable);
                    }
                    admitted.protect::<Location>(*id);
                }
            }
        } else if !self.projected_locations.is_empty() {
            return Err(Error::Unavailable);
        }
        if batch.projected::<Map>(store, StableId(self.scene.id.0))? != Some(&self.scene) {
            return Err(Error::Unavailable);
        }
        if batch.touches::<Map>(StableId(self.scene.id.0)) {
            admitted.protect::<Map>(StableId(self.scene.id.0));
        }
        let id = StableId(self.encounter.id.0);
        if batch.touches::<Encounter>(id) {
            return Err(Error::Unavailable);
        }
        admitted.protect::<Encounter>(id);
        for actor in self.encounter.fighters.keys() {
            let id = StableId(actor.0);
            if batch.touches::<Participation>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Participation>(id);
            batch.insert(
                id,
                Participation {
                    encounter: self.encounter.id,
                },
            );
        }
        batch.insert(id, self.encounter);
        self.reads.bind(store, batch, &admitted)
    }
}
impl World {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_start(
        &self,
        store: &Store,
        id: EncounterId,
        opening: EncounterOpening,
        scene: &Map,
        participants: &[(PersonId, u32)],
        fatigue: &[(PersonId, u64)],
        now: SimTime,
    ) -> Result<PreparedStart, Error> {
        if opening.rules != self.rules || opening.participants != participants {
            return Err(Error::Unavailable);
        }
        opening.reads.validate(store)?;
        let mut encounter =
            self.initialize_encounter(store, id, &opening, scene, participants, now)?;
        encounter.order = rank_initiative(store, &encounter, fatigue)?;
        let mut reads = opening.reads;
        reads.read::<Map>(store, StableId(scene.id.0));
        if scene.area != opening.area
            || game_transport::position_from_location(&scene.location, now)?.key()
                != game_transport::position_from_location(&opening.location, now)?.key()
        {
            return Err(Error::Unavailable);
        }
        reads.exclusive_entity(store, StableId(id.0))?;
        reads.expect_row::<Encounter>(StableId(id.0), None);
        for actor in encounter.fighters.keys() {
            reads.read::<Profile>(store, StableId(actor.0));
            reads.read::<Captivity>(store, StableId(actor.0));
            reads.read::<Participation>(store, StableId(actor.0));
            let mut anchor = StableId(actor.0);
            let mut seen = BTreeSet::new();
            while seen.insert(anchor) {
                match reads.read::<Location>(store, anchor) {
                    Some(Location::Carried { carrier }) => anchor = StableId(carrier.0),
                    _ => break,
                }
            }
        }
        Ok(PreparedStart {
            encounter,
            scene: scene.clone(),
            reads,
            rules: self.rules.clone(),
            projected_locations: opening.projected_locations,
        })
    }
}
pub(crate) fn rank_initiative(
    store: &Store,
    e: &Encounter,
    fatigue: &[(PersonId, u64)],
) -> Result<Vec<PersonId>, Error> {
    let fatigue: BTreeMap<_, _> = fatigue.iter().copied().collect();
    let mut ranked = Vec::new();
    for fighter in e
        .fighters
        .values()
        .filter(|f| !f.withdrawn && f.joins_round <= e.round)
    {
        let profile = profile(store, fighter.actor).ok_or(Error::Unavailable)?;
        let fatigue = fatigue.get(&fighter.actor).ok_or(Error::Unavailable)?;
        ranked.push((
            std::cmp::Reverse(profile.initiative.saturating_sub(fatigue / 1_000)),
            fighter.actor,
        ));
    }
    ranked.sort_unstable();
    Ok(ranked.into_iter().map(|(_, actor)| actor).collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetreatRequest {
    Area {
        from: AreaId,
        to: AreaId,
    },
    Road {
        point: game_core::RoadPosition,
        to: AreaId,
    },
}
impl World {
    pub fn preflight_rounds(
        &self,
        store: &Store,
        ids: &[EncounterId],
        at: SimTime,
    ) -> Result<Vec<RetreatRequest>, Error> {
        let mut requests = Vec::new();
        for id in ids {
            let encounter = encounter(store, *id).ok_or(Error::Unavailable)?;
            encounter.round.checked_add(1).ok_or(Error::Overflow)?;
            at.0.checked_add(self.rules.round.0)
                .ok_or(Error::Overflow)?;
            for fighter in encounter.fighters.values() {
                if let Some(to) = fighter.retreat {
                    requests.push(match encounter.location {
                        Location::StoppedOnRoad {
                            from,
                            to: destination,
                            progress,
                            duration,
                        } => RetreatRequest::Road {
                            point: game_core::RoadPosition {
                                from,
                                to: destination,
                                progress,
                                duration,
                            },
                            to,
                        },
                        _ => RetreatRequest::Area {
                            from: encounter.area,
                            to,
                        },
                    });
                }
            }
        }
        Ok(requests)
    }
    pub fn visible_fighters<'a>(
        &self,
        store: &'a Store,
        id: EncounterId,
        viewer: PersonId,
        present: &BTreeSet<PersonId>,
    ) -> Result<Vec<&'a Fighter>, Error> {
        let encounter = encounter(store, id).ok_or(Error::Unavailable)?;
        let map = map(store, encounter.map).ok_or(Error::Unavailable)?;
        let mine = encounter
            .fighters
            .get(&viewer)
            .ok_or(Error::Unavailable)?
            .position;
        Ok(encounter
            .fighters
            .values()
            .filter(|fighter| {
                fighter.actor == viewer
                    || present.contains(&fighter.actor) && map.clear(mine, fighter.position)
            })
            .collect())
    }
}
