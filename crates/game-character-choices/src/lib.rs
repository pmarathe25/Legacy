//! Ordered character decision review under the characters resource owner.
use game_character_clock as scheduling;
use game_character_state::Minds;
pub use game_character_state::record_failure;
use game_choice::Proposal;
use game_core::*;
use game_decision_intents::Proceed;
use game_physiology::Bodies;
use game_provisioning as plans;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize)]
struct Execute {
    person: PersonId,
    proposal: Proposal,
}
#[derive(Clone, Serialize, Deserialize)]
struct ClearFrameConsiderations;
impl Request for ClearFrameConsiderations {
    const KEY: &'static str = "characters.clear-frame-considerations";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct ResetFrameTiers;
impl Request for ResetFrameTiers {
    const KEY: &'static str = "characters.reset-frame-tiers";
    type Response = ();
}
impl Request for Execute {
    const KEY: &'static str = "characters.execute";
    type Response = game_choice::Outcome;
}
pub fn review(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    reconsider(tx, p, false)
}
pub struct ReviewCursor {
    stamp: Option<game_decision_intents::ReviewStamp>,
    _preparation: game_diagnostics::Span,
    pub person: PersonId,
    pub acting: Option<Action>,
    rules: Arc<game_autonomy_policy::Rules>,
    now: SimTime,
    snapshot: bool,
    pub reused: bool,
    tier_fast: bool,
    full_tier: Option<(u64, u64)>,
    sources: std::collections::BTreeMap<String, String>,
    expected: Vec<game_choice::Candidate>,
    margin: i64,
    selection: game_decision_intents::SelectionCursor,
    attempts: u8,
}
pub enum Selected {
    Done,
    Refresh,
    Attempt(game_utility::Decision, Proposal, bool),
}
pub fn reconsider(tx: &mut Transaction<'_>, p: PersonId, fresh: bool) -> Result<()> {
    let _measurement = game_diagnostics::span("character review");
    let Some(review) = prepare_review(tx, p, fresh)? else {
        return Ok(());
    };
    continue_review(tx, review)
}
pub fn continue_review(tx: &mut Transaction<'_>, mut review: ReviewCursor) -> Result<()> {
    loop {
        match next_selection(tx, &mut review)? {
            Selected::Done => return finish_full_tier(tx, &review),
            Selected::Refresh => return refresh_review(tx, review),
            Selected::Attempt(decision, proposal, continues) => {
                if apply_selection(tx, &review, decision, proposal, continues)? {
                    return finish_full_tier(tx, &review);
                }
                if review.reused {
                    return refresh_review(tx, review);
                }
            }
        }
    }
}
pub fn apply_and_continue(
    tx: &mut Transaction<'_>,
    review: ReviewCursor,
    decision: game_utility::Decision,
    proposal: Proposal,
    continues: bool,
) -> Result<()> {
    if apply_selection(tx, &review, decision, proposal, continues)? {
        return finish_full_tier(tx, &review);
    }
    if review.reused {
        game_diagnostics::interaction_profile!(count "interaction.review.replan_owner_rejection", 1);
        return refresh_review(tx, review);
    }
    game_diagnostics::interaction_profile!(count "interaction.review.retained_owner_rejection", 1);
    continue_review(tx, review)
}
fn refresh_review(tx: &mut Transaction<'_>, review: ReviewCursor) -> Result<()> {
    let attempts = review.attempts + 1;
    if attempts >= 8 {
        if review.stamp.is_some() {
            scheduling::defer_urgent(tx, review.person, tx.now())?;
        }
        return finish_full_tier(tx, &review);
    }
    let Some(mut refreshed) = prepare_review(tx, review.person, true)? else {
        return Ok(());
    };
    refreshed.attempts = attempts;
    continue_review(tx, refreshed)
}
fn finish_full_tier(tx: &mut Transaction<'_>, review: &ReviewCursor) -> Result<()> {
    game_character_state::complete_full_tier(tx, review.person, review.now, review.full_tier)
}
/// Recheck a selected provider after other actors have changed the world.
pub fn stale_selection(
    tx: &mut Transaction<'_>,
    review: &ReviewCursor,
    proposal: &Proposal,
) -> Result<bool> {
    if game_choice::frame_tuning(tx).local_interactions
        && !tx.has_before::<game_choice::FrameRecheck>()
    {
        return Ok(false);
    }
    game_choice::frame_stale_provider(
        tx,
        review.person,
        proposal,
        review.sources.get(&proposal.key).cloned(),
    )
}
/// Player control takes precedence over autonomous decision execution.
pub fn controlled(tx: &Transaction<'_>, person: PersonId) -> Result<bool> {
    Ok(tx.get::<game_lineage::Lineage>()?.current == Some(person)
        && game_invocation::controlled(tx.get()?, tx.now()))
}
pub fn prepare_review(
    tx: &mut Transaction<'_>,
    p: PersonId,
    fresh: bool,
) -> Result<Option<ReviewCursor>> {
    let body = &tx.get::<Bodies>()?.people[&p];
    if !body.alive || body.health == 0 || controlled(tx, p)? {
        return Ok(None);
    }
    let acting = tx.get::<Actions>()?.0.get(&p).cloned();
    if acting.as_ref().is_some_and(|a| a.kind == "combat") {
        return Ok(None);
    }
    if let Some(mut routine) = tx
        .get::<Minds>()?
        .people
        .get(&p)
        .and_then(|m| m.routine.clone())
    {
        let rules = tx.get::<Minds>()?.rules.clone();
        if let Some(action) = routine.resume(tx, p, rules.urgent_hunger, rules.rest_fatigue)? {
            let mind = tx.edit::<Minds>()?.people.get_mut(&p).unwrap();
            mind.engagement = Some(action);
            if mind.routine.is_some() {
                mind.routine = Some(routine);
            }
            return Ok(None);
        }
        tx.edit::<Minds>()?.people.get_mut(&p).unwrap().routine = None;
    }
    let now = tx.now();
    if tx.get::<Minds>()?.people[&p].review_clock.is_some() {
        tx.edit::<Minds>()?
            .people
            .get_mut(&p)
            .unwrap()
            .review_clock
            .as_mut()
            .unwrap()
            .reviewed = Some(now);
    }
    let preparation = game_diagnostics::span("full character review");
    if !game_choice::observed(tx, p) && game_transport::area(tx.get()?, p).is_ok() {
        tx.call(game_knowledge::ObserveSurroundings { person: p })?;
        let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        for inventory in packs {
            tx.call(game_knowledge::ObserveStore {
                person: p,
                inventory,
            })?;
        }
    }
    let interval = game_choice::frame_tuning(tx).full_review_millis;
    let tier_enabled = interval > 0;
    let previous_tier = tier_enabled
        .then(|| tx.get::<Minds>().ok()?.people.get(&p)?.frame_tier)
        .flatten();
    let tier_fast = previous_tier.is_some_and(|tier| tier.fast_at(now));
    let prepared = game_choice::frame_decision::<game_decision_intents::Prepared>(tx, p)
        .filter(|_| !fresh)
        .filter(|d| d.person == p && d.at == tx.now())
        .filter(|d| {
            (!d.reused && (!d.tier_enabled || d.hunger_urgent))
                || tx.get::<Bodies>().is_ok_and(|bodies| {
                    bodies.people.get(&p).is_some_and(|body| {
                        tx.get::<Minds>()
                            .is_ok_and(|minds| body.hunger < minds.rules.urgent_hunger)
                    })
                })
        })
        .filter(|d| {
            tx.get::<Minds>().is_ok_and(|m| {
                m.people.get(&p).is_some_and(|mind| {
                    mind.plan == d.plan
                        && mind.failures == d.failures
                        && (!d.cache_enabled || mind.considered == d.prior_considered)
                        && d.tier_enabled == tier_enabled
                        && (!tier_enabled
                            || mind.frame_tier == d.prior_tier && d.fast_tier == tier_fast)
                })
            })
        });
    let snapshot = prepared.is_some();
    let (scored, reused, sources) = if let Some(prepared) = prepared {
        game_diagnostics::interaction_profile!(count "interaction.review.prepared", 1);
        let urgency = tx.get::<Minds>()?.cohort_urgency.get(&p).copied();
        let hunger_was_urgent = (prepared.cache_enabled || prepared.tier_enabled)
            && tx.get::<Bodies>()?.people[&p].hunger >= tx.get::<Minds>()?.rules.urgent_hunger;
        for effect in prepared.effects {
            tx.invoke(effect)?;
        }
        let Some(mind) = tx.get::<Minds>()?.people.get(&p) else {
            return Ok(None);
        };
        if (prepared.cache_enabled || prepared.tier_enabled)
            && (prepared.cache_enabled && mind.considered != prepared.prior_considered
                || prepared.tier_enabled && mind.frame_tier != prepared.prior_tier
                || tx.get::<Minds>()?.cohort_urgency.get(&p).copied() != urgency
                || !hunger_was_urgent
                    && tx.get::<Bodies>()?.people[&p].hunger
                        >= tx.get::<Minds>()?.rules.urgent_hunger
                || !game_choice::observed(tx, p))
        {
            tx.call(game_choice::InvalidateFramePerson(p))?;
            return prepare_review(tx, p, true);
        }
        let previous = mind.considered.clone();
        if let Some(mut memory) = prepared.remembered {
            if let Some(old) = previous {
                old.cancel(tx);
            }
            memory.arm(tx, p)?;
            tx.edit::<Minds>()?.people.get_mut(&p).unwrap().considered = Some(Arc::new(memory));
        }
        (prepared.scored, prepared.reused, prepared.sources)
    } else {
        game_diagnostics::interaction_profile!(count if fresh { "interaction.review.live_fresh" } else { "interaction.review.live_unprepared" }, 1);
        if game_choice::frame_tuning(tx).nonurgent_millis > 0 {
            drop(game_diagnostics::span("frame nonurgent live fallback"));
            if let Some(old) = tx
                .get::<Minds>()?
                .people
                .get(&p)
                .and_then(|mind| mind.considered.clone())
            {
                old.cancel(tx);
                tx.edit::<Minds>()?.people.get_mut(&p).unwrap().considered = None;
            }
        }
        let state = tx.get::<Minds>()?;
        let rules = state.rules.clone();
        let memory = state.people[&p].considered.clone();
        let game_autonomy_policy::Options {
            offers,
            food: options,
            reused,
            remembered,
        } = if tier_enabled {
            let offers = tx.call(game_choice::CollectScoped {
                person: p,
                fast: tier_fast,
            })?;
            game_autonomy_policy::Options {
                offers,
                food: game_decision_food::options(tx, p, rules.gathering_meals),
                reused: false,
                remembered: None,
            }
        } else {
            game_autonomy_policy::consider(tx, p, &rules, memory.as_deref(), fresh)?
        };
        if let Some(memory) = remembered
            && let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&p)
        {
            mind.considered = Some(memory);
        }
        if !tx.get::<Minds>()?.people.contains_key(&p) {
            return Ok(None);
        }
        (
            game_decision_intents::score(tx, p, offers, options?, acting.as_ref())?,
            reused,
            Default::default(),
        )
    };
    let game_decision_intents::Scored {
        offers,
        expected,
        continuing,
        margin,
    } = scored;
    let rules = tx.get::<Minds>()?.rules.clone();
    let now = tx.now();
    game_contracts::causal::candidates(
        p.0,
        offers.iter().map(|o| {
            (
                o.motive.id,
                [
                    o.motive.need,
                    o.motive.goal,
                    o.motive.value,
                    o.motive.effort,
                    o.motive.risk,
                ],
                o.continuation,
            )
        }),
    );
    Ok(Some(ReviewCursor {
        stamp: game_choice::frame_tuning(tx)
            .local_interactions
            .then(|| {
                game_decision_intents::ReviewStamp::capture(tx, p, acting.as_ref().map(|a| a.id))
            })
            .transpose()?,
        _preparation: preparation,
        person: p,
        acting,
        rules,
        now,
        snapshot,
        reused,
        tier_fast,
        full_tier: (tier_enabled && !tier_fast)
            .then_some((interval, previous_tier.unwrap_or_default().generation)),
        sources,
        expected,
        margin,
        selection: game_decision_intents::SelectionCursor::new(offers, continuing, margin),
        attempts: 0,
    }))
}
// Yield only after the stale check, immediately before the original owner action.
pub fn next_selection(tx: &mut Transaction<'_>, review: &mut ReviewCursor) -> Result<Selected> {
    loop {
        if let Some(stamp) = &review.stamp {
            use game_decision_intents::ReviewStatus::*;
            match stamp.status(tx, review.person)? {
                Interrupted => return Ok(Selected::Refresh),
                Occupied | Unavailable => return Ok(Selected::Done),
                Current if controlled(tx, review.person)? => return Ok(Selected::Done),
                Current => (),
            }
        }
        if review.attempts >= 8 {
            return Ok(Selected::Done);
        }
        review.attempts += 1;
        let ongoing = tx.get::<Actions>()?.0.get(&review.person);
        let margin = if review.snapshot {
            review.margin
        } else {
            game_vitality::switching_margin(tx, review.person, review.rules.switching_margin)
        };
        review.selection.set_margin(margin);
        let Some((decision, proposal)) = review.selection.next(
            ongoing.map(|action| action.id),
            review.acting.as_ref().map(|action| action.id),
        ) else {
            return Ok(Selected::Done);
        };
        if review.reused && proposal.key == "wait" {
            return Ok(Selected::Refresh);
        }
        let continues = ongoing.is_some_and(|a| proposal.continuation == Some(a.id));
        if review.tier_fast && proposal.key == "wait" && ongoing.is_some() {
            return Ok(Selected::Done);
        }
        if !continues && stale_selection(tx, review, &proposal)? {
            tx.edit::<Minds>()?
                .people
                .get_mut(&review.person)
                .unwrap()
                .frame_retry = None;
            continue;
        }
        return Ok(Selected::Attempt(decision, proposal, continues));
    }
}
pub fn apply_selection(
    tx: &mut Transaction<'_>,
    review: &ReviewCursor,
    decision: game_utility::Decision,
    proposal: Proposal,
    continues: bool,
) -> Result<bool> {
    if continues
        || tx
            .attempt(Execute {
                person: review.person,
                proposal: proposal.clone(),
            })
            .is_ok_and(|outcome| outcome == game_choice::Outcome::Performed)
    {
        complete_selection(tx, review, decision, proposal, continues)?;
        return Ok(true);
    }
    record_failure(tx, review.person, decision.id)?;
    Ok(false)
}
/// Finish a selection after its owner has successfully committed the action.
pub fn complete_selection(
    tx: &mut Transaction<'_>,
    review: &ReviewCursor,
    mut decision: game_utility::Decision,
    proposal: Proposal,
    continues: bool,
) -> Result<()> {
    let engagement = tx.get::<Actions>()?.0.get(&review.person).map(|a| a.id);
    let routine = game_autonomy_policy::Routine::capture(
        tx,
        review.person,
        &proposal,
        review.rules.routine.as_ref(),
    )?;
    let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&review.person) else {
        return Ok(());
    };
    mind.retry_after.retain(|_, at| *at > review.now);
    mind.retry_after.remove(&decision.id);
    if !continues {
        mind.expectation = review
            .expected
            .iter()
            .find(|c| c.id == decision.id)
            .copied();
        if let Some(memory) = &mut mind.considered {
            Arc::make_mut(memory).consume(&proposal.key);
        }
    }
    decision.reasons = vec![proposal.key.clone()];
    mind.last = Some(decision);
    mind.chosen = Some(proposal.key);
    mind.engagement = engagement;
    mind.routine = routine.map(Box::new);
    Ok(())
}
/// Unknown hooks require the original request chain.
pub fn meal_wave_has_hooks(tx: &Transaction<'_>) -> bool {
    tx.has_before::<Execute>()
        || tx.has_before::<game_choice::Run>()
        || tx.has_before::<Proceed>()
        || tx.has_before::<scheduling::FinishPausedReview>()
        || tx.has_before::<game_choice::FrameRecheck>()
        || tx.has_before::<game_physiology::TryConsumeSelfMeals>()
        || tx.has_before::<game_physiology::CheckSelfMeal>()
        || plans::meal_wave_has_hooks(tx)
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<game_physiology::CheckSelfMeal>(|tx, r| {
        let state = tx.get::<Minds>()?;
        if !state.paused_reviews.contains_key(&r.person) {
            return Ok(true);
        }
        if state.paused_reviews.get(&r.person) != Some(&false)
            || state.cohort_urgency.get(&r.person) != Some(&0)
            || controlled(tx, r.person)?
        {
            return Ok(false);
        }
        let plan = plans::Plan::Meal {
            inventory: r.inventory,
            good: Some(r.good),
        };
        Ok(
            plans::local_meal(tx, r.person, &plan)?.is_some_and(|claim| {
                claim.inventory == r.inventory
                    && claim.good == r.good
                    && claim.quantity == r.quantity
            }),
        )
    })?;
    b.request::<ClearFrameConsiderations>(|tx, _| {
        let memories: Vec<_> = tx
            .get::<Minds>()?
            .people
            .iter()
            .filter_map(|(&person, mind)| mind.considered.clone().map(|memory| (person, memory)))
            .collect();
        for (person, memory) in memories {
            memory.cancel(tx);
            if let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&person) {
                mind.considered = None;
            }
        }
        Ok(())
    })?;
    b.request::<ResetFrameTiers>(|tx, _| {
        let people: Vec<_> = tx.get::<Minds>()?.people.keys().copied().collect();
        for person in people {
            scheduling::wake(tx, person, false, true)?;
            if let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&person) {
                let tier = mind.frame_tier.get_or_insert_default();
                tier.invalidate()?;
                tier.full_due = None;
            }
        }
        Ok(())
    })?;
    b.before::<game_choice::TuneFrames>("characters.clear-frame-considerations", |tx, request| {
        let tuning = game_choice::frame_tuning(tx);
        let old = tuning.nonurgent_millis;
        require(
            request.0.full_review_millis == 0
                || (tx.get::<Minds>()?.rules.option_memory.is_none()
                    && tx.get::<Minds>()?.rules.routine.is_none()),
            "review tiers require ordinary attention",
        )?;
        if tuning.full_review_millis != request.0.full_review_millis {
            tx.call(ResetFrameTiers)?;
        }
        if old > 0 && old != request.0.nonurgent_millis {
            tx.call(ClearFrameConsiderations)?;
        }
        Ok(())
    })?;
    b.before::<game_choice::ConfigureFrames>(
        "characters.clear-frame-considerations",
        |tx, request| {
            let frame = tx.get::<game_choice::Offers>()?.frame.as_ref();
            let clear = frame.is_some_and(|f| {
                f.tuning.nonurgent_millis > 0 && request.interval != Some(f.interval)
            });
            let reset = frame.is_some_and(|f| {
                f.tuning.full_review_millis > 0 && request.interval != Some(f.interval)
            });
            if clear {
                tx.call(ClearFrameConsiderations)?;
            }
            if reset {
                tx.call(ResetFrameTiers)?;
            }
            Ok(())
        },
    )?;
    b.subscribe::<game_choice::Resolved>("characters.outcome", |tx, e| {
        let mind = tx.edit::<Minds>()?.people.get_mut(&e.person);
        let Some(mind) = mind.filter(|m| m.engagement == Some(e.action)) else {
            return Ok(());
        };
        mind.engagement = None;
        let id = mind.chosen.as_deref().map(game_choice::identity);
        if e.outcome == game_choice::Outcome::Reconsider
            && let Some(id) = id
        {
            scheduling::cancel_consideration(tx, e.person)?;
            tx.edit::<Minds>()?
                .people
                .get_mut(&e.person)
                .unwrap()
                .considered = None;
            tx.edit::<Minds>()?
                .people
                .get_mut(&e.person)
                .unwrap()
                .routine = None;
            record_failure(tx, e.person, id)?;
        }
        Ok(())
    })?;
    b.request::<Proceed>(|tx, r| {
        let (performed, next) = tx.call(plans::Advance {
            person: r.person,
            plan: r.plan,
        })?;
        if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&r.person) {
            m.plan = next;
        }
        Ok(performed.into())
    })?;
    b.request::<Execute>(|tx, r| {
        tx.cancel_action(r.person)?;
        if !r.proposal.key.starts_with("food:") && r.proposal.key != "reserve" {
            tx.edit::<Minds>()?.people.get_mut(&r.person).unwrap().plan = None;
        }
        tx.call(game_choice::Run(r.proposal.request))
    })?;
    b.subscribe::<game_choice::Changed>("characters.new-evidence", |tx, e| {
        scheduling::wake(tx, e.person, false, false)
    })
}
