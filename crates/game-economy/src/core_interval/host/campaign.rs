//! Persistent player continuity and independent guardian actions at elapsed time.
use super::*;
use game_economy_campaign::{
    InvocationWarning, Life,
    elapsed::{Change, Dose, Event},
};
use game_economy_family::FamilyEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCampaignView {
    pub current: Option<u64>,
    pub controlled: Option<u64>,
    pub past_lives: Vec<Life>,
    pub warning: Option<InvocationWarning>,
    pub guardians: Vec<game_economy_campaign::elapsed::Guardian>,
}

impl CoreHost {
    /// Release authored player inputs for an unattended simulation and keep the lineage.
    pub fn detach_player_control(&mut self) -> Result<(), String> {
        if let Some(campaign) = &mut self.campaign {
            campaign.become_observer()?;
        }
        self.core
            .people
            .par_iter_mut()
            .for_each(|person| person.controlled = false);
        self.work_checked_revision = None;
        Ok(())
    }

    fn campaign_now(&self) -> Result<u64, String> {
        u64::try_from(self.core.simulated_millis()).map_err(|_| "campaign time overflow".into())
    }

    pub(super) fn next_invocation_deadline(&self) -> Option<u64> {
        let next = self.campaign.as_ref()?.costs.as_ref()?.next_millis?;
        u64::try_from(u128::from(next).saturating_sub(self.core.simulated_millis())).ok()
    }

    pub fn campaign_view(&self) -> Option<CoreCampaignView> {
        let campaign = self.campaign.as_ref()?;
        Some(CoreCampaignView {
            current: campaign.current(),
            controlled: campaign.controlled(),
            past_lives: campaign.past_lives().to_vec(),
            warning: campaign
                .costs
                .as_ref()
                .and_then(|clock| clock.owner.pending.clone()),
            guardians: campaign.guardians.clone(),
        })
    }

    pub(super) fn campaign_lifecycle(
        &mut self,
        receipts: &[FamilyEvent],
    ) -> Result<Vec<Event>, String> {
        if self.campaign.is_none() {
            return Ok(Vec::new());
        }
        let now = self.campaign_now()?;
        let Some(campaign) = &mut self.campaign else {
            return Ok(Vec::new());
        };
        let previous = campaign.current();
        let find = |id| {
            self.core
                .people
                .binary_search_by_key(&id, |person| person.id)
                .ok()
                .map(|i| &self.core.people[i])
        };
        let alive = previous
            .and_then(find)
            .map(|person| person.health > 0 || person.immortal.is_some());
        let newborn = if previous.is_none() {
            receipts
                .par_iter()
                .filter(|event| event.kind == "birth")
                .filter_map(|event| event.people.first().copied())
                .filter(|&id| find(id).is_some_and(|person| person.health > 0))
                .min()
        } else {
            None
        };
        if alive != Some(false) && newborn.is_none() {
            return Ok(Vec::new());
        }
        let changes = campaign.observe_lifecycle(alive, newborn, now);
        if !changes.is_empty() {
            let current = campaign.controlled();
            self.sync_campaign_control(previous, current);
        }
        Ok(changes)
    }

