use crate::systems::*;
use crate::{LearningEvent, Rules, Runtime, SocialFacts, register_components, skills};
use game_core::{Error, PersonId, SimDuration, SimTime, Skill};
use game_ecs::{Schema, StableId};
use game_world::{GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, RuntimeSlot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    skills: Option<Runtime>,
    facts: (),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Published(Event);
impl From<Event> for Published {
    fn from(event: Event) -> Self {
        Self(event)
    }
}

struct Facts {
    people: Vec<PersonId>,
    agriculture: Vec<WorkEvidence>,
    workshop: Vec<WorkEvidence>,
    health: Vec<WorkEvidence>,
    physical: Vec<WorkEvidence>,
    social: Vec<SocialFacts>,
}
struct FixtureFacts(Facts);
impl<E: 'static> OwnerSystem<E> for FixtureFacts {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn read_agriculture<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadAgricultureEvidence,
) -> Result<Vec<WorkEvidence>, Error> {
    Ok(owner.0.agriculture.clone())
}
fn read_workshop<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadWorkshopEvidence,
) -> Result<Vec<WorkEvidence>, Error> {
    Ok(owner.0.workshop.clone())
}
fn read_health<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadHealthEvidence,
) -> Result<Vec<WorkEvidence>, Error> {
    Ok(owner.0.health.clone())
}
fn read_physical<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadPhysicalEvidence,
) -> Result<Vec<WorkEvidence>, Error> {
    Ok(owner.0.physical.clone())
}
fn read_social<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadSocialLearningFacts,
) -> Result<Vec<SocialFacts>, Error> {
    Ok(owner.0.social.clone())
}
fn read_identity<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    request: ReadIdentity,
) -> Result<bool, Error> {
    Ok(owner.0.people.contains(&request.0))
}
fn read_body<E: 'static>(
    owner: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    request: ReadBodyLearningFacts,
) -> Result<Option<BodyLearningFacts>, Error> {
    Ok(owner
        .0
        .people
        .contains(&request.0)
        .then_some(BodyLearningFacts {
            alive: true,
            health: 1,
        }))
}
fn read_physical_enabled<E: 'static>(
    _: &FixtureFacts,
    _: &mut OwnerReadTxn<'_, '_, FixtureFacts, E>,
    _: ReadPhysicalSkillsEnabled,
) -> Result<bool, Error> {
    Ok(true)
}

fn runtime() -> Runtime {
    Runtime::new(Rules {
        per_level: SimDuration(600),
        max_level: 10,
        work_bonus_per_level: 10,
        damage_bonus_per_level: 5,
        action_practice_ms: 60,
    })
    .unwrap()
}
fn setup(facts: Facts) -> (Registry<State, Published>, GenericHost<State, Published>) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut host = GenericHost::new(
        schema,
        7,
        State {
            skills: None,
            facts: (),
        },
    );
    for person in &facts.people {
        host.storage.ensure(StableId(person.0)).unwrap();
    }
    let mut registry = Registry::new();
    register_systems(
        &mut registry,
        RuntimeSlot::new(
            |state: &State| &state.skills,
            |state: &mut State| &mut state.skills,
        ),
    )
    .unwrap();
    registry
        .register(
            FixtureFacts(facts),
            RuntimeSlot::new(
                |state: &State| &state.facts,
                |state: &mut State| &mut state.facts,
            ),
        )
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadAgricultureEvidence>(
            read_agriculture::<Published>,
        )
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadWorkshopEvidence>(read_workshop::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadHealthEvidence>(read_health::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadPhysicalEvidence>(read_physical::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadSocialLearningFacts>(read_social::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadIdentity>(read_identity::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadBodyLearningFacts>(read_body::<Published>)
        .unwrap();
    registry
        .register_read_request::<FixtureFacts, ReadPhysicalSkillsEnabled>(
            read_physical_enabled::<Published>,
        )
        .unwrap();
    (registry, host)
}
fn call(
    registry: &Registry<State, Published>,
    host: &mut GenericHost<State, Published>,
    input: Input,
) -> Result<Output, Error> {
    let at = match &input {
        Input::Advance { next, .. } => *next,
        Input::ValidateSave { now } => *now,
        _ => SimTime(0),
    };
    let prepared = registry.prepare(host, at, None, |tx| tx.call::<Systems>(input))?;
    registry.commit(host, prepared)
}
fn evidence(actor: PersonId) -> WorkEvidence {
    WorkEvidence {
        actor,
        skill: Skill::Craft,
        mastery_cap: None,
    }
}

#[test]
fn native_advance_owns_learning_rows_views_and_level_events() {
    let id = PersonId(1);
    let (registry, mut host) = setup(Facts {
        people: vec![id],
        agriculture: vec![],
        workshop: vec![evidence(id)],
        health: vec![],
        physical: vec![],
        social: vec![],
    });
    call(
        &registry,
        &mut host,
        Input::Initialize {
            runtime: runtime(),
            people: vec![id],
        },
    )
    .unwrap();
    let Output::View(Some(view)) = call(&registry, &mut host, Input::Observe(id)).unwrap() else {
        panic!("known actor has a skill view");
    };
    assert!(view.skills.iter().all(|skill| skill.level == 0));
    call(
        &registry,
        &mut host,
        Input::Advance {
            from: SimTime(0),
            next: SimTime(600),
        },
    )
    .unwrap();
    let row = skills(&host.storage, id).unwrap();
    assert_eq!(row.level(Skill::Craft), 1);
    assert_eq!(row.practice(Skill::Craft), 600);
    assert!(host.events().iter().any(|event| {
        event.event.0
            == Event::LeveledUp {
                actor: id,
                skill: Skill::Craft,
                level: 1,
            }
    }));
}

#[test]
fn rejected_receiving_request_rolls_back_earlier_skill_rows_and_events() {
    let actor = PersonId(1);
    let missing = PersonId(99);
    let (registry, mut host) = setup(Facts {
        people: vec![actor],
        agriculture: vec![],
        workshop: vec![],
        health: vec![],
        physical: vec![],
        social: vec![],
    });
    call(
        &registry,
        &mut host,
        Input::Initialize {
            runtime: runtime(),
            people: vec![actor],
        },
    )
    .unwrap();
    let before = host.storage.snapshot().unwrap();
    let error = call(
        &registry,
        &mut host,
        Input::Receive(LearningEvent::AgreementActivated {
            accepted: vec![actor, missing],
        }),
    );
    assert_eq!(error, Err(Error::Unavailable));
    assert_eq!(host.storage.snapshot().unwrap(), before);
    assert!(host.events().is_empty());
}

#[test]
fn boundary_uses_native_work_evidence_and_the_borrowed_skill_row() {
    let id = PersonId(1);
    let (registry, mut host) = setup(Facts {
        people: vec![id],
        agriculture: vec![],
        workshop: vec![evidence(id)],
        health: vec![],
        physical: vec![],
        social: vec![],
    });
    call(
        &registry,
        &mut host,
        Input::Initialize {
            runtime: runtime(),
            people: vec![id],
        },
    )
    .unwrap();
    assert_eq!(
        call(
            &registry,
            &mut host,
            Input::Boundary {
                from: SimTime(0),
                limit: SimTime(1_000),
            },
        )
        .unwrap(),
        Output::Boundary(SimTime(600))
    );
}
