//! Reincarnation through actual events and explicit experience costs.
use crate::{Campaign, Invocation, InvocationWarning, Life, Lineage};
use game_economy_person::{Person, travel};
use game_families::Families;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    LifeEnded,
    Successor,
    GuardianMet,
    MentorshipReleased,
    ControlSuspended,
    ControlRestored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at_millis: u64,
    pub person: u64,
    pub other: Option<u64>,
    pub change: Change,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardian {
    pub person: u64,
    pub loyalty: bool,
    pub required_millis: u64,
    pub observed_millis: u64,
    pub student: Option<u64>,
    pub released: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contact_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostClock {
    pub owner: Invocation,
    pub period_millis: u64,
    /// Number of actually applied doses, including recovery.
    pub elapsed_periods: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dose {
    Warning(InvocationWarning),
    Applied {
        physical: Option<(u64, u32, u64)>,
        control_change: Option<Change>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedCampaign {
    /// Lineage opening, closing, and waiting times are elapsed milliseconds.
    pub lineage: Lineage,
    pub guardians: Vec<Guardian>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub costs: Option<CostClock>,
}

impl ElapsedCampaign {
    pub fn from_fresh(mut campaign: Campaign, unit_millis: u64) -> Result<Self, String> {
        if unit_millis == 0 || campaign.guardians.is_some() || !campaign.lineage.events.is_empty() {
            return Err("elapsed campaign import requires a fresh corporeal lineage".into());
        }
        let convert = |value: u64| {
            value
                .checked_mul(unit_millis)
                .ok_or("campaign time overflow")
        };
        campaign.lineage.started = convert(campaign.lineage.started)?;
        campaign.lineage.waiting_since = campaign.lineage.waiting_since.map(convert).transpose()?;
        for life in &mut campaign.lineage.history {
            life.started = convert(life.started)?;
            life.ended = convert(life.ended)?;
        }
        let costs = campaign.lineage.invocation.take().map(|owner| CostClock {
            next_millis: (!owner.active.is_empty()
                || owner.burden > 0
                || owner.control_until.is_some())
            .then_some(unit_millis),
            owner,
            period_millis: unit_millis,
            elapsed_periods: 0,
        });
        let current = campaign.lineage.current;
        let mut guardians = campaign
            .embodied_guardians
            .map_or(Ok(Vec::new()), |state| {
                state
                    .guardians
                    .into_par_iter()
                    .map(|row| {
                        Ok(Guardian {
                            person: row.person,
                            loyalty: true,
                            required_millis: convert(row.mentorship_required)?,
                            observed_millis: convert(row.mentorship)?,
                            student: row.student.or(current),
                            released: row.released,
                            last_contact_millis: None,
                        })
                    })
                    .collect::<Result<Vec<_>, &str>>()
            })?;
        guardians.par_sort_unstable_by_key(|row| row.person);
        Ok(Self {
            lineage: campaign.lineage,
            guardians,
            costs,
        })
    }

    pub fn validate(&self, family: &Families, people: &[Person], now: u64) -> Result<(), String> {
        self.lineage.validate(family, now)?;
        if self.lineage.invocation.is_some()
            || self
                .guardians
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            || self.guardians.par_iter().any(|row| {
                row.required_millis == 0
                    || row
                        .student
                        .is_some_and(|person| Some(person) != self.lineage.current)
                    || row
                        .last_contact_millis
                        .is_some_and(|at| at > now || row.student.is_none())
                    || people
                        .binary_search_by_key(&row.person, |person| person.id)
                        .ok()
                        .is_none_or(|index| {
                            let person = &people[index];
                            person.immortal.is_none()
                                || person.controlled
                                || Some(person.id) == self.lineage.current
                        })
            })
        {
            return Err("invalid elapsed campaign".into());
        }
        if let Some(clock) = &self.costs {
            if clock.period_millis == 0
                || clock
                    .next_millis
                    .is_some_and(|at| at < now || at - now > clock.period_millis)
                || clock.owner.pending.is_some() && clock.next_millis != Some(now)
            {
                return Err("invalid invocation cost clock".into());
            }
            let previous: Vec<_> = self.lineage.history.iter().map(|row| row.person).collect();
            clock
                .owner
                .validate(self.lineage.current, &previous, clock.elapsed_periods)?;
        }
        Ok(())
    }

    pub fn current(&self) -> Option<u64> {
        self.lineage.current
    }

    pub fn controlled(&self) -> Option<u64> {
        self.current().filter(|_| {
            self.costs
                .as_ref()
                .is_none_or(|clock| clock.owner.control_until.is_none())
        })
    }

    pub fn observe_lifecycle(
        &mut self,
        alive: Option<bool>,
        new_person: Option<u64>,
        at: u64,
    ) -> Vec<Event> {
        let old = self.current();
        self.lineage.advance_boundary(alive, new_person, at);
        let current = self.current();
        if old == current {
            return Vec::new();
        }
        if let Some(clock) = &mut self.costs {
            clock.owner.bind(current);
            clock.next_millis = None;
        }
        self.guardians.par_iter_mut().for_each(|guardian| {
            guardian.student = current;
            guardian.observed_millis = 0;
            guardian.last_contact_millis = None;
            guardian.released = false;
        });
        let change = if current.is_some() {
            Change::Successor
        } else {
            Change::LifeEnded
        };
        vec![Event {
            at_millis: at,
            person: current.or(old).expect("changed lineage"),
            other: None,
            change,
        }]
    }

    pub fn past_lives(&self) -> &[Life] {
        &self.lineage.history
    }

    pub fn activate(&mut self, lives: &[u64], now: u64) -> Result<(), String> {
        let clock = self
            .costs
            .as_mut()
            .ok_or("past-life invocation is disabled")?;
        let at = now
            .checked_add(clock.period_millis)
            .ok_or("invocation time overflow")?;
        let history: Vec<_> = self.lineage.history.iter().map(|row| row.person).collect();
        clock
            .owner
            .activate(lives, &history, clock.elapsed_periods)?;
        clock.next_millis.get_or_insert(at);
        Ok(())
    }

    pub fn dismiss(&mut self, now: u64) -> Result<(), String> {
        let clock = self
            .costs
            .as_mut()
            .ok_or("past-life invocation is disabled")?;
        let next = now
            .checked_add(clock.period_millis)
            .ok_or("invocation time overflow")?;
        clock.owner.dismiss(clock.elapsed_periods)?;
        clock.next_millis = (clock.owner.burden > 0).then_some(next);
        Ok(())
    }

    pub fn accept_warning(&mut self, revision: u64) -> Result<(), String> {
        let clock = self
            .costs
            .as_mut()
            .ok_or("past-life invocation is disabled")?;
        clock.owner.accept(revision, clock.elapsed_periods)
    }

    pub fn dose(&mut self, at: u64) -> Result<Option<Dose>, String> {
        let Some(clock) = &mut self.costs else {
            return Ok(None);
        };
        if clock.next_millis != Some(at) {
            return Ok(None);
        }
        let mut next = clock.clone();
        if !next.owner.prepare(next.elapsed_periods)? {
            let warning = next
                .owner
                .pending
                .clone()
                .ok_or("missing invocation warning")?;
            *clock = next;
            return Ok(Some(Dose::Warning(warning)));
        }
        let before = next.owner.control_until.is_some();
        next.owner.apply_at_deadline(next.elapsed_periods)?;
        next.elapsed_periods = next
            .elapsed_periods
            .checked_add(1)
            .ok_or("invocation period overflow")?;
        let after = next.owner.control_until.is_some();
        next.next_millis = if !next.owner.active.is_empty() || next.owner.burden > 0 || after {
            Some(
                at.checked_add(next.period_millis)
                    .ok_or("invocation time overflow")?,
            )
        } else {
            None
        };
        let physical = next
            .owner
            .owner
            .filter(|_| !next.owner.active.is_empty())
            .map(|person| {
                let strain = if next.owner.burden >= next.owner.rules.strain_threshold {
                    next.owner.rules.strain_damage_per_tick
                } else {
                    0
                };
                let lifespan =
                    if next.owner.exposure >= next.owner.rules.permanent_exposure_threshold {
                        next.owner.rules.lifespan_loss_per_tick
                    } else {
                        0
                    };
                (person, strain, lifespan)
            })
            .filter(|&(_, strain, lifespan)| strain != 0 || lifespan != 0);
        let control_change = (before != after).then_some(if after {
            Change::ControlSuspended
        } else {
            Change::ControlRestored
        });
        *clock = next;
        Ok(Some(Dose::Applied {
            physical,
            control_change,
        }))
    }

    /// Guardian location knowledge is confined to this specific innate link.
    pub fn seek_successor(
        &self,
        people: &[Person],
        travel: &travel::Travel,
        busy: &[u64],
    ) -> Vec<travel::Departure> {
        let Some(current) = self.current() else {
            return Vec::new();
        };
        let locate = |id| {
            people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| &people[index])
        };
        let Some(region) = locate(current).and_then(|row| row.region) else {
            return Vec::new();
        };
        let paths = travel::routes(travel, &[region]);
        self.guardians
            .par_iter()
            .filter_map(|guardian| {
                let actor = locate(guardian.person)?;
                let source = actor.region?;
                if !guardian.loyalty
                    || guardian.released
                    || actor.health == 0
                    || source == region
                    || busy.binary_search(&actor.id).is_ok()
                {
                    return None;
                }
                Some(travel::Departure {
                    person: actor.id,
                    destination: paths.get(&(source, region))?.1,
                })
            })
            .collect()
    }

    /// Record actual physical opportunities to observe the successor's development.
    pub fn observe_guardians(&mut self, colleagues: &[u64], at: u64) -> Vec<Event> {
        self.guardians
            .par_iter_mut()
            .filter_map(|guardian| {
                let student = guardian.student?;
                if !guardian.loyalty
                    || guardian.released
                    || colleagues.binary_search(&guardian.person).is_err()
                {
                    guardian.last_contact_millis = None;
                    return None;
                }
                let previous = guardian.last_contact_millis.replace(at);
                guardian.observed_millis = guardian
                    .observed_millis
                    .saturating_add(previous.map_or(0, |last| at - last));
                previous.is_none().then_some(Event {
                    at_millis: at,
                    person: guardian.person,
                    other: Some(student),
                    change: Change::GuardianMet,
                })
            })
            .collect()
    }

    pub fn ask_release(
        &mut self,
        guardian: u64,
        student: u64,
        mature: bool,
        at: u64,
    ) -> Result<Option<Event>, String> {
        let index = self
            .guardians
            .binary_search_by_key(&guardian, |row| row.person)
            .map_err(|_| "unknown guardian")?;
        let guardian = &mut self.guardians[index];
        if guardian.student != Some(student) {
            return Err("the guardian does not recognize this student".into());
        }
        if guardian.released || !mature || guardian.observed_millis < guardian.required_millis {
            return Ok(None);
        }
        guardian.released = true;
        guardian.last_contact_millis = None;
        Ok(Some(Event {
            at_millis: at,
            person: guardian.person,
            other: Some(student),
            change: Change::MentorshipReleased,
        }))
    }
}