    pub(super) fn campaign_boundary_births(&mut self) -> Result<Vec<Event>, String> {
        if self.campaign.is_none() {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let births: Vec<_> = self.family.as_ref().map_or_else(Vec::new, |family| {
            family
                .events
                .par_iter()
                .filter(|event| event.kind == "birth" && event.tick == tick)
                .cloned()
                .collect()
        });
        self.campaign_lifecycle(&births)
    }

    fn sync_campaign_control(&mut self, previous: Option<u64>, current: Option<u64>) {
        for id in previous.into_iter().chain(current) {
            if let Ok(index) = self
                .core
                .people
                .binary_search_by_key(&id, |person| person.id)
            {
                self.core.people[index].controlled = current == Some(id);
            }
        }
        self.work_checked_revision = None;
    }

    pub(super) fn publish_invocation(&mut self) -> Result<Option<Dose>, String> {
        let Some(owner) = &self.campaign else {
            return Ok(None);
        };
        let now = self.campaign_now()?;
        if owner.costs.as_ref().and_then(|clock| clock.next_millis) != Some(now) {
            return Ok(None);
        }
        let mut next = owner.clone();
        let Some(dose) = next.dose(now)? else {
            return Ok(None);
        };
        if let Dose::Applied {
            physical,
            control_change,
        } = &dose
        {
            if let Some(&(id, damage, loss)) = physical.as_ref() {
                let mut family = if loss == 0 { None } else { self.family.clone() };
                let expired = family
                    .as_mut()
                    .map(|family| {
                        family
                            .reduce_lifespan(id, loss, u128::from(now))
                            .map_err(str::to_owned)
                    })
                    .transpose()?
                    .unwrap_or(false);
                let index = self
                    .core
                    .people
                    .binary_search_by_key(&id, |person| person.id)
                    .map_err(|_| "unknown invocation owner")?;
                let harm = if expired {
                    self.core.people[index].health
                } else {
                    damage
                };
                if harm > 0 {
                    self.apply_personal_harm(game_personal_conflict::Resolution {
                        events: Vec::new(),
                        impacts: vec![game_personal_conflict::Impact {
                            target: id,
                            harm: u64::from(harm),
                        }],
                    })?;
                }
                if let Some(family) = family {
                    self.family = Some(family);
                }
            }
            if control_change.is_some() {
                self.sync_campaign_control(next.current(), next.controlled());
            }
        }
        self.campaign = Some(next);
        Ok(Some(dose))
    }

    pub fn activate_past_lives(&mut self, person: u64, lives: &[u64]) -> Result<(), String> {
        self.controlled_person(person)?;
        let now = self.campaign_now()?;
        let mut next = self
            .campaign
            .as_ref()
            .filter(|owner| owner.controlled() == Some(person))
            .ok_or("this character has no past-life access")?
            .clone();
        next.activate(lives, now)?;
        self.retime_invoked_work(&next)?;
        self.campaign = Some(next);
        Ok(())
    }

    pub fn dismiss_past_lives(&mut self, person: u64) -> Result<(), String> {
        self.controlled_person(person)?;
        let now = self.campaign_now()?;
        let mut next = self
            .campaign
            .as_ref()
            .filter(|owner| owner.controlled() == Some(person))
            .ok_or("this character has no past-life access")?
            .clone();
        next.dismiss(now)?;
        self.retime_invoked_work(&next)?;
        self.campaign = Some(next);
        Ok(())
    }

    pub fn accept_past_life_warning(&mut self, person: u64, revision: u64) -> Result<(), String> {
        self.controlled_person(person)?;
        self.campaign
            .as_mut()
            .filter(|owner| owner.controlled() == Some(person))
            .ok_or("this character has no past-life access")?
            .accept_warning(revision)
    }

    pub(super) fn act_on_guardian_choices(
        &mut self,
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        let Some(campaign) = &self.campaign else {
            return Ok((Vec::new(), Vec::new()));
        };
        if campaign.guardians.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let now = self.campaign_now()?;
        let busy = self.local_walk_exclusions(blocked);
        let outside = self.block_away_from_stock(&busy);
        let free = outside.as_deref().unwrap_or(&busy);
        let student = campaign.current().and_then(|id| {
            self.core
                .people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index])
        });
        let colleagues: Vec<_> = campaign
            .guardians
            .par_iter()
            .filter_map(|guardian| {
                let student = student?;
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&guardian.person, |p| p.id)
                    .ok()?];
                (guardian.loyalty
                    && !guardian.released
                    && person.health > 0
                    && student.health > 0
                    && person.region.is_some()
                    && person.region == student.region
                    && free.binary_search(&person.id).is_err()
                    && self.same_local_endpoint(person.id, student.id))
                .then_some(person.id)
            })
            .collect();
        let departures = self.core.travel.as_ref().map_or_else(Vec::new, |travel| {
            campaign.seek_successor(&self.core.people, travel, free)
        });
        let mut actors = colleagues.clone();
        actors.par_extend(departures.par_iter().map(|request| request.person));
        actors.par_sort_unstable();
        actors.dedup();
        if !departures.is_empty() {
            self.depart(&departures, free)?;
        }
        let contacts = self
            .campaign
            .as_mut()
            .expect("campaign checked")
            .observe_guardians(&colleagues, now);
        Ok((actors, contacts))
    }

    pub fn ask_guardian_release(
        &mut self,
        person: u64,
        guardian: u64,
    ) -> Result<Option<Event>, String> {
        let actor = self.controlled_person(person)?;
        let observer = self
            .core
            .people
            .binary_search_by_key(&guardian, |row| row.id)
            .ok()
            .map(|i| &self.core.people[i])
            .ok_or("unknown guardian")?;
        if actor.health == 0
            || observer.health == 0
            || actor.region.is_none()
            || actor.region != observer.region
            || !self.same_local_endpoint(person, guardian)
        {
            return Err("guardian discussion requires physical contact".into());
        }
        let family = self.family.as_ref().ok_or("mentorship requires family")?;
        let age = family
            .members
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .map(|index| family.age_at(&family.members[index], self.core.simulated_millis()))
            .ok_or("unknown student")?;
        let now = self.campaign_now()?;
        self.campaign
            .as_mut()
            .filter(|owner| owner.current() == Some(person))
            .ok_or("no current campaign student")?
            .ask_release(guardian, person, age >= family.rules.adult_age, now)
    }

    pub(super) fn invocation_control_event(&self, change: Change) -> Option<Event> {
        Some(Event {
            at_millis: self.campaign_now().ok()?,
            person: self.campaign.as_ref()?.current()?,
            other: None,
            change,
        })
    }
}
