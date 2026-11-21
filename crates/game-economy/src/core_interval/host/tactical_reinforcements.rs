//! A new participant can choose to intervene in a visible, local engagement.
use super::*;
use game_economy_combat::{elapsed::Event, elapsed_reinforcements::Request};

impl CoreHost {
    fn can_join_tactical(&self, request: Request, busy: &[u64]) -> bool {
        let Some((owner, person)) = self.tactical().zip(
            self.core
                .people
                .binary_search_by_key(&request.person, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index]),
        ) else {
            return false;
        };
        let Some(fight) = owner.engagement(request.beneficiary) else {
            return false;
        };
        person.health > 0
            && person.region == Some(fight.region)
            && busy.binary_search(&person.id).is_err()
            && owner.engagement(person.id).is_none()
            && request.beneficiary != request.opponent
            && request.opponent != person.id
            && fight
                .people
                .binary_search_by_key(&request.opponent, |row| row.person)
                .is_ok()
            && self.regional_stock_reachable(person.id, fight.region)
            && [request.beneficiary, request.opponent]
                .into_iter()
                .all(|participant| {
                    self.held_at(participant).is_none()
                        && self.walking().is_none_or(|walking| {
                            walking.stock_reachable(participant, fight.region)
                        })
                        && self
                            .core
                            .people
                            .binary_search_by_key(&participant, |row| row.id)
                            .ok()
                            .is_some_and(|index| {
                                let fighter = &self.core.people[index];
                                fighter.health > 0 && fighter.region == Some(fight.region)
                            })
                })
    }

    pub fn join_controlled_engagement(
        &mut self,
        person: u64,
        beneficiary: u64,
        opponent: u64,
    ) -> Result<Event, String> {
        self.controlled_person(person)?;
        let busy = self.local_walk_exclusions(&[]);
        let request = Request {
            person,
            beneficiary,
            opponent,
            fight_on_entry: false,
        };
        if !self.can_join_tactical(request, &busy) {
            return Err("the controlled person cannot enter that local engagement".into());
        }
        self.admit_tactical_reinforcements(&[request])?
            .into_iter()
            .next()
            .ok_or("no tactical entry was attempted".into())
    }

    pub(super) fn intervene_autonomously(
        &mut self,
        unavailable: &[u64],
    ) -> Result<Vec<Event>, String> {
        let Some(owner) = self
            .tactical()
            .filter(|owner| !owner.interventions.is_empty() && !owner.engagements.is_empty())
        else {
            return Ok(Vec::new());
        };
        let busy = self.local_walk_exclusions(unavailable);
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let requests: Vec<_> = owner
            .interventions
            .par_iter()
            .filter_map(|row| {
                let index = self
                    .core
                    .people
                    .binary_search_by_key(&row.person, |person| person.id)
                    .ok()?;
                if self.core.people[index].controlled {
                    return None;
                }
                let motivated = row.willing
                    || row
                        .pressure_from
                        .zip(opinions.as_ref())
                        .is_some_and(|(source, index)| {
                            let (fear, obligation) = index.feelings(row.person, source);
                            row.fear_at_least.is_some_and(|threshold| fear >= threshold)
                                || row
                                    .obligation_at_least
                                    .is_some_and(|threshold| obligation >= threshold)
                        });
                let request = Request {
                    person: row.person,
                    beneficiary: row.beneficiary,
                    opponent: row.opponent,
                    fight_on_entry: true,
                };
                (motivated && self.can_join_tactical(request, &busy)).then_some(request)
            })
            .collect();
        self.admit_tactical_reinforcements(&requests)
    }

    fn admit_tactical_reinforcements(
        &mut self,
        requests: &[Request],
    ) -> Result<Vec<Event>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let now = self.core.simulated_millis();
        self.personal_conflict
            .as_mut()
            .and_then(|owner| owner.tactical.as_mut())
            .ok_or("tactical engagements are disabled")?
            .reinforce(requests, now, self.core.seed)
    }
}
