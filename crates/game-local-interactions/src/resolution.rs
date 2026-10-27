use super::*;
use game_cargo_planning::{StockDemand, StockIntent};
use game_character_choices as choices;
use game_character_clock as clock;
use game_character_state::Minds;
use game_drafting::arbitration::Decision as Admission;
use game_physiology::{Bodies, Body};
use std::collections::BTreeMap;
mod deferred;
use deferred::{Deferred, Guard, settle};

#[derive(Clone, Serialize, Deserialize)]
struct Current {
    meal: Meal,
    request: Pending,
    body: Body,
    patient_body: Body,
    patient_serial: Option<u64>,
    plan: Option<game_provisioning::Plan>,
    failures: u64,
}
struct Selected {
    current: Current,
    review: choices::ReviewCursor,
    decision: game_utility::Decision,
    proposal: game_choice::Proposal,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct ActiveWave {
    #[serde(skip)]
    rows: BTreeMap<PersonId, Current>,
}
impl Resource for ActiveWave {
    const KEY: &'static str = "characters.local-interaction-phase";
    fn validate(&self) -> Result<()> {
        require(self.rows.is_empty(), "local interaction phase left open")
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Wave(Vec<Current>);
impl Request for Wave {
    const KEY: &'static str = "characters.local-interaction-wave";
    type Response = Vec<bool>;
}

fn describe(tx: &mut Transaction<'_>, person: PersonId, request: &Pending) -> Result<Option<Meal>> {
    if let Some(proceed) =
        game_choice::inspect_frame_request::<game_decision_intents::Proceed>(request)
    {
        if proceed.person != person {
            return Ok(None);
        }
        return Ok(
            game_provisioning::local_meal(tx, person, &proceed.plan)?.map(|m| Meal {
                caregiver: person,
                patient: person,
                inventory: m.inventory,
                good: m.good,
                quantity: m.quantity,
                observed_need: 0,
            }),
        );
    }
    if tx.registered::<DescribeCare>() {
        tx.call(DescribeCare {
            person,
            request: request.clone(),
        })
    } else {
        Ok(None)
    }
}
fn actor_valid(tx: &Transaction<'_>, row: &Current, paused: bool) -> Result<bool> {
    let person = row.meal.caregiver;
    let state = tx.get::<Minds>()?;
    Ok(state
        .people
        .get(&person)
        .is_some_and(|m| m.plan == row.plan && m.failures == row.failures)
        && (!paused || state.paused_reviews.get(&person) == Some(&false))
        && state.cohort_urgency.get(&person) == Some(&0)
        && state.cohort_urgency.get(&row.meal.patient).copied() == row.patient_serial
        && !choices::controlled(tx, person)?
        && !tx.get::<Actions>()?.0.contains_key(&person)
        && tx
            .get::<Bodies>()?
            .people
            .get(&person)
            .is_some_and(|b| b.alive && b.health > 0))
}
fn current(tx: &mut Transaction<'_>, row: &Current, paused: bool) -> Result<bool> {
    if !actor_valid(tx, row, paused)? {
        return Ok(false);
    }
    let bodies = tx.get::<Bodies>()?;
    if bodies.people.get(&row.meal.caregiver) != Some(&row.body)
        || bodies.people.get(&row.meal.patient) != Some(&row.patient_body)
        || tx.get::<Actions>()?.0.contains_key(&row.meal.patient)
    {
        return Ok(false);
    }
    Ok(describe(tx, row.meal.caregiver, &row.request)? == Some(row.meal.clone()))
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.resource::<ActiveWave>()?;
    b.request::<game_physiology::CheckLocalMeal>(|tx, game_physiology::CheckLocalMeal(r)| {
        if let Some(row) = tx.get::<ActiveWave>()?.rows.get(&r.caregiver).cloned() {
            return Ok(row.meal.patient == r.patient
                && row.meal.inventory == r.inventory
                && row.meal.good == r.good
                && row.meal.quantity == r.quantity
                && current(tx, &row, true)?);
        }
        Ok(true)
    })?;
    b.request::<Wave>(|tx, Wave(mut rows)| {
        require(
            tx.get::<ActiveWave>()?.rows.is_empty(),
            "nested local interaction wave",
        )?;
        require(
            !choices::meal_wave_has_hooks(tx),
            "selection hooks require ordered execution",
        )?;
        for row in &rows {
            require(current(tx, row, true)?, "local interaction invalidated")?;
        }
        let mut watched = Vec::new();
        for row in &mut rows {
            if !tx
                .get::<Minds>()?
                .cohort_urgency
                .contains_key(&row.meal.patient)
            {
                tx.edit::<Minds>()?
                    .cohort_urgency
                    .insert(row.meal.patient, 0);
                watched.push(row.meal.patient);
                row.patient_serial = Some(0);
            }
        }
        tx.edit::<ActiveWave>()?.rows =
            rows.iter().map(|r| (r.meal.caregiver, r.clone())).collect();
        let outcomes = tx.call(game_physiology::ServeLocalMeals {
            meals: rows.iter().map(|r| r.meal.serve()).collect(),
            minimum_parallel: 2,
        })?;
        require(
            outcomes.len() == rows.len(),
            "invalid local interaction results",
        )?;
        for (row, outcome) in rows.iter().zip(&outcomes) {
            let accepted = *outcome == game_physiology::OfferOutcome::Accepted;
            if row.meal.caregiver != row.meal.patient {
                tx.call(CompleteCare {
                    meal: row.meal.clone(),
                    accepted,
                })?;
            } else {
                tx.call(game_knowledge::ObserveStore {
                    person: row.meal.caregiver,
                    inventory: row.meal.inventory,
                })?;
            }
        }
        for row in &rows {
            require(actor_valid(tx, row, true)?, "local interaction interrupted")?;
        }
        for row in &rows {
            require(
                !tx.call(clock::FinishPausedReview(row.meal.caregiver))?,
                "local review interrupted",
            )?;
            if row.meal.caregiver == row.meal.patient {
                tx.edit::<Minds>()?
                    .people
                    .get_mut(&row.meal.caregiver)
                    .unwrap()
                    .plan = None;
            }
        }
        for person in watched {
            tx.edit::<Minds>()?.cohort_urgency.remove(&person);
        }
        tx.edit::<ActiveWave>()?.rows.clear();
        Ok(outcomes
            .into_iter()
            .map(|o| o == game_physiology::OfferOutcome::Accepted)
            .collect())
    })
}
fn finish(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if tx.get::<Minds>()?.people.contains_key(&person) {
        let next = clock::next(tx, person)?;
        tx.edit::<Minds>()?.schedule.set(person, next);
    }
    let now = tx.now();
    let state = tx.edit::<Minds>()?;
    state.clear_wake_at(person, now);
    state.cohort_urgency.remove(&person);
    Ok(())
}
fn fallback(tx: &mut Transaction<'_>, row: Selected, admitted: bool) -> Result<()> {
    let person = row.review.person;
    let valid = current(tx, &row.current, true)?
        && !choices::stale_selection(tx, &row.review, &row.proposal)?;
    let interrupted = tx.call(clock::FinishPausedReview(person))?;
    if !valid || interrupted || !current(tx, &row.current, false)? {
        game_diagnostics::interaction_profile!(count "interaction.fallback.invalidated_review", 1);
        deferred::refresh(tx, person)?;
    } else if admitted {
        game_diagnostics::interaction_profile!(count "interaction.fallback.ordered_execution", 1);
        choices::apply_and_continue(tx, row.review, row.decision, row.proposal, false)?;
    } else {
        // Arbitration denial is not an attempted action and must not teach failure.
        game_diagnostics::interaction_profile!(count "interaction.fallback.arbitration_denial", 1);
        let _ = tx.attempt(game_knowledge::ObserveStore {
            person,
            inventory: row.current.meal.inventory,
        });
        if current(tx, &row.current, false)? {
            choices::continue_review(tx, row.review)?;
        } else {
            deferred::refresh(tx, person)?;
        }
    }
    finish(tx, person)
}
pub fn resolve(tx: &mut Transaction<'_>, people: Vec<PersonId>) -> Result<()> {
    game_diagnostics::interaction_profile!({
        game_diagnostics::count("interaction.resolve.calls", 1);
        game_diagnostics::count("interaction.resolve.people", people.len() as u64);
    });
    let mut selected = Vec::new();
    let mut deferred = BTreeMap::new();
    for &person in &people {
        if !clock::resume_eligible(tx, person)? || !clock::begin_review(tx, person)? {
            continue;
        }
        let now = tx.now();
        tx.edit::<Minds>()?.schedule.waking.insert(person, now);
        require(
            !tx.get::<Minds>()?.cohort_urgency.contains_key(&person),
            "review already active",
        )?;
        tx.edit::<Minds>()?.cohort_urgency.insert(person, 0);
        if tx
            .get::<Minds>()?
            .people
            .get(&person)
            .is_some_and(|m| m.urgent_frame_at == Some(now))
        {
            game_diagnostics::interaction_profile!(count "interaction.collection.urgent_entry", 1);
            tx.call(game_choice::InvalidateFramePerson(person))?;
            tx.edit::<Minds>()?
                .people
                .get_mut(&person)
                .unwrap()
                .urgent_frame_at = None;
        }
        let Some(mut review) = choices::prepare_review(tx, person, false)? else {
            deferred.insert(person, Deferred::Finish(false));
            continue;
        };
        let choice = choices::next_selection(tx, &mut review)?;
        if tx.get::<Minds>()?.cohort_urgency.get(&person) != Some(&0) {
            game_diagnostics::interaction_profile!(count "interaction.collection.interrupted", 1);
            deferred.insert(person, Deferred::Finish(true));
            continue;
        }
        match choice {
            choices::Selected::Done => {
                deferred.insert(person, Deferred::Finish(false));
            }
            choices::Selected::Refresh => {
                deferred.insert(person, Deferred::Finish(true));
            }
            choices::Selected::Attempt(decision, proposal, continues) => {
                let claim = if continues || proposal.continuation.is_some() {
                    None
                } else {
                    describe(tx, person, &proposal.request)?
                };
                if let Some(meal) = claim {
                    game_diagnostics::interaction_profile!({
                        game_diagnostics::interaction_meal(meal.caregiver == meal.patient, None);
                    });
                    let state = tx.get::<Minds>()?;
                    let mind = &state.people[&person];
                    let row = Current {
                        patient_body: tx.get::<Bodies>()?.people[&meal.patient],
                        patient_serial: state.cohort_urgency.get(&meal.patient).copied(),
                        meal,
                        request: proposal.request.clone(),
                        body: tx.get::<Bodies>()?.people[&person],
                        plan: mind.plan.clone(),
                        failures: mind.failures,
                    };
                    tx.call(clock::PauseReview(person))?;
                    selected.push(Selected {
                        current: row,
                        review,
                        decision,
                        proposal,
                    });
                } else {
                    game_diagnostics::interaction_profile!(count "interaction.selection.ordered_or_ineligible", 1);
                    game_diagnostics::interaction_profile!({
                        game_diagnostics::ordered_proposal(&proposal.key, continues);
                    });
                    let guard = Guard::capture(tx, person)?;
                    tx.call(clock::PauseReview(person))?;
                    deferred.insert(
                        person,
                        Deferred::Ordered(Box::new((review, decision, proposal, continues, guard))),
                    );
                }
            }
        }
    }
    let mut ready = Vec::new();
    for row in selected {
        // Other participants may have entered their own reviews after this selection.
        let mut row = row;
        if row.current.patient_serial.is_none()
            && tx
                .get::<Minds>()?
                .cohort_urgency
                .get(&row.current.meal.patient)
                == Some(&0)
        {
            row.current.patient_serial = Some(0);
        }
        if current(tx, &row.current, true)?
            && !choices::stale_selection(tx, &row.review, &row.proposal)?
        {
            ready.push(row);
        } else {
            game_diagnostics::interaction_profile!(count "interaction.candidate.invalidated_before_arbitration", 1);
            deferred.insert(row.review.person, Deferred::Meal(row, true));
        }
    }
    if ready.is_empty() {
        return settle(tx, &people, deferred);
    }
    let urgent = tx.get::<Minds>()?.rules.urgent_hunger;
    let intents: Vec<_> = ready
        .iter()
        .map(|r| StockIntent {
            id: r.review.person.0,
            actor: r.review.person,
            participants: vec![r.current.meal.patient],
            priority: u8::from(
                if r.current.meal.caregiver == r.current.meal.patient {
                    r.current.body.hunger
                } else {
                    r.current.meal.observed_need
                } < urgent,
            ),
            demands: vec![StockDemand {
                inventory: r.current.meal.inventory,
                good: r.current.meal.good,
                quantity: r.current.meal.quantity,
            }],
        })
        .collect();
    let precedence: Vec<_> = ready.iter().map(|r| r.review.person).collect();
    let plan =
        game_cargo_planning::plan_stock_ordered(tx.get()?, tx.compute(), &intents, &precedence)
            .map_err(|_| Error::Rejected("invalid local interaction claims".into()))?;
    let (admitted, denied): (Vec<_>, Vec<_>) = ready.into_iter().partition(|r| {
        plan.resolution.decisions.get(&r.review.person.0) == Some(&Admission::Granted)
    });
    game_diagnostics::interaction_profile!({
        game_diagnostics::count("interaction.admitted.people", admitted.len() as u64);
        for decision in plan.resolution.decisions.values() {
            let key = match decision {
                Admission::Granted => continue,
                Admission::Insufficient(_) => "interaction.denied.stock",
                Admission::ParticipantAlreadyGranted(_) => "interaction.denied.participant",
                Admission::ActorAlreadyGranted => "interaction.denied.actor",
            };
            game_diagnostics::count(key, 1);
        }
    });
    game_diagnostics::interaction_profile!(size "interaction.admitted.batch_size", admitted.len());
    if !admitted.is_empty() {
        game_diagnostics::interaction_profile!(count "interaction.wave.attempts", 1);
        let result = tx.attempt(Wave(admitted.iter().map(|r| r.current.clone()).collect()));
        game_diagnostics::interaction_profile!({
            game_diagnostics::failure(
                &result
                    .as_ref()
                    .map_err(|error| format!("local interaction wave: {error}")),
            );
        });
        if let Ok(outcomes) = result {
            game_diagnostics::interaction_profile!({
                game_diagnostics::count("interaction.wave.successful_attempts", 1);
                for (row, accepted) in admitted.iter().zip(&outcomes) {
                    game_diagnostics::interaction_meal(
                        row.current.meal.caregiver == row.current.meal.patient,
                        Some(*accepted),
                    );
                }
            });
            for (row, accepted) in admitted.into_iter().zip(outcomes) {
                let person = row.review.person;
                if accepted {
                    choices::complete_selection(
                        tx,
                        &row.review,
                        row.decision,
                        row.proposal,
                        false,
                    )?;
                } else {
                    deferred.insert(person, Deferred::Refused(row));
                    continue;
                }
                deferred.insert(person, Deferred::Finish(false));
            }
        } else {
            game_diagnostics::interaction_profile!(count "interaction.wave.failed_attempts", 1);
            for row in admitted {
                deferred.insert(row.review.person, Deferred::Meal(row, true));
            }
        }
    }
    for row in denied {
        deferred.insert(row.review.person, Deferred::Meal(row, false));
    }
    settle(tx, &people, deferred)
}
