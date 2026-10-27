//! One actor task includes all providers, food queries and final scoring.
use super::*;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prepared {
    pub person: PersonId,
    pub at: SimTime,
    pub scored: Scored,
    pub effects: Vec<Pending>,
    pub sources: BTreeMap<String, String>,
    pub plan: Option<Plan>,
    pub failures: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tier_enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fast_tier: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hunger_urgent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_tier: Option<game_choice::FrameTier>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cache_enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_considered: Option<Arc<game_autonomy_policy::Consideration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remembered: Option<game_autonomy_policy::Consideration>,
}
impl Request for Prepared {
    const KEY: &'static str = "characters.frame-decision";
    type Response = ();
}
fn prepare(tx: &Transaction<'_>, person: PersonId, fanout: bool) -> Option<Pending> {
    let _span = game_diagnostics::span("actor decision preparation");
    let mut scored = None;
    let mut remembered = None;
    let state = tx.get::<Minds>().ok()?;
    let rules = state.rules.clone();
    let tuning = game_choice::frame_tuning(tx);
    let tier_enabled = tuning.full_review_millis > 0;
    let prior_tier = tier_enabled
        .then(|| state.people.get(&person)?.frame_tier)
        .flatten();
    let fast_tier = prior_tier.is_some_and(|tier| tier.fast_at(tx.now()));
    let window = tuning.nonurgent_millis;
    let cache_enabled = window > 0;
    let prior_considered = cache_enabled
        .then(|| state.people.get(&person)?.considered.clone())
        .flatten();
    let body = tx
        .get::<game_physiology::Bodies>()
        .ok()?
        .people
        .get(&person)?;
    let hunger_urgent = tier_enabled && body.hunger >= rules.urgent_hunger;
    let recalled = (cache_enabled && body.hunger < rules.urgent_hunger)
        .then(|| prior_considered.as_ref()?.recall(tx.now()))
        .flatten();
    let reused = recalled.is_some();
    let draft = tx.draft(
        game_choice::Offers::planning_tier(fanout, fast_tier),
        |child| {
            let offers = if let Some(offers) = &recalled {
                offers.clone()
            } else {
                child.call(game_choice::Collect { person })?
            };
            if cache_enabled && !reused {
                let weights = child
                    .get::<game_traits::Traits>()
                    .ok()
                    .map(|t| t.weights(person));
                remembered = Some(game_autonomy_policy::Consideration::shortlist(
                    child.now(),
                    SimDuration(window),
                    &offers,
                    rules.option_limit,
                    |term| {
                        weights
                            .as_ref()
                            .and_then(|w| w.get(term))
                            .copied()
                            .unwrap_or(100)
                    },
                )?);
            }
            let count = child.get::<Minds>()?.rules.gathering_meals;
            let food = plans::options(child, person, count)?;
            let acting = child.get::<Actions>()?.0.get(&person);
            scored = Some(score(child, person, offers, food, acting)?);
            Ok(())
        },
    );
    if !draft.accepted {
        return None;
    }
    if cache_enabled {
        drop(game_diagnostics::span(if reused {
            "frame nonurgent reused"
        } else {
            "frame nonurgent refreshed"
        }));
    }
    if tier_enabled {
        drop(game_diagnostics::span(if fast_tier {
            "tier fast prepared"
        } else {
            "tier full prepared"
        }));
    }
    let (effects, mut sources) = draft.state.finish_planning();
    if reused {
        sources = prior_considered.as_ref()?.sources().clone();
    } else if let Some(memory) = &mut remembered {
        memory.set_sources(sources.clone());
    }
    Some(Pending::immediate(Prepared {
        person,
        at: tx.now(),
        scored: scored?,
        effects,
        sources,
        plan: tx.get::<Minds>().ok()?.people.get(&person)?.plan.clone(),
        failures: tx.get::<Minds>().ok()?.people.get(&person)?.failures,
        tier_enabled,
        fast_tier,
        hunger_urgent,
        prior_tier,
        cache_enabled,
        reused,
        prior_considered,
        remembered,
    }))
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<Prepared>(|_, _| {
        Err(Error::Rejected("prepared decision is not an action".into()))
    })?;
    b.subscribe::<game_choice::PlanCohort>("characters.prepare-decisions", |tx, e| {
        if e.people.len() < game_choice::frame_tuning(tx).minimum_cohort {
            return Ok(());
        }
        let fanout = e.people.len() < tx.compute().current_num_threads() * 2;
        let rows = tx.compute().install(|| {
            e.people
                .par_iter()
                .map(|&p| (p, prepare(tx, p, fanout)))
                .collect()
        });
        tx.call(game_choice::StoreDecisions(rows))
    })
}
