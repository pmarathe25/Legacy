//! A live invocation lets the player consult records from its chosen lives.
use super::skills::InvokedWork;
use super::*;
use game_economy_campaign::elapsed::ElapsedCampaign;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CorePastLifeView {
    pub person: u64,
    pub known_recipes: Vec<u32>,
    pub food_reports: Vec<game_economy_knowledge::Record>,
    pub route_reports: Vec<game_economy_person::travel::RouteObservation>,
    pub words: Vec<game_language::Word>,
    pub written_forms: Vec<game_language::writing::Association>,
}

impl CoreHost {
    fn invoked_bonus(&self, work: InvokedWork, campaign: &ElapsedCampaign) -> Result<u64, String> {
        let Some(index) = self.skill_rates.as_ref() else {
            return Ok(0);
        };
        let borrowed = campaign
            .costs
            .as_ref()
            .filter(|clock| clock.owner.owner == Some(work.person))
            .into_iter()
            .flat_map(|clock| clock.owner.active.iter())
            .map(|&life| index.rate(life, work.recipe))
            .max()
            .unwrap_or(0);
        work.raw_rate
            .checked_mul(borrowed.saturating_sub(work.own_factor))
            .ok_or_else(|| "recalled skill rate overflow".into())
    }

    pub(super) fn plan_invoked_work(
        &self,
        start: game_production_model::Start,
        raw_rate: u64,
    ) -> Result<Option<InvokedWork>, String> {
        let Some((index, campaign)) = self
            .skill_rates
            .as_ref()
            .zip(self.campaign.as_ref())
            .filter(|(_, campaign)| {
                campaign.controlled() == Some(start.worker) && campaign.costs.is_some()
            })
        else {
            return Ok(None);
        };
        let mut work = InvokedWork {
            job: start.id,
            person: start.worker,
            recipe: start.recipe,
            raw_rate,
            own_factor: index.rate(start.worker, start.recipe),
            bonus_rate: 0,
        };
        work.bonus_rate = self.invoked_bonus(work, campaign)?;
        if work.effective_rate().is_none() {
            return Err("recalled skill rate overflow".into());
        }
        Ok(Some(work))
    }

    /// Retime the incarnation's personal contribution from the current offset.
    pub(super) fn retime_invoked_work(&mut self, campaign: &ElapsedCampaign) -> Result<(), String> {
        let Some(work) = self.skills.as_ref().and_then(|skills| skills.invoked_work) else {
            return Ok(());
        };
        let physical = &self.core.physical;
        let ongoing = physical
            .work()
            .binary_search_by_key(&work.job, |row| row.job.id)
            .ok()
            .map(|i| &physical.work()[i])
            .is_some_and(|row| {
                row.job.worker == work.person
                    && row.job.recipe == work.recipe
                    && row.job.remaining > 0
                    && row.phase == game_lot_runtime::Phase::Working
            });
        if !ongoing {
            self.skills
                .as_mut()
                .expect("invoked work owner")
                .invoked_work = None;
            return Ok(());
        }
        let bonus = self.invoked_bonus(work, campaign)?;
        if bonus == work.bonus_rate {
            return Ok(());
        }
        let replace = |rate: u64| {
            rate.checked_sub(work.bonus_rate)
                .and_then(|base| base.checked_add(bonus))
                .ok_or("invalid recalled work rate change")
        };
        let amendment = |rates: &[(u64, u64)]| {
            rates
                .binary_search_by_key(&work.job, |row| row.0)
                .ok()
                .map(|position| replace(rates[position].1).map(|rate| (position, rate)))
                .transpose()
        };
        let paused = amendment(&self.paused_work_rates)?;
        let carried = self
            .schedule
            .as_ref()
            .map(|schedule| amendment(&schedule.carried_rates))
            .transpose()?
            .flatten();
        let active = physical
            .work_interval()
            .map(|phase| {
                amendment(&phase.rates).map(|change| {
                    change.map(|(position, rate)| {
                        let mut rates = phase.rates.clone();
                        rates[position].1 = rate;
                        rates
                    })
                })
            })
            .transpose()?
            .flatten();
        let shared = self
            .shared_work
            .as_ref()
            .map(|shared| {
                shared
                    .sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, session)| session.event.job == work.job)
                    .map(|(position, session)| {
                        replace(session.base_rate).map(|rate| (position, rate))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        if let Some(rates) = active {
            self.core.physical.replace_interval_rates(&rates)?;
        }
        if let Some((position, rate)) = paused {
            self.paused_work_rates[position].1 = rate;
        }
        if let Some((position, rate)) = carried {
            self.schedule
                .as_mut()
                .expect("carried schedule")
                .carried_rates[position]
                .1 = rate;
        }
        if let Some(changes) = shared {
            let shared = self.shared_work.as_mut().expect("assist sessions");
            for (position, rate) in changes {
                shared.sessions[position].base_rate = rate;
            }
        }
        self.skills
            .as_mut()
            .expect("invoked work owner")
            .invoked_work = Some(InvokedWork {
            bonus_rate: bonus,
            ..work
        });
        self.work_checked_revision = None;
        Ok(())
    }

    pub fn consult_past_life(&self, person: u64, life: u64) -> Result<CorePastLifeView, String> {
        let traveler = self.controlled_person(person)?;
        let owner = self
            .campaign
            .as_ref()
            .filter(|campaign| campaign.controlled() == Some(person))
            .and_then(|campaign| campaign.costs.as_ref())
            .filter(|clock| clock.owner.active.contains(&life))
            .ok_or("consulting a past life requires its active invocation")?;
        if traveler.health == 0 || owner.owner.owner != Some(person) {
            return Err("the current incarnation cannot consult this past life".into());
        }
        let known_recipes = self
            .skills
            .as_ref()
            .and_then(|skills| skills.owner.recipe_knowledge.as_ref())
            .and_then(|knowledge| {
                knowledge
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| knowledge.minds[index].known.clone())
            })
            .unwrap_or_default();
        let food_reports = self
            .knowledge
            .as_ref()
            .and_then(|knowledge| {
                knowledge
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| knowledge.minds[index].records.clone())
            })
            .unwrap_or_default();
        let route_reports = self.core.travel.as_ref().map_or_else(Vec::new, |travel| {
            travel
                .observations
                .par_iter()
                .filter(|row| row.person == life)
                .cloned()
                .collect()
        });
        let language = self
            .social
            .as_ref()
            .and_then(|owner| owner.language.as_ref());
        let words = language
            .and_then(|language| {
                language
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| language.minds[index].words.clone())
            })
            .unwrap_or_default();
        let written_forms = language
            .and_then(|language| language.writing.as_ref())
            .and_then(|forms| {
                forms
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| forms.minds[index].associations.clone())
            })
            .unwrap_or_default();
        Ok(CorePastLifeView {
            person: life,
            known_recipes,
            food_reports,
            route_reports,
            words,
            written_forms,
        })
    }
}
