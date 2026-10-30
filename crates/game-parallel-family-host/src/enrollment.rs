//! Attach actual newborns to configured decision owners at a committed boundary.
use game_parallel_autonomy::{Actor, Autonomy};
use game_parallel_families::Families;
use game_parallel_knowledge::{Knowledge, Mind};
use game_parallel_social::{Participant, Social};
use rayon::prelude::*;

pub struct Context<'a> {
    pub families: Option<&'a Families>,
    pub social: &'a mut Option<Social>,
    pub knowledge: &'a mut Option<Knowledge>,
    pub autonomy: &'a mut Option<Autonomy>,
    pub tick: u64,
}
impl Context<'_> {
    pub fn enroll(self) {
        let Some(families) = self
            .families
            .filter(|families| families.rules.enroll_births)
        else {
            return;
        };
        let births: Vec<_> = families
            .events
            .par_iter()
            .filter(|event| event.tick == self.tick && event.kind == "birth")
            .filter_map(|event| event.people.first().copied())
            .collect();
        self.enroll_ids(&births, families.rules.adult_age);
    }
    /// Enroll already validated newborn identities without expanding receipts.
    pub fn enroll_ids(self, births: &[u64], adult_age: u64) {
        if births.is_empty() {
            return;
        }
        if let Some(social) = self.social {
            social
                .participants
                .par_extend(births.par_iter().map(|&person| Participant {
                    person,
                    willing: true,
                }));
        }
        if let Some(knowledge) = self.knowledge {
            knowledge
                .minds
                .par_extend(births.par_iter().map(|&person| Mind {
                    person,
                    records: Vec::new(),
                }));
        }
        if let Some(autonomy) = self.autonomy {
            autonomy
                .actors
                .par_extend(births.par_iter().map(|&person| Actor {
                    person,
                    enabled: true,
                    next_review: self.tick.saturating_add(adult_age),
                    travel_caution: 0,
                    hunger_urgency: 0,
                }));
        }
    }
}
