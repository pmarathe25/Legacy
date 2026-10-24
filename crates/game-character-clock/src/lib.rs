use game_autonomy_policy::Resume;
use game_character_state::Minds;
pub use game_choice::Next;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod attention;
pub use attention::{ConfigureAttention, ConfigureBusyAttention};
mod execution;
/// Receiver-owned urgent evidence; ordinary chatter continues through choice::Changed.
#[derive(Clone, Serialize, Deserialize)]
pub struct Alert {
    pub person: PersonId,
}
impl Event for Alert {
    const KEY: &'static str = "characters.alert";
}
#[derive(Clone, Serialize, Deserialize)]
struct ResolveFrame {
    people: Vec<PersonId>,
}
impl Request for ResolveFrame {
    const KEY: &'static str = "characters.resolve-frame";
    type Response = ();
}
pub fn register(
    b: &mut Builder,
    review: fn(&mut Transaction<'_>, PersonId) -> Result<()>,
) -> Result<()> {
    attention::register(b)?;
    execution::register(b)?;
    b.subscribe::<Alert>("characters.alert", |tx, e| wake(tx, e.person, false, true))?;
    b.before::<game_choice::TuneFrames>("characters.restore-frame-wakes", |tx, r| {
        if game_choice::frame_tuning(tx).commit_batch > 0 && r.0.commit_batch == 0 {
            restore_wakes(tx)?;
        }
        Ok(())
    })?;
    b.before::<game_choice::ConfigureFrames>("characters.restore-frame-wakes", |tx, r| {
        if tx
            .get::<game_choice::Offers>()?
            .frame
            .as_ref()
            .is_some_and(|f| f.tuning.commit_batch > 0 && Some(f.interval) != r.interval)
        {
            restore_wakes(tx)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_choice::PlanCohort>("characters.resolve-frames", |tx, e| {
        let count = game_choice::frame_tuning(tx).commit_batch;
        if count == 0 {
            return Ok(());
        }
        require(
            count <= 128 && e.people.len() <= 100_000,
            "invalid frame commit group",
        )?;
        require(
            e.people.windows(2).all(|p| p[0] < p[1]),
            "unordered frame cohort",
        )?;
        for people in e.people.chunks(count) {
            tx.schedule(
                tx.now(),
                50,
                ResolveFrame {
                    people: people.to_vec(),
                },
            )?;
        }
        Ok(())
    })?;
    b.request::<ResolveFrame>(|tx, r| {
        for person in r.people {
            tx.call(Resume { person })?;
        }
        Ok(())
    })?;
    b.request::<game_autonomy_policy::Recall>(move |tx, r| {
        let state = tx.get::<Minds>()?;
        if state.running
            && state.schedule.waking.get(&r.person) != Some(&tx.now())
            && r.matches(tx.now(), state.people.get(&r.person))
        {
            review(tx, r.person)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Fed>("characters.fed", |tx, e| {
        wake(tx, e.patient, true, false)
    })?;
    b.subscribe::<game_physiology::Damaged>("characters.hurt", |tx, e| {
        wake(tx, e.person, false, true)
    })?;
    b.request::<Resume>(move |tx, r| {
        if !tx.get::<Minds>()?.running || !tx.get::<Minds>()?.people.contains_key(&r.person) {
            return Ok(());
        }
        review(tx, r.person)?;
        Ok(())
    })
}
fn restore_wakes(tx: &mut Transaction<'_>) -> Result<()> {
    let minds = tx.get::<Minds>()?;
    let wakes: Vec<_> = minds
        .schedule
        .waking
        .iter()
        .filter(|(p, _)| minds.running && minds.people.contains_key(p))
        .map(|(p, at)| (*p, (*at).max(tx.now())))
        .collect();
    for (person, at) in wakes {
        tx.schedule(at, 50, Resume { person })?;
    }
    Ok(())
}
pub fn wake(
    tx: &mut Transaction<'_>,
    person: PersonId,
    hunger_only: bool,
    immediate: bool,
) -> Result<()> {
    if immediate && let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&person) {
        mind.considered = None;
    }
    if tx
        .get::<Minds>()?
        .people
        .get(&person)
        .is_some_and(|m| m.routine.is_some())
    {
        tx.edit::<Minds>()?.people.get_mut(&person).unwrap().routine = None;
    }
    queue(tx, person, hunger_only, immediate, true)
}
pub fn completed(
    tx: &mut Transaction<'_>,
    person: PersonId,
    hunger_only: bool,
    immediate: bool,
) -> Result<()> {
    queue(tx, person, hunger_only, immediate, false)
}
fn queue(
    tx: &mut Transaction<'_>,
    person: PersonId,
    hunger_only: bool,
    immediate: bool,
    evidence: bool,
) -> Result<()> {
    let state = tx.get::<Minds>()?;
    let floor = if evidence {
        state
            .rules
            .attention
            .map(|p| p.evidence)
            .or_else(|| state.rules.review_floor())
    } else {
        state.rules.review_floor()
    };
    let frame = game_choice::frame_deadline(tx);
    let suppressed = !state.running
        || !state.people.contains_key(&person)
        || frame.is_some()
            && state
                .schedule
                .waking
                .get(&person)
                .is_some_and(|at| *at >= tx.now())
        || state.schedule.waking.get(&person) == Some(&tx.now())
            && (floor.is_none() && !hunger_only
                || floor.is_some()
                    && state.people[&person]
                        .review_clock
                        .is_some_and(|c| c.in_progress(tx.now())));
    game_contracts::causal::wake(person.0, tx.now().0, suppressed, hunger_only, immediate);
    if suppressed {
        return Ok(());
    }
    let now = tx.now();
    let clock = floor.map(|floor| {
        let body = &tx.get::<game_physiology::Bodies>().unwrap().people[&person];
        (
            state.people[&person].review_clock.unwrap_or_default(),
            floor,
            immediate
                || state.rules.busy_attention
                    && !tx.get::<Actions>().unwrap().0.contains_key(&person)
                || state.people[&person].routine.is_some()
                || body.hunger >= state.rules.urgent_hunger
                || body.health == 0,
        )
    });
    if let Some(at) = frame {
        game_choice::wake_frame(tx, at)?;
    }
    let state = tx.edit::<Minds>()?;
    state.schedule.set(person, Next::Active);
    state.schedule.waking.insert(person, frame.unwrap_or(now));
    if let Some((mut clock, floor, immediate)) = clock {
        clock.schedule(tx, floor, immediate, Resume { person })?;
        let state = tx.edit::<Minds>()?;
        state.people.get_mut(&person).unwrap().review_clock = Some(clock);
    } else if frame.is_none() || game_choice::frame_tuning(tx).commit_batch == 0 {
        tx.schedule(frame.unwrap_or(now), 50, Resume { person })?;
    }
    Ok(())
}
pub fn begin_review(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    if let Some(start) = game_choice::frame_start(tx) {
        let now = tx.now();
        if start != now || game_choice::frame_action_changed(tx, person) {
            tx.edit::<Minds>()?.clear_wake_at(person, now);
            completed(tx, person, false, false)?;
            return Ok(false);
        }
        return Ok(tx.edit::<Minds>()?.mark_frame_review(person, now, start));
    }
    let state = tx.get::<Minds>()?;
    let Some(floor) = state.rules.review_floor() else {
        return Ok(true);
    };
    let Some(mind) = state.people.get(&person) else {
        return Ok(false);
    };
    let mut clock = mind.review_clock.unwrap_or_default();
    let idle = state.rules.busy_attention && !tx.get::<Actions>()?.0.contains_key(&person);
    if !idle && mind.routine.is_none() && !clock.due(tx.now(), floor) {
        completed(tx, person, false, false)?;
        return Ok(false);
    }
    clock.consume(tx);
    let state = tx.edit::<Minds>()?;
    state.people.get_mut(&person).unwrap().review_clock = Some(clock);
    Ok(true)
}
pub fn next(tx: &mut Transaction<'_>, person: PersonId) -> Result<Next> {
    let state = tx.get::<Minds>()?;
    let next = game_autonomy_policy::next(tx, person, &state.rules, &state.schedule)?;
    let live = tx
        .get::<game_choice::Offers>()?
        .frame
        .as_ref()
        .is_some_and(|f| {
            f.tuning.batch_decisions
                && f.tuning.minimum_cohort > 0
                && f.people.len() < f.tuning.minimum_cohort
        });
    if game_choice::frame_tuning(tx).recover_idle
        && !live
        && !tx.get::<Actions>()?.0.contains_key(&person)
        && state.people[&person].frame_retry != Some(tx.now())
        && let Some(at) = game_choice::frame_deadline(tx).filter(|at| *at > tx.now())
    {
        game_choice::wake_frame(tx, at)?;
        if game_choice::frame_tuning(tx).commit_batch == 0 {
            tx.schedule(at, 50, Resume { person })?;
        }
        let state = tx.edit::<Minds>()?;
        state.people.get_mut(&person).unwrap().frame_retry = Some(at);
        state.schedule.waking.insert(person, at);
        return Ok(Next::Active);
    }
    Ok(next)
}
