//! Character permissions and urgent interruption of mechanically owned sessions.
use super::*;
use game_mechanics_state::{
    CheckSessions, InterruptSession, Machines, SessionLimits, session_active,
};

fn allowed(tx: &Transaction<'_>, person: PersonId) -> bool {
    let minds = tx.get::<Minds>().unwrap();
    minds.running
        && minds.people.contains_key(&person)
        && !controlled(tx, person)
        && !urgent(tx, person)
        && game_choice::frame_tuning(tx).mechanics_sessions
}
fn interrupt(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if session_active(tx, person) {
        tx.call(InterruptSession(person))?;
    }
    Ok(())
}
fn cancel_all(tx: &mut Transaction<'_>) -> Result<()> {
    let people: Vec<_> = tx
        .get::<Actions>()?
        .0
        .keys()
        .copied()
        .filter(|p| session_active(tx, *p))
        .collect();
    for person in people {
        interrupt(tx, person)?;
    }
    Ok(())
}
pub(super) fn filter(tx: &mut Transaction<'_>, people: Vec<PersonId>) -> Result<Vec<PersonId>> {
    let mut remaining = Vec::new();
    for person in people {
        if !session_active(tx, person) {
            remaining.push(person);
            continue;
        }
        let session = tx.get::<Machines>()?.work[&person]
            .session
            .as_ref()
            .unwrap();
        let body = &tx.get::<Bodies>()?.people[&person];
        if !allowed(tx, person)
            || body.health < session.health
            || body.resting
            || body.hunger >= session.limits.hunger_below
            || body.fatigue >= session.limits.fatigue_below
        {
            interrupt(tx, person)?;
            remaining.push(person);
        } else {
            let next = game_character_clock::next(tx, person)?;
            let now = tx.now();
            let minds = tx.edit::<Minds>()?;
            minds.schedule.set(person, next);
            minds.clear_wake_at(person, now);
            game_diagnostics::interaction_profile!(count "activity.mechanics.review-skipped", 1);
        }
    }
    Ok(remaining)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<CheckSessions>(|tx, r| {
        Ok(r.0.iter().all(|p| allowed(tx, *p)).then(|| SessionLimits {
            hunger_below: tx.get::<Minds>().unwrap().rules.meal_hunger,
            fatigue_below: tx.get::<Minds>().unwrap().rules.rest_fatigue,
        }))
    })?;
    b.subscribe::<game_character_clock::Alert>("characters.mechanics-alert", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.subscribe::<game_physiology::Damaged>("characters.mechanics-damage", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.before::<game_lineage::Begin>("characters.mechanics-control", |tx, r| {
        interrupt(tx, r.person)
    })?;
    b.subscribe::<game_lineage::Successor>("characters.mechanics-control", |tx, e| {
        interrupt(tx, e.person)
    })?;
    b.before::<game_choice::TuneFrames>("characters.mechanics-toggle", |tx, r| {
        if !r.0.mechanics_sessions {
            cancel_all(tx)?;
        }
        Ok(())
    })?;
    b.before::<game_choice::ConfigureFrames>("characters.mechanics-disable", |tx, r| {
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
    })
}
