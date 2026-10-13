//! Body interval policy and reactions to factual activity and consumption inputs.
use crate::{ActivityFlags, Body, BodyLimits, traits::Phenotype};
use game_core::{Error, PersonId, Quantity, SimDuration, SimTime, Vitals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub hunger_harm: u64,
    pub fatigue_limit: u64,
    pub max_health: u64,
    pub meal_relief: u64,
    pub meal_size: Quantity,
    pub eating_time: SimDuration,
    pub loading_time: SimDuration,
    pub carrying_capacity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub rules: Rules,
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.hunger_harm == 0
            || self.fatigue_limit == 0
            || self.max_health == 0
            || self.meal_relief == 0
            || self.meal_size.0 == 0
            || self.eating_time.0 == 0
            || self.loading_time.0 == 0
            || self.carrying_capacity.0 == 0
        {
            return Err(Error::InvalidConfiguration("invalid physical rules".into()));
        }
        Ok(())
    }
    pub fn limits(&self) -> BodyLimits {
        BodyLimits {
            hunger_harm: self.hunger_harm,
            fatigue_limit: self.fatigue_limit,
            max_health: self.max_health,
        }
    }
    pub fn expressed(&self, phenotype: Option<Phenotype>) -> Self {
        let mut rules = self.clone();
        if let Some(phenotype) = phenotype {
            let scale = |value: u64, numerator: u64, denominator: u64| {
                (u128::from(value) * u128::from(numerator) / u128::from(denominator.max(1)))
                    .min(u128::from(u64::MAX)) as u64
            };
            rules.max_health = scale(
                rules.max_health,
                u64::from(phenotype.max_health_milli),
                1_000,
            )
            .max(1);
            rules.carrying_capacity.0 = scale(
                rules.carrying_capacity.0,
                u64::from(phenotype.strength_milli),
                1_000,
            );
            rules.hunger_harm = scale(
                rules.hunger_harm,
                1_000,
                u64::from(phenotype.metabolism_milli),
            );
            rules.meal_relief = scale(
                rules.meal_relief,
                1_000,
                u64::from(phenotype.metabolism_milli),
            );
        }
        rules
    }
    pub fn newborn(&self) -> Result<Body, Error> {
        self.validate()?;
        Ok(Body {
            vitals: Vitals {
                hunger: 0,
                fatigue: 0,
                health: self.max_health,
                alive: true,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivityKind {
    RealmTalk,
    #[default]
    Other,
    Idle,
    Eat,
    Care,
    Comfort,
    Express,
    Lift,
    HealthWork,
    Unbind,
    Escape,
    Restrain,
    Rest,
    Work,
    Gather,
    Load,
    Take,
    Construct,
    Combat,
    ArmyApproach,
    Play,
    Cultivate,
}
impl ActivityKind {
    fn exerting(self) -> bool {
        matches!(
            self,
            Self::Work
                | Self::Gather
                | Self::Load
                | Self::Take
                | Self::Construct
                | Self::Combat
                | Self::ArmyApproach
                | Self::Play
                | Self::Cultivate
        )
    }
    fn stops_at_exhaustion(self) -> bool {
        matches!(
            self,
            Self::Work
                | Self::Gather
                | Self::Load
                | Self::Take
                | Self::Construct
                | Self::Play
                | Self::Cultivate
        )
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ActivityFacts {
    pub activity: ActivityKind,
    pub transport_effort: bool,
    pub health_work_effort: bool,
    pub social_practice: bool,
    pub patient_resting: bool,
}
impl ActivityFacts {
    pub fn flags(self) -> ActivityFlags {
        ActivityFlags {
            exerting: self.activity.exerting()
                || self.transport_effort
                || self.health_work_effort
                || self.social_practice,
            resting: self.activity == ActivityKind::Rest || self.patient_resting,
        }
    }
}
#[derive(Clone, Debug)]
pub struct BoundaryFacts {
    pub actor: PersonId,
    pub body: Body,
    pub rules: Rules,
    pub hunger_exempt: bool,
    pub incapacitation: Option<u64>,
}
/// Select the first stable-rate body transition, clipping hunger's slope change first.
/// Projection supplies contributions from their owners through the normal integration path.
pub fn next_boundary(
    now: SimTime,
    limit: SimTime,
    facts: &[BoundaryFacts],
    mut project: impl FnMut(PersonId, u64) -> Result<Vitals, Error>,
) -> Result<SimTime, Error> {
    let mut next = limit;
    for row in facts {
        if row.body.vitals.alive
            && !row.hunger_exempt
            && row.body.vitals.hunger < row.rules.hunger_harm
        {
            let remaining = row.rules.hunger_harm - row.body.vitals.hunger;
            if remaining < next.0.checked_sub(now.0).ok_or(Error::Overflow)? {
                next = SimTime(now.0.checked_add(remaining).ok_or(Error::Overflow)?);
            }
        }
    }
    for row in facts {
        let before = &row.body.vitals;
        if !before.alive {
            continue;
        }
        let elapsed = next.0.checked_sub(now.0).ok_or(Error::Overflow)?;
        if let Some(dt) = crate::first_crossing(elapsed, |dt| {
            let after = project(row.actor, dt)?;
            Ok(!after.alive
                || before.fatigue < row.rules.fatigue_limit
                    && after.fatigue >= row.rules.fatigue_limit
                || before.fatigue > 0 && after.fatigue == 0
                || before.health < row.rules.max_health && after.health >= row.rules.max_health
                || row.incapacitation.is_some_and(|threshold| {
                    (before.health > threshold) != (after.health > threshold)
                }))
        })? {
            next = SimTime(now.0.checked_add(dt).ok_or(Error::Overflow)?);
        }
    }
    Ok(next)
}
#[derive(Clone, Copy, Debug, Default)]
pub struct IntervalReactionFacts {
    pub consumed_food: bool,
    pub activity: ActivityKind,
    pub practical_social_activity: bool,
    pub health_work_activity: bool,
    pub social_tool_selected: bool,
    pub health_tool_selected: bool,
    pub equipped: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalReaction {
    pub interrupted: bool,
    pub died: bool,
    pub recovered: bool,
}
/// Receive a completed numerical interval and actual same-boundary consumption.
pub fn receive_interval(
    before: &Body,
    mut after: Body,
    rules: &Rules,
    facts: IntervalReactionFacts,
) -> Result<(Body, IntervalReaction), Error> {
    if !before.vitals.alive {
        after.vitals.alive = false;
        after.vitals.health = 0;
        after.health_remainder = 0;
    }
    if facts.consumed_food {
        after.vitals.hunger = after.vitals.hunger.saturating_sub(rules.meal_relief);
    }
    after.validate()?;
    let exhausted = after.vitals.fatigue >= rules.fatigue_limit;
    let reaction = IntervalReaction {
        interrupted: exhausted
            && (facts.activity.stops_at_exhaustion()
                || facts.practical_social_activity
                || facts.health_work_activity)
            || !facts.equipped
                && (facts.practical_social_activity && facts.social_tool_selected
                    || facts.health_tool_selected
                    || facts.activity == ActivityKind::Work),
        died: before.vitals.alive && !after.vitals.alive,
        recovered: before.vitals.health < rules.max_health
            && after.vitals.health == rules.max_health,
    };
    Ok((after, reaction))
}
#[derive(Clone, Copy, Debug)]
pub struct ValidationFacts {
    pub actor: PersonId,
    pub hunger_exempt: bool,
    pub in_transit: bool,
    pub carried: u128,
    pub idle: bool,
    pub has_work: bool,
    pub phenotype: Option<Phenotype>,
}
pub fn validate_with_facts(
    store: &game_ecs::Store,
    rules: &Rules,
    facts: &[ValidationFacts],
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent physical body facts".into());
    rules.validate().map_err(|_| fail())?;
    let expected: std::collections::BTreeSet<_> = facts.iter().map(|row| row.actor).collect();
    if expected.len() != facts.len()
        || crate::bodies(store)
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>()
            != expected
    {
        return Err(fail());
    }
    for row in facts {
        let body = crate::body(store, row.actor).ok_or_else(fail)?;
        let rules = rules.expressed(row.phenotype);
        if body.validate().is_err()
            || body.vitals.health > rules.max_health
            || body.vitals.fatigue > rules.fatigue_limit
            || row.in_transit && row.carried > u128::from(rules.carrying_capacity.0)
            || row.hunger_exempt && (body.vitals.hunger != 0 || !body.vitals.alive)
            || !body.vitals.alive && (!row.idle || row.has_work)
        {
            return Err(fail());
        }
    }
    Ok(())
}

/// Publish the owner's already evaluated body through registered component validation.
pub fn receive_body(store: &mut game_ecs::Store, actor: PersonId, body: Body) -> Result<(), Error> {
    if crate::body(store, actor).is_none() {
        return Err(Error::Unavailable);
    }
    store.insert(game_ecs::StableId(actor.0), body)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules() -> Rules {
        Rules {
            hunger_harm: 100,
            fatigue_limit: 200,
            max_health: 300,
            meal_relief: 50,
            meal_size: Quantity(1),
            eating_time: SimDuration(1),
            loading_time: SimDuration(1),
            carrying_capacity: Quantity(100),
        }
    }
    fn store() -> game_ecs::Store {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        game_ecs::Store::with_schema(schema)
    }
    #[test]
    fn boundary_uses_expressed_limits_and_hunger_slope_before_projection() {
        let rules = rules();
        let mut body = rules.newborn().unwrap();
        body.vitals.health = 20;
        body.vitals.hunger = 90;
        let facts = [BoundaryFacts {
            actor: PersonId(1),
            body: body.clone(),
            rules: rules.clone(),
            hunger_exempt: false,
            incapacitation: Some(10),
        }];
        let project = |_: PersonId, dt| {
            crate::integrate(
                &body,
                rules.limits(),
                crate::BodyInput {
                    elapsed_ms: dt,
                    hunger_exempt: false,
                    immortal: false,
                    activity: ActivityFlags::default(),
                    contributions: &[crate::Contribution {
                        health_milli: -2_000 * i128::from(dt),
                        fatigue_milli: 0,
                    }],
                },
            )
            .map(|body| body.vitals)
        };
        assert_eq!(
            next_boundary(SimTime(10), SimTime(100), &facts, project).unwrap(),
            SimTime(15)
        );
        let mut reversed = facts.to_vec();
        reversed.reverse();
        assert_eq!(
            next_boundary(SimTime(10), SimTime(100), &reversed, project).unwrap(),
            SimTime(15)
        );
    }
    #[test]
    fn receiving_interval_owns_consumption_exhaustion_death_and_recovery() {
        let rules = rules();
        let mut before = rules.newborn().unwrap();
        before.vitals.health = 200;
        let mut after = before.clone();
        after.vitals.health = 300;
        after.vitals.fatigue = 200;
        after.vitals.hunger = 70;
        let (after, reaction) = receive_interval(
            &before,
            after,
            &rules,
            IntervalReactionFacts {
                consumed_food: true,
                activity: ActivityKind::Work,
                equipped: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(after.vitals.hunger, 20);
        assert_eq!(
            reaction,
            IntervalReaction {
                interrupted: true,
                died: false,
                recovered: true
            }
        );
        let mut store = store();
        crate::insert_body(&mut store, PersonId(1), before.clone()).unwrap();
        receive_body(&mut store, PersonId(1), after.clone()).unwrap();
        assert_eq!(crate::body(&store, PersonId(1)), Some(&after));
        let snapshot = store.snapshot().unwrap();
        let mut invalid = after;
        invalid.health_remainder = 1_000;
        assert!(receive_body(&mut store, PersonId(1), invalid).is_err());
        assert_eq!(store.snapshot().unwrap(), snapshot);
    }
    #[test]
    fn body_validation_receives_explicit_foreign_facts_and_preserves_store() {
        let rules = rules();
        let mut store = store();
        crate::insert_body(&mut store, PersonId(1), rules.newborn().unwrap()).unwrap();
        let facts = ValidationFacts {
            actor: PersonId(1),
            hunger_exempt: false,
            in_transit: true,
            carried: 100,
            idle: true,
            has_work: false,
            phenotype: None,
        };
        validate_with_facts(&store, &rules, &[facts]).unwrap();
        let before = store.snapshot().unwrap();
        assert!(
            validate_with_facts(
                &store,
                &rules,
                &[ValidationFacts {
                    carried: 101,
                    ..facts
                }]
            )
            .is_err()
        );
        assert!(validate_with_facts(&store, &rules, &[facts, facts]).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn authored_body_rules_and_activity_contributions_are_owner_policies() {
        let mut configured = rules();
        configured.max_health = 600;
        let phenotype = Phenotype {
            strength_milli: 1_500,
            max_health_milli: 500,
            metabolism_milli: 2_000,
            cold_tolerance_centi: 0,
        };
        let expressed = configured.expressed(Some(phenotype));
        assert_eq!(
            (
                expressed.max_health,
                expressed.hunger_harm,
                expressed.meal_relief,
                expressed.carrying_capacity.0
            ),
            (300, 50, 25, 150)
        );
        assert_eq!(expressed.newborn().unwrap().vitals.health, 300);
        assert!(
            ActivityFacts {
                patient_resting: true,
                transport_effort: true,
                ..Default::default()
            }
            .flags()
            .exerting
        );
        assert!(
            ActivityFacts {
                patient_resting: true,
                ..Default::default()
            }
            .flags()
            .resting
        );
    }
}

/// Receive a lifespan or other owner-directed death once.
pub fn receive_death(store: &mut game_ecs::Store, actor: PersonId) -> Result<bool, Error> {
    crate::prepare_death(store, actor)?.commit(store)
}
/// Receive an instantaneous injury without teaching the source feature body rules.
pub fn receive_injury(
    store: &mut game_ecs::Store,
    actor: PersonId,
    damage: u64,
    immortal: bool,
) -> Result<bool, Error> {
    crate::prepare_injury(store, actor, damage, immortal)?.commit(store)
}
pub fn receive_health_expression(
    store: &mut game_ecs::Store,
    actor: PersonId,
    old_factor: u16,
    new_factor: u16,
) -> Result<(), Error> {
    let mut body = crate::body(store, actor).ok_or(Error::Unavailable)?.clone();
    body.rescale_health_factor(old_factor, new_factor);
    receive_body(store, actor, body)
}

/// An evaluated body interval retains the factual component joins that supplied its rates.
#[derive(Clone, Debug)]
pub struct EvaluatedInterval {
    actor: PersonId,
    after: Body,
    phenotype: Option<Phenotype>,
    reads: game_ecs::ReadSet,
}
impl EvaluatedInterval {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
    pub fn after(&self) -> &Body {
        &self.after
    }
}
pub fn prepare_interval_output(
    store: &game_ecs::Store,
    actor: PersonId,
    after: Body,
    phenotype: Option<Phenotype>,
    mut factual_reads: game_ecs::ReadSet,
) -> Result<EvaluatedInterval, Error> {
    after.validate()?;
    factual_reads
        .read::<Body>(store, game_ecs::StableId(actor.0))
        .ok_or(Error::Unavailable)?;
    factual_reads.read::<crate::traits::Development>(store, game_ecs::StableId(actor.0));
    Ok(EvaluatedInterval {
        actor,
        after,
        phenotype,
        reads: factual_reads,
    })
}
/// Capture the authoritative Body membership once for an evaluated frame.
/// Cloned read sets share immutable witnesses across its per-actor outputs.
pub fn body_roster_reads(store: &game_ecs::Store) -> game_ecs::ReadSet {
    let mut reads = game_ecs::ReadSet::new();
    reads.membership::<Body>(store, |_, _| true);
    reads
}
pub fn validate_interval_outputs<'a>(
    store: &game_ecs::Store,
    expected: impl IntoIterator<Item = PersonId>,
    outputs: impl IntoIterator<Item = &'a EvaluatedInterval>,
    mut phenotype: impl FnMut(PersonId) -> Option<Phenotype>,
) -> Result<(), Error> {
    let expected = expected
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let mut actual = std::collections::BTreeSet::new();
    for output in outputs {
        if !actual.insert(output.actor) || output.phenotype != phenotype(output.actor) {
            return Err(Error::InvalidSave("stale physical component batch".into()));
        }
        output
            .reads
            .validate(store)
            .map_err(|_| Error::InvalidSave("stale physical component batch".into()))?;
    }
    if actual != expected {
        return Err(Error::InvalidSave(
            "incomplete physical component batch".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod receiving_tests {
    use super::*;
    #[test]
    fn opening_body_membership_rejects_stale_population_before_incomplete_count() {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        let body = Body {
            vitals: Vitals {
                health: 100,
                hunger: 0,
                fatigue: 0,
                alive: true,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        };
        crate::insert_body(&mut store, PersonId(1), body.clone()).unwrap();
        crate::insert_body(&mut store, PersonId(2), body.clone()).unwrap();
        let roster = body_roster_reads(&store);
        let first =
            prepare_interval_output(&store, PersonId(1), body.clone(), None, roster).unwrap();
        let second = prepare_interval_output(
            &store,
            PersonId(2),
            body.clone(),
            None,
            game_ecs::ReadSet::new(),
        )
        .unwrap();
        assert_eq!(
            validate_interval_outputs(&store, [PersonId(1), PersonId(2)], [&first], |_| None),
            Err(Error::InvalidSave(
                "incomplete physical component batch".into()
            ))
        );
        crate::insert_body(&mut store, PersonId(3), body).unwrap();
        let before = store.snapshot().unwrap();
        assert_eq!(
            validate_interval_outputs(
                &store,
                [PersonId(1), PersonId(2), PersonId(3)],
                [&first, &second],
                |_| None
            ),
            Err(Error::InvalidSave("stale physical component batch".into()))
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn evaluated_interval_rejects_stale_fact_joins_and_death_is_owned_once() {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        schema
            .register::<Quantity>("test/load", |_| Ok(()))
            .unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        let actor = PersonId(1);
        let body = Body {
            vitals: Vitals {
                health: 100,
                hunger: 0,
                fatigue: 0,
                alive: true,
            },
            health_remainder: 0,
            fatigue_remainder: 0,
        };
        crate::insert_body(&mut store, actor, body.clone()).unwrap();
        store.insert(game_ecs::StableId(1), Quantity(10)).unwrap();
        let mut reads = game_ecs::ReadSet::new();
        reads.read::<Quantity>(&store, game_ecs::StableId(1));
        let output = prepare_interval_output(&store, actor, body, None, reads).unwrap();
        validate_interval_outputs(&store, [actor], [&output], |_| None).unwrap();
        store.insert(game_ecs::StableId(1), Quantity(11)).unwrap();
        let before = store.snapshot().unwrap();
        assert!(validate_interval_outputs(&store, [actor], [&output], |_| None).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert!(!receive_injury(&mut store, actor, 100, true).unwrap());
        assert!(crate::body(&store, actor).unwrap().vitals.alive);
        assert!(receive_death(&mut store, actor).unwrap());
        assert!(!receive_death(&mut store, actor).unwrap());
        receive_health_expression(&mut store, actor, 1_000, 2_000).unwrap();
        assert!(!crate::body(&store, actor).unwrap().vitals.alive);
        assert_eq!(crate::body(&store, actor).unwrap().vitals.health, 0);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WorkDeadlineFacts {
    pub alive: bool,
    pub active: bool,
    pub completion_after: Option<u64>,
}
pub fn next_work_boundary(
    now: SimTime,
    limit: SimTime,
    facts: impl IntoIterator<Item = WorkDeadlineFacts>,
) -> SimTime {
    let available = limit.0.saturating_sub(now.0);
    let mut elapsed = available;
    for row in facts {
        if row.alive
            && row.active
            && let Some(completion) = row.completion_after
        {
            elapsed = elapsed.min(completion);
        }
    }
    SimTime(now.0 + elapsed.max(1).min(available))
}
impl crate::Contribution {
    pub fn from_strain_and_injury_rates(
        elapsed: u64,
        strain: u64,
        injury: u64,
    ) -> Result<Self, Error> {
        Ok(Self {
            health_milli: i128::from(elapsed)
                .checked_mul(i128::from(injury))
                .and_then(i128::checked_neg)
                .ok_or(Error::Overflow)?,
            fatigue_milli: i128::from(elapsed)
                .checked_mul(i128::from(strain))
                .ok_or(Error::Overflow)?,
        })
    }
}

pub fn validate_location_activity(
    location: &game_core::Location,
    activity: ActivityKind,
) -> Result<(), Error> {
    let allowed = match location {
        game_core::Location::AtArea(_) => true,
        game_core::Location::StoppedOnRoad { .. } => matches!(
            activity,
            ActivityKind::Idle
                | ActivityKind::Rest
                | ActivityKind::Eat
                | ActivityKind::Load
                | ActivityKind::Care
                | ActivityKind::Comfort
                | ActivityKind::Express
                | ActivityKind::Lift
                | ActivityKind::HealthWork
                | ActivityKind::Unbind
                | ActivityKind::Escape
                | ActivityKind::Restrain
                | ActivityKind::Combat
        ),
        game_core::Location::InTransit { .. } => matches!(
            activity,
            ActivityKind::Idle | ActivityKind::Express | ActivityKind::Escape
        ),
        game_core::Location::Carried { .. } => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(Error::InvalidSave(
            "physical activity conflicts with position".into(),
        ))
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    #[test]
    fn dead_or_inactive_work_cannot_create_a_zero_scheduling_interval() {
        let facts = [
            WorkDeadlineFacts {
                alive: false,
                active: true,
                completion_after: Some(0),
            },
            WorkDeadlineFacts {
                alive: true,
                active: false,
                completion_after: Some(0),
            },
            WorkDeadlineFacts {
                alive: true,
                active: true,
                completion_after: Some(7),
            },
        ];
        assert_eq!(
            next_work_boundary(SimTime(10), SimTime(30), facts),
            SimTime(17)
        );
        assert_eq!(
            next_work_boundary(SimTime(10), SimTime(30), facts.into_iter().rev()),
            SimTime(17)
        );
        assert_eq!(
            next_work_boundary(SimTime(10), SimTime(10), []),
            SimTime(10)
        );
        let rates = crate::Contribution::from_strain_and_injury_rates(7, 71, 333).unwrap();
        assert_eq!((rates.fatigue_milli, rates.health_milli), (497, -2_331));
    }
}

pub struct PreparedIntervalReception {
    actor: PersonId,
    after: Body,
    reaction: IntervalReaction,
    reads: game_ecs::ReadSet,
}
pub fn prepare_interval_reception(
    store: &game_ecs::Store,
    output: &EvaluatedInterval,
    rules: &Rules,
    facts: IntervalReactionFacts,
) -> Result<PreparedIntervalReception, Error> {
    let actor = output.actor;
    let mut reads = game_ecs::ReadSet::new();
    let before = reads
        .read::<Body>(store, game_ecs::StableId(actor.0))
        .ok_or(Error::Unavailable)?;
    let (after, reaction) = receive_interval(before, output.after.clone(), rules, facts)?;
    Ok(PreparedIntervalReception {
        actor,
        after,
        reaction,
        reads,
    })
}
impl PreparedIntervalReception {
    pub fn reaction(&self) -> IntervalReaction {
        self.reaction
    }
    pub fn stage_into(
        self,
        store: &game_ecs::Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = game_ecs::StableId(self.actor.0);
        if batch.touches::<Body>(id) {
            return Err(Error::Unavailable);
        }
        batch.insert(id, self.after);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Body>(id);
        self.reads.bind(store, batch, &admitted)
    }
}

impl Rules {
    pub fn eating_deadline(&self, now: SimTime) -> Result<SimTime, Error> {
        if self.eating_time.0 == 0 {
            return Err(Error::InvalidConfiguration("zero eating duration".into()));
        }
        Ok(SimTime(
            now.0
                .checked_add(self.eating_time.0)
                .ok_or(Error::Overflow)?,
        ))
    }
}
pub fn physically_capable(body: &Body) -> bool {
    body.vitals.alive && body.vitals.health > 0
}

#[cfg(test)]
mod reception_tests {
    use super::*;
    #[test]
    fn completed_body_interval_preserves_an_already_published_death() {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        let actor = PersonId(1);
        let rules = Rules {
            hunger_harm: 100,
            fatigue_limit: 200,
            max_health: 300,
            meal_relief: 50,
            meal_size: Quantity(1),
            eating_time: SimDuration(1),
            loading_time: SimDuration(1),
            carrying_capacity: Quantity(100),
        };
        crate::insert_body(&mut store, actor, rules.newborn().unwrap()).unwrap();
        let output = prepare_interval_output(
            &store,
            actor,
            rules.newborn().unwrap(),
            None,
            game_ecs::ReadSet::new(),
        )
        .unwrap();
        crate::receive_death(&mut store, actor).unwrap();
        let reception =
            prepare_interval_reception(&store, &output, &rules, IntervalReactionFacts::default())
                .unwrap();
        assert!(!reception.reaction().died);
        let mut batch = game_ecs::Batch::new();
        let guard = reception.stage_into(&store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert!(!crate::body(&store, actor).unwrap().vitals.alive);
        assert_eq!(crate::body(&store, actor).unwrap().vitals.health, 0);
    }
}

pub fn carrying_room(capacity: Quantity, held: Quantity) -> Quantity {
    Quantity(capacity.0.saturating_sub(held.0))
}
pub fn can_add_load(capacity: Quantity, held: Quantity, added: Quantity) -> bool {
    held.0
        .checked_add(added.0)
        .is_some_and(|total| total <= capacity.0)
}
