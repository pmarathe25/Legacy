use super::*;
#[derive(Deserialize)]
pub(super) struct Saved {
    enabled: bool,
    nursery: Table<PersonId, AreaId>,
    partners: Table<PersonId, PersonId>,
    children: Table<PersonId, Vec<PersonId>>,
    next: Table<PersonId, SimTime>,
    dependents: Table<PersonId, PersonId>,
    review: Option<ScheduleId>,
    #[serde(default)]
    food_care: Table<PersonId, FoodCare>,
}
pub(super) fn build(
    dependents: &Table<PersonId, PersonId>,
) -> Table<PersonId, Table<PersonId, ()>> {
    let mut wards = Table::<PersonId, Table<PersonId, ()>>::new();
    for (child, carer) in dependents {
        wards.entry(*carer).or_default().insert(*child, ());
    }
    wards
}
impl From<Saved> for Households {
    fn from(s: Saved) -> Self {
        Self {
            wards: build(&s.dependents),
            enabled: s.enabled,
            nursery: s.nursery,
            partners: s.partners,
            children: s.children,
            next: s.next,
            dependents: s.dependents,
            review: s.review,
            food_care: s.food_care,
        }
    }
}
impl Households {
    pub fn dependents_of(&self, person: PersonId) -> impl Iterator<Item = PersonId> + '_ {
        self.wards
            .get(&person)
            .into_iter()
            .flat_map(|s| s.keys().copied())
    }
    pub(super) fn assign(&mut self, child: PersonId, carer: Option<PersonId>) -> Option<PersonId> {
        let old = self.dependents.remove(&child);
        if let Some(person) = old {
            let wards = self.wards.get_mut(&person).expect("caregiver index");
            wards.remove(&child);
            if wards.is_empty() {
                self.wards.remove(&person);
            }
        }
        if let Some(person) = carer {
            self.dependents.insert(child, person);
            self.wards.entry(person).or_default().insert(child, ());
        }
        old
    }
}
