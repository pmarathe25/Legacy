//! Experimental selected-meal arbitration; actions remain receiver-owned.
use super::*;
use game_cargo_planning::{StockDemand, StockIntent};
use game_drafting::arbitration::Decision as Admission;
struct Meal {
    review: choices::ReviewCursor,
    decision: game_utility::Decision,
    proposal: game_choice::Proposal,
    claim: plans::LocalMeal,
    body: game_physiology::Body,
    plan: Option<plans::Plan>,
    failures: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct WaveClaim {
    person: PersonId,
    claim: plans::LocalMeal,
    plan: Option<plans::Plan>,
    failures: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct MealWave(Vec<WaveClaim>);
impl Request for MealWave {
    const KEY: &'static str = "characters.selected-meal-wave";
    type Response = ();
}

fn wave_valid(tx: &Transaction<'_>, item: &WaveClaim, paused: bool) -> Result<bool> {
    let state = tx.get::<Minds>()?;
    let Some(mind) = state.people.get(&item.person) else {
        return Ok(false);
    };
    Ok(
        (!paused || state.paused_reviews.get(&item.person) == Some(&false))
            && state.cohort_urgency.get(&item.person) == Some(&0)
            && mind.plan == item.plan
            && mind.failures == item.failures
            && tx
                .get::<game_physiology::Bodies>()?
                .people
                .get(&item.person)
                .is_some_and(|b| b.alive && b.health > 0)
            && !tx.get::<Actions>()?.0.contains_key(&item.person)
            && !choices::controlled(tx, item.person)?,
    )
}

pub(super) fn register(b: &mut Builder) -> Result<()> {
    game_local_interactions::register(b)?;
    b.request::<MealWave>(|tx, r| {
        require(
            !choices::meal_wave_has_hooks(tx),
            "meal request hooks require ordered execution",
        )?;
        let meals =
            r.0.iter()
                .map(|item| game_physiology::ConsumeEdible {
                    person: item.person,
                    inventory: item.claim.inventory,
                    good: item.claim.good,
                    quantity: item.claim.quantity,
                })
                .collect();
        tx.call(game_physiology::TryConsumeSelfMeals(
            game_physiology::ConsumeSelfMeals {
                meals,
                // Measured body-row work is faster serially up to the 128-person cap.
                minimum_parallel: 129,
            },
        ))?;
        for item in &r.0 {
            tx.call(game_knowledge::ObserveStore {
                person: item.person,
                inventory: item.claim.inventory,
            })?;
        }
        for item in &r.0 {
            require(wave_valid(tx, item, true)?, "meal wave invalidated")?;
            require(
                !tx.call(scheduling::FinishPausedReview(item.person))?,
                "meal wave interrupted",
            )?;
        }
        for item in &r.0 {
            require(wave_valid(tx, item, false)?, "meal wave invalidated")?;
        }
        for item in r.0 {
            tx.edit::<Minds>()?
                .people
                .get_mut(&item.person)
                .unwrap()
                .plan = None;
        }
        Ok(())
    })
}

fn finish(tx: &mut Transaction<'_>, person: PersonId, now: SimTime) -> Result<()> {
    if tx.get::<Minds>()?.people.contains_key(&person) {
        let next = scheduling::next(tx, person)?;
        tx.edit::<Minds>()?.schedule.set(person, next);
    }
    let state = tx.edit::<Minds>()?;
    state.clear_wake_at(person, now);
    state.cohort_urgency.remove(&person);
    Ok(())
}

fn selected_meal(
    tx: &Transaction<'_>,
    person: PersonId,
    proposal: &game_choice::Proposal,
) -> Result<Option<plans::LocalMeal>> {
    if proposal.continuation.is_some() || tx.get::<Actions>()?.0.contains_key(&person) {
        return Ok(None);
    }
    let Some(request) =
        game_choice::inspect_frame_request::<game_decision_intents::Proceed>(&proposal.request)
    else {
        return Ok(None);
    };
    if request.person != person {
        return Ok(None);
    }
    plans::local_meal(tx, person, &request.plan)
}

fn replan(tx: &mut Transaction<'_>, person: PersonId, now: SimTime) -> Result<()> {
    tx.call(game_choice::InvalidateFramePerson(person))?;
    if tx.get::<Minds>()?.people.contains_key(&person) {
        choices::reconsider(tx, person, true)?;
    }
    finish(tx, person, now)
}

// Paused decisions are advisory; never reuse one after intervening personal changes.
fn changed(tx: &mut Transaction<'_>, meal: &Meal, paused: bool) -> Result<bool> {
    let person = meal.review.person;
    let Some(mind) = tx.get::<Minds>()?.people.get(&person) else {
        return Ok(true);
    };
    let body = tx.get::<game_physiology::Bodies>()?.people.get(&person);
    if body != Some(&meal.body)
        || mind.plan != meal.plan
        || mind.failures != meal.failures
        || tx.get::<Minds>()?.cohort_urgency.get(&person) != Some(&0)
        || choices::controlled(tx, person)?
        || tx.get::<Actions>()?.0.contains_key(&person)
        || (paused
            && tx
                .get::<Minds>()?
                .paused_reviews
                .get(&person)
                .copied()
                .unwrap_or(true))
    {
        return Ok(true);
    }
    if choices::stale_selection(tx, &meal.review, &meal.proposal)? {
        return Ok(true);
    }
    Ok(selected_meal(tx, person, &meal.proposal)? != Some(meal.claim.clone()))
}

fn wave_candidates_current(tx: &mut Transaction<'_>, meals: &[Meal]) -> Result<bool> {
    meals
        .iter()
        .try_fold(true, |valid, meal| Ok(valid && !changed(tx, meal, true)?))
}

fn resolve_meal(
    tx: &mut Transaction<'_>,
    meal: Meal,
    granted: bool,
    now: SimTime,
    force_refresh: bool,
) -> Result<()> {
    let person = meal.review.person;
    let stale = force_refresh || changed(tx, &meal, true)?;
    let interrupted = tx.call(scheduling::FinishPausedReview(person))?;
    if stale || interrupted || changed(tx, &meal, false)? {
        let _measurement = game_diagnostics::span("meal cohort invalidated");
        return if tx.get::<Actions>()?.0.contains_key(&person) {
            finish(tx, person, now)
        } else {
            replan(tx, person, now)
        };
    }
    let enough = tx
        .get::<game_cargo::Cargo>()?
        .0
        .get(&meal.claim.inventory)
        .is_some_and(|row| row.amount(meal.claim.good) >= meal.claim.quantity);
    if granted && enough {
        let _measurement = game_diagnostics::span("meal cohort admitted execution");
        choices::apply_and_continue(tx, meal.review, meal.decision, meal.proposal, false)?;
    } else {
        let _measurement = game_diagnostics::span("meal cohort final denial");
        // A locally visible shortage updates personal knowledge without recording
        // an action failure. Observation hooks may themselves change the actor.
        let _ = tx.attempt(game_knowledge::ObserveStore {
            person,
            inventory: meal.claim.inventory,
        });
        if changed(tx, &meal, false)? {
            return if tx.get::<Actions>()?.0.contains_key(&person) {
                finish(tx, person, now)
            } else {
                replan(tx, person, now)
            };
        }
        choices::continue_review(tx, meal.review)?;
    }
    finish(tx, person, now)
}

pub(super) fn resolve(tx: &mut Transaction<'_>, people: Vec<PersonId>) -> Result<()> {
    if game_choice::frame_tuning(tx).local_interactions {
        return game_local_interactions::resolve(tx, people);
    }
    let _measurement = game_diagnostics::span("meal cohort resolution");
    let now = tx.now();
    let mut meals = Vec::new();
    for person in people {
        let urgent_pending = tx
            .get::<Minds>()?
            .people
            .get(&person)
            .is_some_and(|mind| mind.urgent_frame_at == Some(now));
        if !scheduling::resume_eligible(tx, person)? || !scheduling::begin_review(tx, person)? {
            continue;
        }
        tx.edit::<Minds>()?.schedule.waking.insert(person, now);
        require(
            !tx.get::<Minds>()?.cohort_urgency.contains_key(&person),
            "cohort review already active",
        )?;
        tx.edit::<Minds>()?.cohort_urgency.insert(person, 0);
        if urgent_pending {
            tx.call(game_choice::InvalidateFramePerson(person))?;
            if let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&person) {
                mind.urgent_frame_at = None;
            }
        }
        let Some(mut review) = choices::prepare_review(tx, person, false)? else {
            if tx
                .get::<Minds>()?
                .cohort_urgency
                .get(&person)
                .copied()
                .unwrap_or(0)
                > 0
            {
                replan(tx, person, now)?;
            } else {
                finish(tx, person, now)?;
            }
            continue;
        };
        let selected = choices::next_selection(tx, &mut review)?;
        if tx
            .get::<Minds>()?
            .cohort_urgency
            .get(&person)
            .copied()
            .unwrap_or(0)
            > 0
        {
            replan(tx, person, now)?;
            continue;
        }
        match selected {
            choices::Selected::Done => finish(tx, person, now)?,
            choices::Selected::Refresh => {
                choices::reconsider(tx, person, true)?;
                finish(tx, person, now)?;
            }
            choices::Selected::Attempt(decision, proposal, continues) => {
                if !continues && let Some(claim) = selected_meal(tx, person, &proposal)? {
                    let body = tx.get::<game_physiology::Bodies>()?.people[&person];
                    let mind = &tx.get::<Minds>()?.people[&person];
                    let (plan, failures) = (mind.plan.clone(), mind.failures);
                    tx.call(scheduling::PauseReview(person))?;
                    drop(game_diagnostics::span("meal cohort selected"));
                    meals.push(Meal {
                        review,
                        decision,
                        proposal,
                        claim,
                        body,
                        plan,
                        failures,
                    });
                } else {
                    choices::apply_and_continue(tx, review, decision, proposal, continues)?;
                    finish(tx, person, now)?;
                }
            }
        }
    }
    // A failed or invalidated winner may leave stock for denied actors. Replan
    // those claims against the current stock until no more claims are admitted.
    while !meals.is_empty() {
        let mut candidates = Vec::new();
        for meal in meals {
            if changed(tx, &meal, true)? {
                resolve_meal(tx, meal, false, now, true)?;
            } else {
                candidates.push(meal);
            }
        }
        if candidates.is_empty() {
            break;
        }
        let urgent = tx.get::<Minds>()?.rules.urgent_hunger;
        let intents: Vec<_> = candidates
            .iter()
            .map(|meal| StockIntent {
                id: meal.review.person.0,
                actor: meal.review.person,
                participants: vec![],
                priority: u8::from(meal.body.hunger < urgent),
                demands: vec![StockDemand {
                    inventory: meal.claim.inventory,
                    good: meal.claim.good,
                    quantity: meal.claim.quantity,
                }],
            })
            .collect();
        let precedence: Vec<_> = candidates.iter().map(|meal| meal.review.person).collect();
        let planning = game_diagnostics::span("meal cohort arbitration");
        let plan = game_cargo_planning::plan_stock_ordered(
            tx.get::<game_cargo::Cargo>()?,
            tx.compute(),
            &intents,
            &precedence,
        )
        .map_err(|_| Error::Rejected("invalid provisional meal claims".into()))?;
        drop(planning);
        let (admitted, denied): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|meal| {
            plan.resolution.decisions.get(&meal.review.person.0) == Some(&Admission::Granted)
        });
        for _ in &denied {
            drop(game_diagnostics::span("meal cohort provisional denial"));
        }
        if admitted.is_empty() {
            for meal in denied {
                resolve_meal(tx, meal, false, now, false)?;
            }
            break;
        }
        if game_choice::frame_tuning(tx).meal_wave
            && admitted.len() > 1
            && !choices::meal_wave_has_hooks(tx)
            && wave_candidates_current(tx, &admitted)?
        {
            let claims = admitted
                .iter()
                .map(|meal| WaveClaim {
                    person: meal.review.person,
                    claim: meal.claim.clone(),
                    plan: meal.plan.clone(),
                    failures: meal.failures,
                })
                .collect();
            if tx.attempt(MealWave(claims)).is_ok() {
                drop(game_diagnostics::span("meal cohort wave committed"));
                for _ in &admitted {
                    drop(game_diagnostics::span("meal cohort wave person"));
                }
                for meal in admitted {
                    let person = meal.review.person;
                    choices::complete_selection(
                        tx,
                        &meal.review,
                        meal.decision,
                        meal.proposal,
                        false,
                    )?;
                    finish(tx, person, now)?;
                }
            } else {
                drop(game_diagnostics::span("meal cohort wave fallback"));
                for meal in admitted {
                    resolve_meal(tx, meal, true, now, false)?;
                }
            }
        } else {
            for meal in admitted {
                resolve_meal(tx, meal, true, now, false)?;
            }
        }
        meals = denied;
    }
    Ok(())
}
