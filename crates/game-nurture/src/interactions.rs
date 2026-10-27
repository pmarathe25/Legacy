//! Care-owned descriptors for the experimental local interaction boundary.
use super::*;
use game_local_interactions::{CompleteCare, DescribeCare, Meal};

fn describe(tx: &Transaction<'_>, r: DescribeCare) -> Result<Option<Meal>> {
    let p = r.person;
    if tx.has_before::<TryAssist>()
        || tx.has_before::<Assist>()
        || tx.has_before::<Review>()
        || tx.has_before::<game_physiology::TryServe>()
        || tx.get::<Actions>()?.0.contains_key(&p)
        || game_physiology::capable(tx, p).is_err()
        || !game_vitality::can_manipulate(tx, p)
        || tx
            .get::<game_characters::Minds>()?
            .people
            .get(&p)
            .is_none_or(|m| m.plan.is_some())
    {
        return Ok(None);
    }
    let household = tx.get::<Households>()?;
    let retained = household.food_care.get(&p);
    let patient = if let Some(request) = game_choice::inspect_frame_request::<TryAssist>(&r.request)
    {
        if request.0.caregiver != p || !household.enabled {
            return Ok(None);
        }
        request.0.beneficiary
    } else if let Some(request) = game_choice::inspect_frame_request::<Assist>(&r.request) {
        if request.caregiver != p || !household.enabled {
            return Ok(None);
        }
        request.beneficiary
    } else if let Some(request) = game_choice::inspect_frame_request::<Review>(&r.request) {
        if request.caregiver != p {
            return Ok(None);
        }
        let Some(intent) = retained else {
            return Ok(None);
        };
        intent.beneficiary
    } else {
        return Ok(None);
    };
    if p == patient
        || !local(tx, p, patient)
        || game_physiology::capable(tx, patient).is_err()
        || tx.get::<Actions>()?.0.contains_key(&patient)
    {
        return Ok(None);
    }
    let need = if game_communication::active(tx) {
        game_communication::estimated_hunger(tx, p, patient).unwrap_or(0)
    } else {
        tx.get::<game_physiology::Bodies>()?.people[&patient].hunger
    };
    let motive = concern(tx, p, patient, need);
    if need == 0 || motive <= 0 {
        return Ok(None);
    }
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&p) else {
        return Ok(None);
    };
    let plan = if let Some(intent) = retained.filter(|i| i.beneficiary == patient) {
        intent.plan.clone()
    } else {
        let best = game_provisioning::care_options(tx, p, (need / 40 + 1).min(64))?
            .into_iter()
            .filter(|o| {
                o.plan
                    .intended_good(facts)
                    .and_then(|good| {
                        refusals::known_quantity(tx, facts, &o.plan, good, need).map(|quantity| {
                            !refusals::blocked(tx, p, patient, good, quantity, need)
                        })
                    })
                    .unwrap_or(true)
            })
            .map(|o| {
                let effort = o.travel.0.saturating_add(o.plan.work_duration().0);
                (
                    motive + o.value - o.risk - (effort / MINUTE.0).min(1000) as i64 / 2,
                    o,
                )
            })
            .filter(|(value, _)| *value > 0)
            .max_by_key(|(value, o)| (*value, std::cmp::Reverse(o.id)));
        let Some((_, option)) = best else {
            return Ok(None);
        };
        option.plan
    };
    let Some(local) = game_provisioning::local_meal(tx, p, &plan)? else {
        return Ok(None);
    };
    let Some(quantity) = refusals::known_quantity(tx, facts, &plan, local.good, need) else {
        return Ok(None);
    };
    if refusals::blocked(tx, p, patient, local.good, quantity, need) {
        return Ok(None);
    }
    Ok(Some(Meal {
        caregiver: p,
        patient,
        inventory: local.inventory,
        good: local.good,
        quantity,
        observed_need: need,
    }))
}

pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<DescribeCare>(|tx, r| describe(tx, r))?;
    b.request::<CompleteCare>(|tx, r| {
        let m = r.meal;
        remember(tx, m.caregiver, None)?;
        if !r.accepted {
            if tx.get::<Households>()?.food_refusal_window.is_some() {
                refusals::remember(
                    tx,
                    m.caregiver,
                    m.patient,
                    m.good,
                    m.quantity,
                    m.observed_need,
                )?;
            }
            tx.emit(
                FeedingFailed {
                    caregiver: m.caregiver,
                    patient: m.patient,
                    good: m.good,
                    quantity: m.quantity,
                    reason: Error::Rejected("offered food declined".into()).to_string(),
                },
                vec![m.caregiver, m.patient],
            )?;
        }
        tx.call(game_knowledge::ObserveStore {
            person: m.caregiver,
            inventory: m.inventory,
        })
    })
}
