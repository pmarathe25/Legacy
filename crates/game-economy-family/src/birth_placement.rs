use game_economy_person::{
    Person,
    travel::{Journey, Travel},
};
use game_families::Birth;
use rayon::prelude::*;

/// Physical people and journey records prepared against one validated instant.
pub struct BirthPlacement {
    infants: Vec<Person>,
    journeys: Vec<Journey>,
}

impl BirthPlacement {
    pub fn prepare(
        births: &[Birth],
        people: &[Person],
        travel: Option<&Travel>,
        seek_meals: bool,
        work_preference: Option<bool>,
    ) -> Result<Self, &'static str> {
        let infants = births
            .par_iter()
            .map(|birth| Person {
                id: birth.id,
                region: birth.region,
                health: birth.health,
                hunger: 0,
                hunger_remainder: 0,
                starvation_remainder: 0,
                immortal: None,
                willing_to_work: work_preference.unwrap_or_else(|| {
                    people
                        .binary_search_by_key(&birth.mother, |person| person.id)
                        .ok()
                        .is_some_and(|index| people[index].willing_to_work)
                }),
                seek_meals,
                controlled: false,
            })
            .collect();
        let journeys = births
            .par_iter()
            .filter(|birth| birth.region.is_none())
            .map(|birth| {
                let travel = travel.ok_or("traveling newborn has no travel owner")?;
                let index = travel
                    .journeys
                    .binary_search_by_key(&birth.mother, |journey| journey.person)
                    .map_err(|_| "traveling mother has no journey")?;
                let mut journey = travel.journeys[index].clone();
                journey.person = birth.id;
                Ok(journey)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { infants, journeys })
    }

    /// Publish against the people and travel generation used in preparation.
    pub fn publish(self, people: &mut Vec<Person>, travel: &mut Option<Travel>) {
        people.par_extend(self.infants);
        if people.par_windows(2).any(|rows| rows[0].id > rows[1].id) {
            people.par_sort_unstable_by_key(|person| person.id);
        }
        if !self.journeys.is_empty() {
            let travel = travel.as_mut().expect("validated newborn journeys");
            travel.journeys.par_extend(self.journeys);
        }
        if let Some(travel) = travel
            && travel
                .journeys
                .par_windows(2)
                .any(|rows| rows[0].person > rows[1].person)
        {
            travel
                .journeys
                .par_sort_unstable_by_key(|journey| journey.person);
        }
    }
}
