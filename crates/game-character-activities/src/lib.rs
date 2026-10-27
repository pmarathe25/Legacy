//! Character-owned admission to owner-defined activities before broad deliberation.
use game_character_state::Minds;
use game_core::*;
use game_physiology::{Bodies, CheckRestActivities, InterruptRest, RestActivities, RestActivity};
use game_sim::*;
use serde::{Deserialize, Serialize};
mod mechanics;
#[derive(Clone, Serialize, Deserialize)]
pub struct Epoch(pub Vec<PersonId>);
impl Request for Epoch {
    const KEY: &'static str = "characters.activity-epoch";
    type Response = Vec<PersonId>;
}
fn controlled(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<game_lineage::Lineage>()
        .is_ok_and(|s| s.current == Some(person))
        && tx
            .get::<game_invocation::Invocation>()
            .is_ok_and(|s| game_invocation::controlled(s, tx.now()))
}
fn urgent(tx: &Transaction<'_>, person: PersonId) -> bool {
    let minds = tx.get::<Minds>().unwrap();
    minds.cohort_urgency.get(&person).is_some_and(|n| *n > 0)
        || minds
            .people
            .get(&person)
            .is_some_and(|m| m.urgent_frame_at == Some(tx.now()))
}
fn permitted(tx: &Transaction<'_>, person: PersonId, starting: bool) -> bool {
    let state = tx.get::<Minds>().unwrap();
    state.running
        && state
            .people
            .get(&person)
            .is_some_and(|m| !starting || m.plan.is_none())
        && !controlled(tx, person)
        && !urgent(tx, person)
        && game_choice::frame_tuning(tx).activity_epochs
}
fn eligible(tx: &Transaction<'_>, person: PersonId) -> bool {
    let bodies = tx.get::<Bodies>().unwrap();
    let rules = &tx.get::<Minds>().unwrap().rules;
    permitted(tx, person, true)
        && !tx.get::<Actions>().unwrap().0.contains_key(&person)
        && bodies.people.get(&person).is_some_and(|b| {
            b.alive
                && b.health == b.maximum(&bodies.rules)
                && b.health > 0
                && b.hunger < rules.meal_hunger
                && b.fatigue > rules.rest_fatigue
        })
        && game_transport::area(tx.get().unwrap(), person).is_ok()
}
fn cancel_all(tx: &mut Transaction<'_>) -> Result<()> {
    let people: Vec<_> = tx
        .get::<Actions>()?
        .0
        .keys()
        .copied()
        .filter(|p| game_physiology::resting_activity(tx, *p))
        .collect();
    for person in people {
        tx.call(InterruptRest(person))?;
    }
    Ok(())
}
fn interrupt(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if game_physiology::resting_activity(tx, person) {
        tx.call(InterruptRest(person))?;
    }
    Ok(())
}
pub fn register(b: &mut Builder) -> Result<()> {
    mechanics::register(b)?;
    b.request::<CheckRestActivities>(|tx, r| {
        Ok(r.people.iter().all(|p| permitted(tx, *p, r.starting)))
    })?;
    b.subscribe::<game_character_clock::Alert>("characters.activity-alert", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.subscribe::<game_physiology::Damaged>("characters.activity-damage", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.before::<game_lineage::Begin>("characters.activity-control", |tx, r| {
        interrupt(tx, r.person)
    })?;
    b.subscribe::<game_lineage::Successor>("characters.activity-control", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.before::<game_choice::TuneFrames>("characters.activity-toggle", |tx, r| {
        if !r.0.activity_epochs {
            cancel_all(tx)?;
        }
        Ok(())
    })?;
    b.before::<game_choice::ConfigureFrames>("characters.activity-disable", |tx, r| {
        if tx
            .get::<game_choice::Offers>()?
            .frame
            .as_ref()
            .map(|f| f.interval)
            != r.interval
        {
            cancel_all(tx)?;
        }
        Ok(())
    })?;
    b.request::<Epoch>(|tx, r| {
        let r = Epoch(mechanics::filter(tx, r.0)?);
        if !game_choice::frame_tuning(tx).activity_epochs {
            return Ok(r.0);
        }
        require(
            r.0.windows(2).all(|p| p[0] < p[1]),
            "unordered activity cohort",
        )?;
        for &person in &r.0 {
            if game_physiology::resting_activity(tx, person) {
                let body = &tx.get::<Bodies>()?.people[&person];
                if !permitted(tx, person, false)
                    || body.hunger >= tx.get::<Minds>()?.rules.meal_hunger
                    || body.health < body.maximum(&tx.get::<Bodies>()?.rules)
                {
                    tx.call(InterruptRest(person))?;
                }
            }
        }
        if tx.has_before::<game_physiology::Rest>() {
            return Ok(r.0);
        }
        let selected: Vec<_> = r.0.iter().copied().filter(|p| eligible(tx, *p)).collect();
        if selected.is_empty() {
            return Ok(r.0);
        }
        game_diagnostics::interaction_profile!(count "activity.rest.candidates", selected.len() as u64);
        for &person in &selected {
            require(
                !tx.get::<Minds>()?.cohort_urgency.contains_key(&person),
                "activity review already active",
            )?;
            tx.edit::<Minds>()?.cohort_urgency.insert(person, 0);
        }
        let rules = tx.get::<Minds>()?.rules.clone();
        let result = tx.attempt(RestActivities(
            selected
                .iter()
                .map(|&person| RestActivity {
                    person,
                    fatigue_above: rules.rest_fatigue,
                    hunger_below: rules.meal_hunger,
                })
                .collect(),
        ));
        for person in &selected {
            tx.edit::<Minds>()?.cohort_urgency.remove(person);
        }
        if result.is_err() {
            game_diagnostics::interaction_profile!(count "activity.rest.fallback", selected.len() as u64);
            return Ok(r.0);
        }
        for &person in &selected {
            let action = tx.get::<Actions>()?.0[&person].id;
            let now = tx.now();
            let next = game_character_clock::next(tx, person)?;
            let state = tx.edit::<Minds>()?;
            let mind = state.people.get_mut(&person).unwrap();
            mind.chosen = Some("rest".into());
            mind.engagement = Some(action);
            mind.last = None;
            mind.expectation = None;
            mind.routine = None;
            state.schedule.set(person, next);
            state.clear_wake_at(person, now);
        }
        Ok(r.0
            .into_iter()
            .filter(|p| selected.binary_search(p).is_err())
            .collect())
    })
}
