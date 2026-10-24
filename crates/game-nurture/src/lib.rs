//! Voluntary food care uses observed need, personal concern, and physical delivery.
use game_care_perception::{candidates, concern, local, observed_need};
use game_core::*;
use game_households::{FoodCare, Households, RememberFoodCare, TendFood};
use game_provisioning::Plan;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod delivery;
use delivery::advance;
#[derive(Clone, Serialize, Deserialize)]
pub struct Assist {
    pub caregiver: PersonId,
    pub beneficiary: PersonId,
}
impl Request for Assist {
    const KEY: &'static str = "care.food-assist";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct Review {
    caregiver: PersonId,
}
#[derive(Clone, Serialize, Deserialize)]
struct ForgetInvalid {
    caregiver: PersonId,
    intent: FoodCare,
}
impl Request for ForgetInvalid {
    const KEY: &'static str = "care.forget-invalid-intent";
    type Response = ();
}
impl Request for Review {
    const KEY: &'static str = "care.food-review";
    type Response = game_choice::Outcome;
}
// Choice outcomes retain observed failure evidence while trying another offer.
#[derive(Clone, Serialize, Deserialize)]
struct TryAssist(Assist);
impl Request for TryAssist {
    const KEY: &'static str = "care.food-choice";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FeedingFailed {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub good: GoodId,
    pub quantity: u64,
    pub reason: String,
}
impl Event for FeedingFailed {
    const KEY: &'static str = "care.feeding-failed";
}
fn remember(tx: &mut Transaction<'_>, caregiver: PersonId, intent: Option<FoodCare>) -> Result<()> {
    tx.call(RememberFoodCare { caregiver, intent })
}
fn predicted(tx: &Transaction<'_>, p: PersonId, intent: &FoodCare) -> u64 {
    if game_communication::active(tx)
        && let Some(need) = game_communication::estimated_hunger(tx, p, intent.beneficiary)
    {
        return need;
    }
    let rules = &tx.get::<game_physiology::Bodies>().unwrap().rules;
    let elapsed = tx.now().0.saturating_sub(intent.observed_at.0);
    let foresight = game_vitality::ability(tx, p, "foresight");
    let growth = u128::from(elapsed) * u128::from(rules.hunger_per_tick) / u128::from(rules.tick.0);
    intent.observed_need.saturating_add(
        (growth * u128::from(foresight) / (1000 + u128::from(foresight))).min(u128::from(u64::MAX))
            as u64,
    )
}
fn start(tx: &mut Transaction<'_>, p: PersonId, child: PersonId) -> Result<bool> {
    game_physiology::capable(tx, p)?;
    require(
        game_vitality::can_manipulate(tx, p),
        "cannot provide physical food care",
    )?;
    if local(tx, p, child) {
        game_physiology::capable(tx, child)?;
    }
    let home = if local(tx, p, child) {
        Some(game_transport::area(tx.get()?, p)?)
    } else {
        game_signals::perception(tx, p, child).and_then(|p| p.area)
    };
    let Some(home) = home else {
        return Ok(false);
    };
    let need = observed_need(tx, p, child)?;
    if need == 0 || concern(tx, p, child, need) <= 0 {
        return Ok(false);
    }
    tx.call(game_knowledge::ObserveSurroundings { person: p })?;
    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
    for inventory in packs {
        tx.call(game_knowledge::ObserveStore {
            person: p,
            inventory,
        })?;
    }
    let motive = concern(tx, p, child, need);
    let options = game_provisioning::care_options(tx, p, (need / 40 + 1).min(64))?;
    let chosen = options
        .into_iter()
        .map(|o| {
            let effort = o.travel.0.saturating_add(o.plan.work_duration().0);
            let value = motive + o.value - o.risk - (effort / MINUTE.0).min(1000) as i64 / 2;
            (value, o)
        })
        .filter(|(value, _)| *value > 0)
        .max_by_key(|(value, o)| (*value, std::cmp::Reverse(o.id)));
    let Some((value, option)) = chosen else {
        return Ok(false);
    };
    let action = tx.get::<Actions>()?.0.get(&p);
    let commitment = action.map_or(0, |a| {
        a.until.map_or(30, |until| {
            (until.0.saturating_sub(tx.now().0) / MINUTE.0).min(120) as i64 + 10
        })
    });
    if value <= commitment {
        return Ok(false);
    }
    if action.is_some_and(|a| {
        !matches!(
            a.kind.as_str(),
            "rest" | "craft" | "farm" | "build" | "socialize" | "pastime" | "treat" | "gather"
        )
    }) {
        return Ok(false);
    }
    let has_plan = tx
        .get::<game_characters::Minds>()
        .ok()
        .and_then(|m| m.people.get(&p))
        .is_some_and(|m| m.plan.is_some());
    if has_plan && tx.registered::<game_characters::Abandon>() {
        tx.call(game_characters::Abandon { person: p })?;
    } else if tx.get::<Actions>()?.0.contains_key(&p) {
        tx.cancel_action(p)?;
    }
    let intent = FoodCare {
        beneficiary: child,
        home,
        observed_need: need,
        observed_at: tx.now(),
        plan: option.plan,
        returning: false,
    };
    remember(tx, p, Some(intent.clone()))?;
    advance(tx, p, intent)
}
fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<bool> {
    if game_physiology::capable(tx, p).is_err()
        || game_transport::area(tx.get()?, p).is_err()
        || !game_vitality::can_manipulate(tx, p)
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
    {
        return Ok(false);
    }
    let Some(h) = tx.get::<Households>().ok() else {
        return Ok(false);
    };
    if let Some(intent) = h.food_care.get(&p).cloned() {
        return advance(tx, p, intent);
    }
    if !h.enabled {
        return Ok(false);
    }
    let local_children: Vec<_> = candidates(tx, p)
        .into_iter()
        .filter(|child| !local(tx, p, *child) || game_physiology::capable(tx, *child).is_ok())
        .collect();
    let mut children = vec![];
    for child in local_children {
        let need = observed_need(tx, p, child)?;
        children.push((concern(tx, p, child, need), child));
    }
    children.sort_unstable_by_key(|(value, child)| (std::cmp::Reverse(*value), *child));
    for (_, child) in children {
        if start(tx, p, child)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn prompt(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if tx
        .get::<Households>()
        .is_ok_and(|h| h.food_care.contains_key(&p) || !candidates(tx, p).is_empty())
    {
        if tx
            .get::<game_characters::Minds>()
            .is_ok_and(|m| m.running && m.people.contains_key(&p))
        {
            return tx.notify(game_choice::Changed { person: p });
        }
        tx.schedule(tx.now(), 45, Review { caregiver: p })?;
    }
    Ok(())
}
/// Registers inside the care owner; household commitments use receiver-owned requests.
pub fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_communication::Signalled>("nurture.heard-need", |tx, e| {
        let mut caregivers = game_transport::area(tx.get()?, e.person)
            .ok()
            .and_then(|area| {
                tx.get::<game_social_life::SocialLife>()
                    .ok()?
                    .residents
                    .get(&area)
            })
            .map(|peers| {
                peers
                    .keys()
                    .copied()
                    .filter(|p| *p != e.person)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Rotate local attention without interrupting an entire growing settlement.
        if !caregivers.is_empty() {
            let start = e.person.0.wrapping_add(tx.now().0 / HOUR.0) as usize % caregivers.len();
            caregivers.rotate_left(start);
            caregivers.truncate(32);
        }
        for person in caregivers {
            if local(tx, person, e.person) && game_vitality::ability(tx, person, "hearing") > 0 {
                let changes_only = tx.get::<game_characters::Minds>().is_ok_and(|m| {
                    m.running && m.rules.care_signal_changes_only && m.people.contains_key(&person)
                });
                let priority = tx
                    .get::<game_characters::Minds>()
                    .ok()
                    .filter(|m| {
                        m.running
                            && m.people.contains_key(&person)
                            && m.rules.busy_attention
                            && m.rules.attention.is_some()
                    })
                    .map(|m| m.rules.urgent_hunger);
                use game_care_perception::SignalReaction;
                let reaction = if let Some(threshold) = priority {
                    game_care_perception::notice_prioritized(
                        tx,
                        person,
                        e.person,
                        changes_only,
                        threshold,
                    )
                } else if game_care_perception::notice_signal(tx, person, e.person, changes_only) {
                    SignalReaction::Changed
                } else {
                    SignalReaction::Ignore
                };
                if matches!(reaction, SignalReaction::Urgent) {
                    tx.notify(game_characters::Alert { person })?;
                } else if matches!(reaction, SignalReaction::Changed)
                    || tx
                        .get::<Households>()
                        .is_ok_and(|h| h.food_care.contains_key(&person))
                {
                    prompt(tx, person)?;
                } else {
                    let _span = game_diagnostics::span("care signal filtered");
                }
            }
        }
        Ok(())
    })?;
    game_choice::serial_provider(b, "nurture.options", |tx, e| {
        if !game_choice::collecting(tx, e.person)
            || !game_vitality::can_manipulate(tx, e.person)
            || !tx.get::<Households>().is_ok_and(|h| h.enabled)
        {
            return Ok(());
        }
        let mut intent = tx.get::<Households>()?.food_care.get(&e.person).cloned();
        if intent.as_ref().is_some_and(|i| {
            local(tx, e.person, i.beneficiary)
                && tx
                    .get::<game_physiology::Bodies>()
                    .unwrap()
                    .people
                    .get(&i.beneficiary)
                    .is_some_and(|b| !b.alive)
        }) {
            if game_choice::planning(tx) {
                game_choice::defer(
                    tx,
                    ForgetInvalid {
                        caregiver: e.person,
                        intent: intent.clone().unwrap(),
                    },
                )?;
            } else {
                remember(tx, e.person, None)?;
            }
            intent = None;
        }
        if let Some(intent) = &intent {
            let need = if local(tx, e.person, intent.beneficiary) {
                observed_need(tx, e.person, intent.beneficiary)?
            } else {
                predicted(tx, e.person, intent)
            };
            let desire = concern(tx, e.person, intent.beneficiary, need) / 5;
            game_choice::offer(
                tx,
                e.person,
                format!("care:food:{}", intent.beneficiary.0),
                game_choice::motive(desire, 0, 0, 1, 0),
                Review {
                    caregiver: e.person,
                },
            )?;
        }
        let children = candidates(tx, e.person);
        for child in children {
            if intent.as_ref().is_some_and(|i| i.beneficiary == child) {
                continue;
            }
            let need = observed_need(tx, e.person, child)?;
            if need == 0 {
                continue;
            }
            let desire = concern(tx, e.person, child, need) / 5;
            game_choice::offer(
                tx,
                e.person,
                format!("care:food:{}", child.0),
                game_choice::motive(desire, 0, 0, 1, 0),
                TryAssist(Assist {
                    caregiver: e.person,
                    beneficiary: child,
                }),
            )?;
        }
        Ok(())
    })?;
    b.request::<ForgetInvalid>(|tx, r| {
        if tx.get::<Households>()?.food_care.get(&r.caregiver) == Some(&r.intent) {
            remember(tx, r.caregiver, None)?;
        }
        Ok(())
    })?;
    b.request::<Assist>(|tx, r| start(tx, r.caregiver, r.beneficiary))?;
    b.request::<TryAssist>(|tx, r| {
        start(tx, r.0.caregiver, r.0.beneficiary).map(game_choice::Outcome::from)
    })?;
    b.request::<TendFood>(|tx, r| consider(tx, r.caregiver).map(|_| ()))?;
    b.request::<Review>(|tx, r| consider(tx, r.caregiver).map(game_choice::Outcome::from))?;
    b.subscribe::<ActionFinished>("care.food-review", |tx, e| {
        let retained = tx.get::<game_characters::Minds>().is_ok_and(|m| {
            m.running
                && m.rules
                    .routine
                    .as_ref()
                    .is_some_and(|p| p.preserve_on_care_completion)
                && m.people.get(&e.person).is_some_and(|m| m.routine.is_some())
        });
        // Continue existing deliveries immediately. Real signals and other evidence
        // still use prompt(); characters handles the ordinary completion wake.
        if retained
            && tx
                .get::<Households>()
                .is_ok_and(|h| !h.food_care.contains_key(&e.person))
        {
            return Ok(());
        }
        prompt(tx, e.person)
    })?;
    b.subscribe::<game_physiology::Fed>("care.food-after-self-meal", |tx, e| {
        if e.caregiver == e.patient {
            prompt(tx, e.caregiver)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("care.food-stop", |tx, e| {
        if tx
            .get::<Households>()
            .is_ok_and(|h| h.food_care.contains_key(&e.person))
        {
            remember(tx, e.person, None)?;
        }
        Ok(())
    })
}
