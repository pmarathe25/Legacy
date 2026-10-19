//! Voluntary food care uses observed need, personal concern, and physical delivery.
use game_core::*;
use game_households::{FoodCare, Households, RememberFoodCare, TendFood};
use game_provisioning::Plan;
use game_sim::*;
use serde::{Deserialize, Serialize};
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
    type Response = ();
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
    let severity = need + need * need * awareness / (urgent * (1000 + awareness));
    let perceived = severity * (500 + empathy / 4 + compassion / 4) / 1000;
    perceived * altruism / 200 + attachment + i64::from(moral) * 3
        - body.hunger.min(1000) as i64 * 2
        - body.fatigue.min(1000) as i64
        - 30
}
fn candidates(h: &Households, p: PersonId) -> impl Iterator<Item = PersonId> + '_ {
    // Primary responsibility does not exclude another parent's voluntary help.
    h.dependents_of(p).chain(
        h.children
            .get(&p)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |child| h.dependents.get(child) != Some(&p)),
    )
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
fn available_pack(tx: &Transaction<'_>, p: PersonId) -> Option<InventoryId> {
    tx.get::<game_cargo::Cargo>().ok()?.carried(p).next()
}
fn advance(tx: &mut Transaction<'_>, p: PersonId, mut intent: FoodCare) -> Result<()> {
    if tx.get::<Actions>()?.0.contains_key(&p) {
        return Ok(());
    }
    if local(tx, p, intent.beneficiary) {
        let body = &tx.get::<game_physiology::Bodies>()?.people[&intent.beneficiary];
        if !body.alive {
            return remember(tx, p, None);
        }
        intent.observed_need = observed_need(tx, p, intent.beneficiary)?;
        intent.observed_at = tx.now();
        intent.home = game_transport::area(tx.get()?, p)?;
    }
    if concern(tx, p, intent.beneficiary, predicted(tx, p, &intent)) <= 0 {
        return remember(tx, p, None);
    }
    if !intent.returning {
        match game_provisioning::acquire(tx, p, &mut intent.plan) {
            Ok(false) => return remember(tx, p, Some(intent)),
            Err(_) => {
                game_provisioning::learn_failure(tx, p, &intent.plan);
                return remember(tx, p, None);
            }
            Ok(true) => {}
        }
        let Some(source) = intent.plan.inventory() else {
            remember(tx, p, None)?;
            return prompt(tx, p);
        };
        if !local(tx, p, intent.beneficiary) {
            let Some(pack) = available_pack(tx, p) else {
                return remember(tx, p, None);
            };
            if pack != source {
                let Some((good, food)) = intended_food(tx, p, &intent.plan, source) else {
                    return remember(tx, p, None);
                };
                if tx
                    .attempt(game_property::Take {
                        actor: p,
                        transfer: game_cargo::Transfer {
                            from: source,
                            to: pack,
                            good,
                            quantity: food
                                .portion
                                .min(tx.get::<game_cargo::Cargo>()?.0[&source].amount(good)),
                        },
                        unauthorized: true,
                    })
                    .is_err()
                {
                    return remember(tx, p, None);
                }
                intent.plan = Plan::Meal {
                    inventory: pack,
                    good: Some(good),
                };
                tx.call(game_knowledge::ObserveStore {
                    person: p,
                    inventory: pack,
                })?;
            }
        }
        intent.returning = true;
    }
    if tx.get::<Actions>()?.0.contains_key(&p) {
        return remember(tx, p, Some(intent));
    }
    let here = game_transport::area(tx.get()?, p)?;
    if here != intent.home {
        let next = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&p)
            .and_then(|facts| game_knowledge::route(facts, here, intent.home).ok())
            .and_then(|route| route.areas.get(1).copied());
        if let Some(destination) = next {
            remember(tx, p, Some(intent))?;
            if tx
                .attempt(game_transport::Move {
                    person: p,
                    destination,
                })
                .is_err()
            {
                remember(tx, p, None)?;
            }
            return Ok(());
        }
        return remember(tx, p, None);
    }
    if !local(tx, p, intent.beneficiary) {
        if tx.registered::<game_search::Seek>()
            && let Some(target) = tx.call(game_search::Seek {
                person: p,
                other: intent.beneficiary,
                last_known: intent.home,
            })?
        {
            intent.home = target;
            return remember(tx, p, Some(intent));
        }
        return remember(tx, p, None);
    }
    let inventory = intent.plan.inventory().unwrap();
    let before = intent.observed_need;
    let mut need = before;
    loop {
        if game_physiology::capable(tx, intent.beneficiary).is_err()
            || need == 0
            || concern(tx, p, intent.beneficiary, need) <= 0
        {
            break;
        }
        let Some((good, food)) = intended_food(tx, p, &intent.plan, inventory) else {
            break;
        };
        let held = tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(good);
        let wanted =
            (u128::from(need) * u128::from(food.portion)).div_ceil(u128::from(food.relief));
        let quantity = wanted.min(u128::from(food.portion)).min(u128::from(held)) as u64;
        if quantity == 0 {
            break;
        }
        if tx
            .attempt(game_physiology::Serve {
                caregiver: p,
                patient: intent.beneficiary,
                inventory,
                good,
                quantity,
            })
            .is_err()
        {
            break;
        }
        let estimated_relief = (u128::from(food.relief) * u128::from(quantity)
            / u128::from(food.portion))
        .min(u128::from(u64::MAX)) as u64;
        need = need.saturating_sub(estimated_relief);
    }
    tx.call(game_knowledge::ObserveStore {
        person: p,
        inventory,
    })?;
    let reconsider = need > 0 && need < before && concern(tx, p, intent.beneficiary, need) > 0;
    remember(tx, p, None)?;
    if reconsider {
        tx.schedule(tx.now().after(MINUTE)?, 45, Review { caregiver: p })?;
    }
    Ok(())
}
fn intended_food(
    tx: &Transaction<'_>,
    p: PersonId,
    plan: &Plan,
    inventory: InventoryId,
) -> Option<(GoodId, game_objects::Food)> {
    let good = tx
        .get::<game_knowledge::Knowledge>()
        .ok()?
        .0
        .get(&p)
        .and_then(|facts| plan.intended_good(facts));
    if let Some(good) = good {
        return tx
            .get::<game_objects::Objects>()
            .ok()?
            .food_rules()
            .get(&good)
            .cloned()
            .map(|food| (good, food));
    }
    game_nutrition_choice(tx, p, inventory)
}

