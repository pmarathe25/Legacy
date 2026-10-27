//! Observed voluntary food refusals, owned and saved with household intentions.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoodRefusal {
    pub observed_need: u64,
    pub quantity: u64,
    pub until: SimTime,
    /// A locally observable conflicting action at the time of refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<u64>,
    #[serde(default)]
    pub urgent: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusalGroup {
    pub foods: Table<GoodId, Vec<FoodRefusal>>,
    pub retry: Option<(SimTime, ScheduleId)>,
}
fn remove_where(group: &mut RefusalGroup, remove: impl Fn(&FoodRefusal) -> bool) -> bool {
    let mut changed = false;
    let goods: Vec<_> = group.foods.keys().copied().collect();
    for good in goods {
        let rows = group.foods.get_mut(&good).unwrap();
        let before = rows.len();
        rows.retain(|refusal| !remove(refusal));
        changed |= rows.len() != before;
        if rows.is_empty() {
            group.foods.remove(&good);
        }
    }
    changed
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureFoodRefusals(pub SimDuration);
impl Request for ConfigureFoodRefusals {
    const KEY: &'static str = "households.configure-food-refusals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RememberFoodRefusal {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub good: GoodId,
    pub refusal: FoodRefusal,
}
impl Request for RememberFoodRefusal {
    const KEY: &'static str = "households.remember-food-refusal";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SetRefusalRetry {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub at: SimTime,
    pub job: ScheduleId,
}
impl Request for SetRefusalRetry {
    const KEY: &'static str = "households.set-refusal-retry";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ExpireFoodRefusals {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub at: SimTime,
}
impl Request for ExpireFoodRefusals {
    const KEY: &'static str = "households.expire-food-refusals";
    /// Whether this was the active timer, and the next remaining expiry.
    type Response = (bool, Option<SimTime>);
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ClearObservedFoodAction {
    pub patient: PersonId,
    pub action: u64,
}
impl Request for ClearObservedFoodAction {
    const KEY: &'static str = "households.clear-observed-food-action";
    type Response = Vec<(PersonId, bool)>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ClearNewFoodUrgency {
    pub caregiver: PersonId,
    pub patient: PersonId,
}
impl Request for ClearNewFoodUrgency {
    const KEY: &'static str = "households.clear-new-food-urgency";
    type Response = ();
}

pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<ConfigureFoodRefusals>(|tx, r| {
        let window = (r.0.0 > 0).then_some(r.0);
        if tx.get::<Households>()?.food_refusal_window == window {
            return Ok(());
        }
        let jobs: Vec<_> = tx
            .get::<Households>()?
            .food_refusals
            .values()
            .flat_map(|patients| patients.values())
            .filter_map(|group| group.retry.map(|(_, job)| job))
            .collect();
        for job in jobs {
            tx.unschedule(job);
        }
        let state = tx.edit::<Households>()?;
        state.food_refusal_window = window;
        state.food_refusals = Table::new();
        Ok(())
    })?;
    b.request::<RememberFoodRefusal>(|tx, r| {
        require(
            tx.get::<Households>()?.food_refusal_window.is_some(),
            "food refusal policy disabled",
        )?;
        require(
            r.caregiver != r.patient && r.refusal.until > tx.now() && r.refusal.quantity > 0,
            "invalid food refusal",
        )?;
        let evict = tx
            .get::<Households>()?
            .food_refusals
            .get(&r.caregiver)
            .filter(|patients| patients.len() >= 32 && !patients.contains_key(&r.patient))
            .and_then(|patients| {
                patients
                    .iter()
                    .min_by_key(|(patient, group)| {
                        (
                            group.retry.map(|(at, _)| at).unwrap_or(SimTime(u64::MAX)),
                            **patient,
                        )
                    })
                    .map(|(patient, group)| (*patient, group.retry.map(|(_, job)| job)))
            });
        if let Some((patient, job)) = evict {
            if let Some(job) = job {
                tx.unschedule(job);
            }
            tx.edit::<Households>()?
                .food_refusals
                .get_mut(&r.caregiver)
                .unwrap()
                .remove(&patient);
        }
        let group = tx
            .edit::<Households>()?
            .food_refusals
            .entry(r.caregiver)
            .or_default()
            .entry(r.patient)
            .or_default();
        // Bound each pair's memory; deterministic eviction never changes another pair.
        if group.foods.len() >= 16 && !group.foods.contains_key(&r.good) {
            let oldest = group
                .foods
                .iter()
                .min_by_key(|(good, values)| (values.iter().map(|r| r.until).min(), **good))
                .map(|(good, _)| *good)
                .expect("nonempty refusal group");
            group.foods.remove(&oldest);
        }
        let rows = group.foods.entry(r.good).or_default();
        if let Some(existing) = rows
            .iter_mut()
            .find(|existing| existing.quantity == r.refusal.quantity)
        {
            *existing = r.refusal;
        } else {
            if rows.len() >= 16 {
                let oldest = rows
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, entry)| (entry.until, entry.quantity))
                    .map(|(index, _)| index)
                    .expect("nonempty food refusals");
                rows.remove(oldest);
            }
            rows.push(r.refusal);
            rows.sort_unstable_by_key(|entry| entry.quantity);
        }
        Ok(())
    })?;
    b.request::<SetRefusalRetry>(|tx, r| {
        require(r.at > tx.now(), "invalid refusal retry time")?;
        let group = tx
            .edit::<Households>()?
            .food_refusals
            .get_mut(&r.caregiver)
            .and_then(|patients| patients.get_mut(&r.patient))
            .ok_or_else(|| Error::Missing("food refusal group".into()))?;
        require(
            group.retry.is_none() && !group.foods.is_empty(),
            "refusal retry already scheduled",
        )?;
        group.retry = Some((r.at, r.job));
        Ok(())
    })?;
    b.request::<ExpireFoodRefusals>(|tx, r| {
        if tx
            .get::<Households>()?
            .food_refusals
            .get(&r.caregiver)
            .and_then(|patients| patients.get(&r.patient))
            .and_then(|group| group.retry)
            .is_none_or(|(at, _)| at != r.at || at > tx.now())
        {
            return Ok((false, None));
        }
        let state = tx.edit::<Households>()?;
        let patients = state.food_refusals.get_mut(&r.caregiver).unwrap();
        let group = patients.get_mut(&r.patient).unwrap();
        group.retry = None;
        remove_where(group, |refusal| refusal.until <= r.at);
        let next = group
            .foods
            .values()
            .flatten()
            .map(|refusal| refusal.until)
            .min();
        if next.is_none() {
            patients.remove(&r.patient);
        }
        if patients.is_empty() {
            state.food_refusals.remove(&r.caregiver);
        }
        Ok((true, next))
    })?;
    b.request::<ClearObservedFoodAction>(|tx, r| {
        if r.action == 0 {
            return Ok(vec![]);
        }
        let patient_area = game_transport::area(tx.get()?, r.patient).ok();
        let mut affected = vec![];
        for (caregiver, patients) in &tx.get::<Households>()?.food_refusals {
            let Some(group) = patients.get(&r.patient) else {
                continue;
            };
            if patient_area.is_none()
                || game_transport::area(tx.get()?, *caregiver).ok() != patient_area
            {
                continue;
            }
            let mut matching = group
                .foods
                .values()
                .flatten()
                .filter(|refusal| refusal.action == Some(r.action));
            if let Some(first) = matching.next() {
                affected.push((
                    *caregiver,
                    first.urgent || matching.any(|refusal| refusal.urgent),
                ));
            }
        }
        for (caregiver, _) in &affected {
            let state = tx.edit::<Households>()?;
            let patients = state.food_refusals.get_mut(caregiver).unwrap();
            let group = patients.get_mut(&r.patient).unwrap();
            remove_where(group, |refusal| refusal.action == Some(r.action));
            let job = group
                .foods
                .is_empty()
                .then(|| group.retry.map(|(_, job)| job))
                .flatten();
            if group.foods.is_empty() {
                patients.remove(&r.patient);
            }
            if patients.is_empty() {
                state.food_refusals.remove(caregiver);
            }
            if let Some(job) = job {
                tx.unschedule(job);
            }
        }
        Ok(affected)
    })?;
    b.request::<ClearNewFoodUrgency>(|tx, r| {
        let Some(group) = tx
            .get::<Households>()?
            .food_refusals
            .get(&r.caregiver)
            .and_then(|patients| patients.get(&r.patient))
        else {
            return Ok(());
        };
        if !group
            .foods
            .values()
            .flatten()
            .any(|refusal| !refusal.urgent)
        {
            return Ok(());
        }
        let state = tx.edit::<Households>()?;
        let patients = state.food_refusals.get_mut(&r.caregiver).unwrap();
        let group = patients.get_mut(&r.patient).unwrap();
        remove_where(group, |refusal| !refusal.urgent);
        let job = group
            .foods
            .is_empty()
            .then(|| group.retry.map(|(_, job)| job))
            .flatten();
        if group.foods.is_empty() {
            patients.remove(&r.patient);
        }
        if patients.is_empty() {
            state.food_refusals.remove(&r.caregiver);
        }
        if let Some(job) = job {
            tx.unschedule(job);
        }
        Ok(())
    })?;
    Ok(())
}

pub fn forget_person(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let mut pairs = vec![];
    for (caregiver, patients) in &tx.get::<Households>()?.food_refusals {
        for (patient, group) in patients {
            if *caregiver == person || *patient == person {
                pairs.push((*caregiver, *patient, group.retry.map(|(_, job)| job)));
            }
        }
    }
    for (caregiver, patient, job) in pairs {
        if let Some(job) = job {
            tx.unschedule(job);
        }
        let state = tx.edit::<Households>()?;
        if let Some(patients) = state.food_refusals.get_mut(&caregiver) {
            patients.remove(&patient);
            if patients.is_empty() {
                state.food_refusals.remove(&caregiver);
            }
        }
    }
    Ok(())
}
