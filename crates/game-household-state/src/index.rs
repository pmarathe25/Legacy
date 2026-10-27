use super::*;
#[derive(Deserialize)]
pub(super) struct Saved {
    enabled: bool,
    nursery: Table<PersonId, AreaId>,
    partners: Table<PersonId, PersonId>,
    children: Table<PersonId, Vec<PersonId>>,
    next: Table<PersonId, SimTime>,
    caregivers: Table<PersonId, Table<PersonId, ()>>,
    self_reliant: Table<PersonId, ()>,
    review: Option<ScheduleId>,
    #[serde(default)]
    food_care: Table<PersonId, FoodCare>,
    #[serde(default)]
    food_refusal_window: Option<SimDuration>,
    #[serde(default)]
    food_refusals: Table<PersonId, Table<PersonId, RefusalGroup>>,
}
pub(super) fn build(
    caregivers: &Table<PersonId, Table<PersonId, ()>>,
) -> Table<PersonId, Table<PersonId, ()>> {
    let mut wards = Table::<PersonId, Table<PersonId, ()>>::new();
    for (person, carers) in caregivers {
        for carer in carers.keys() {
            wards.entry(*carer).or_default().insert(*person, ());
        }
    }
    wards
}
impl From<Saved> for Households {
    fn from(s: Saved) -> Self {
        Self {
            wards: build(&s.caregivers),
            enabled: s.enabled,
            nursery: s.nursery,
            partners: s.partners,
            children: s.children,
            next: s.next,
            caregivers: s.caregivers,
            self_reliant: s.self_reliant,
            review: s.review,
            food_care: s.food_care,
            food_refusal_window: s.food_refusal_window,
            food_refusals: s.food_refusals,
        }
    }
}
impl Households {
    /// Beneficiaries of this person's freely chosen, revocable commitments.
    pub fn beneficiaries_of(&self, person: PersonId) -> impl Iterator<Item = PersonId> + '_ {
        self.wards
            .get(&person)
            .into_iter()
            .flat_map(|s| s.keys().copied())
    }
    pub fn carers_of(&self, person: PersonId) -> impl Iterator<Item = PersonId> + '_ {
        self.caregivers
            .get(&person)
            .into_iter()
            .flat_map(|s| s.keys().copied())
    }
    pub fn committed(&self, caregiver: PersonId, beneficiary: PersonId) -> bool {
        self.caregivers
            .get(&beneficiary)
            .is_some_and(|r| r.contains_key(&caregiver))
    }
    pub fn set_care(&mut self, caregiver: PersonId, beneficiary: PersonId, accept: bool) {
        if accept {
            self.caregivers
                .entry(beneficiary)
                .or_default()
                .insert(caregiver, ());
            self.wards
                .entry(caregiver)
                .or_default()
                .insert(beneficiary, ());
        } else {
            if let Some(r) = self.caregivers.get_mut(&beneficiary) {
                r.remove(&caregiver);
                if r.is_empty() {
                    self.caregivers.remove(&beneficiary);
                }
            }
            if let Some(r) = self.wards.get_mut(&caregiver) {
                r.remove(&beneficiary);
                if r.is_empty() {
                    self.wards.remove(&caregiver);
                }
            }
        }
    }
}