fn game_nutrition_choice(
    tx: &Transaction<'_>,
    p: PersonId,
    inventory: InventoryId,
) -> Option<(GoodId, game_objects::Food)> {
    game_nutrition::choose_meal(
        tx,
        p,
        &tx.get::<game_cargo::Cargo>().ok()?.0.get(&inventory)?.goods,
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
        tx.get::<game_signals::Signals>()
            .ok()
            .and_then(|s| s.perceptions.get(&p)?.get(&child)?.area)
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
    advance(tx, p, intent)?;
    Ok(true)
}
fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if game_physiology::capable(tx, p).is_err()
        || !game_vitality::can_manipulate(tx, p)
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
    {
        return Ok(());
    }
    let Some(h) = tx.get::<Households>().ok() else {
        return Ok(());
    };
    if let Some(intent) = h.food_care.get(&p).cloned() {
        return advance(tx, p, intent);
    }
    if !h.enabled {
        return Ok(());
    }
    let local_children: Vec<_> = candidates(h, p)
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
            break;
        }
    }
    Ok(())
}
fn prompt(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if tx
        .get::<Households>()
        .is_ok_and(|h| h.food_care.contains_key(&p) || candidates(h, p).next().is_some())
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
        let caregivers: Vec<_> = tx
            .get::<Households>()
            .ok()
            .into_iter()
            .flat_map(|h| {
                h.dependents.get(&e.person).copied().into_iter().chain(
                    h.children
                        .iter()
                        .filter(|(_, c)| c.contains(&e.person))
                        .map(|(p, _)| *p),
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
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
        let helpers: Vec<_> = tx
            .get::<game_family_life::Bonds>()
            .ok()
            .into_iter()
            .flat_map(|b| b.caregivers.iter())
            .filter(|(_, r)| r.contains_key(&e.person))
            .map(|(c, _)| *c)
            .collect();
        let children: Vec<_> = candidates(tx.get()?, e.person)
            .chain(helpers)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|c| !local(tx, e.person, *c) || game_physiology::capable(tx, *c).is_ok())
            .take(32)
            .collect();
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
                Assist {
                    caregiver: e.person,
                    beneficiary: child,
                },
            )?;
        }
        Ok(())
    })?;
    b.request::<Assist>(|tx, r| start(tx, r.caregiver, r.beneficiary))?;
    b.request::<TendFood>(|tx, r| consider(tx, r.caregiver))?;
    b.request::<Review>(|tx, r| consider(tx, r.caregiver))?;
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
