//! Voluntary food care uses observed need, personal concern, and physical delivery.
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
fn concern(tx: &Transaction<'_>, p: PersonId, other: PersonId, need: u64) -> i64 {
    let altruism = game_vitality::disposition(tx, p, "altruism");
    let empathy = game_vitality::disposition(tx, p, "empathy");
    let social = tx.get::<game_relationships::Social>().ok();
    let tie = social.and_then(|s| s.relationships.get(&p)?.get(&other));
    let attachment = tie.map_or(0, |r| {
        i64::from(r.affection) * 4 - i64::from(r.grievance) * 4 + i64::from(r.fear) * 2
    });
    let moral = social
        .and_then(|s| s.values.get(&p)?.get("care"))
        .copied()
        .unwrap_or(0);
    let body = &tx.get::<game_physiology::Bodies>().unwrap().people[&p];
    let compassion = game_vitality::disposition(tx, p, "compassion");
    let need = need.min(1000) as i64;
    let awareness = game_vitality::ability(tx, p, "foresight")
        .min(game_vitality::ability(tx, p, "intelligence"))
        .min(16000) as i64;
    let urgent = tx
        .get::<game_characters::Minds>()
        .map_or(100, |m| m.rules.urgent_hunger.clamp(1, 10000)) as i64;
    // Concern includes anticipated consequences of the perceived need, not remote bodily truth.
    let visible_immaturity = if local(tx, p, other) {
        tx.get::<game_physiology::Bodies>()
            .ok()
            .and_then(|b| b.people.get(&other))
            .map_or(0, |b| {
                1000 - game_vitality::maturation(b.born, tx.now(), "constitution")
            }) as i64
    } else {
        0
    };
    let fragility = visible_immaturity * awareness / (1000 + awareness);
    let severity = (need + need * need * awareness / (urgent * (1000 + awareness)))
        * (1000 + fragility)
        / 1000;
    let perceived = severity * (500 + empathy / 4 + compassion / 4) / 1000;
    perceived * altruism / 200 + attachment + i64::from(moral) * 3
        - body.hunger.min(1000) as i64 * 2
        - body.fatigue.min(1000) as i64
        - 30
}
fn candidates(tx: &Transaction<'_>, p: PersonId) -> Vec<PersonId> {
    let Ok(h) = tx.get::<Households>() else {
        return vec![];
    };
    let mut known = std::collections::BTreeSet::new();
    known.extend(h.beneficiaries_of(p));
    known.extend(h.children.get(&p).into_iter().flatten().copied());
    // A perceived need can motivate help without kinship or a prior commitment.
    if let Ok(area) = game_transport::area(tx.get().unwrap(), p)
        && let Ok(social) = tx.get::<game_social_life::SocialLife>()
        && let Some(peers) = social.residents.get(&area)
    {
        let mut peers: Vec<_> = peers.keys().copied().collect();
        if !peers.is_empty() {
            let encounters = social.people.get(&p).map_or(0, |s| s.encounters);
            let start = p.0.wrapping_add(encounters) as usize % peers.len();
            peers.rotate_left(start);
            known.extend(peers.into_iter().take(32));
        }
    }
    known.remove(&p);
    known
        .into_iter()
        .filter(|other| {
            tx.get::<game_physiology::Bodies>()
                .is_ok_and(|b| b.people.get(other).is_some_and(|b| b.alive))
        })
        .collect()
}
fn local(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> bool {
    game_transport::area(tx.get().unwrap(), p)
        .ok()
        .is_some_and(|area| game_transport::area(tx.get().unwrap(), other).ok() == Some(area))
}
fn observed_need(tx: &mut Transaction<'_>, observer: PersonId, person: PersonId) -> Result<u64> {
    if game_communication::active(tx) {
        if !local(tx, observer, person) {
            return Ok(game_communication::estimated_hunger(tx, observer, person).unwrap_or(0));
        }
        let impression = tx.call(game_communication::Notice { observer, person })?;
        return Ok(if impression.guessed_need.as_deref() == Some("food") {
            impression.estimated_need
        } else {
            game_communication::estimated_hunger(tx, observer, person).unwrap_or(0)
        });
    }
    Ok(tx.get::<game_physiology::Bodies>()?.people[&person].hunger)
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
                let _ = tx.attempt(game_communication::Notice {
                    observer: person,
                    person: e.person,
                });
                prompt(tx, person)?;
            }
        }
        Ok(())
    })?;
    game_choice::provider(b, "nurture.options", |tx, e| {
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
            remember(tx, e.person, None)?;
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
    b.request::<Assist>(|tx, r| start(tx, r.caregiver, r.beneficiary))?;
    b.request::<TryAssist>(|tx, r| {
        start(tx, r.0.caregiver, r.0.beneficiary).map(game_choice::Outcome::from)
    })?;
    b.request::<TendFood>(|tx, r| consider(tx, r.caregiver).map(|_| ()))?;
    b.request::<Review>(|tx, r| consider(tx, r.caregiver).map(game_choice::Outcome::from))?;
    b.subscribe::<ActionFinished>("care.food-review", |tx, e| prompt(tx, e.person))?;
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
