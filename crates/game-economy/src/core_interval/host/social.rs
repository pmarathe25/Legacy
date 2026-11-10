//! Personal meetings, subjective relationships, and communicated observations.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSocial {
    pub owner: game_social::Social,
    #[serde(default)]
    pub last_tick: Option<u64>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreHealth {
    pub owner: game_disease::Disease,
    #[serde(default)]
    pub treatment: Vec<game_economy_treatment::Policy>,
    #[serde(default)]
    pub last_tick: Option<u64>,
}
impl Clone for CoreHealth {
    fn clone(&self) -> Self {
        Self {
            owner: game_disease::Disease {
                strains: self.owner.strains.par_iter().cloned().collect(),
                treatments: self.owner.treatments.par_iter().cloned().collect(),
                cases: self.owner.cases.par_iter().cloned().collect(),
                events: self.owner.events.par_iter().cloned().collect(),
            },
            treatment: self.treatment.par_iter().cloned().collect(),
            last_tick: self.last_tick,
        }
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreDynamicsResults {
    pub meetings: Vec<game_social::Encounter>,
    pub disease_events: Vec<game_disease::Event>,
}

impl CoreHost {
    /// Run at most one personal social generation per physical tick.
    pub fn meet_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<game_social::Encounter>, String> {
        Ok(self.advance_dynamics_autonomously(occupied)?.meetings)
    }
    /// Advance social and disease owners once per tick, sharing actual meetings.
    pub fn advance_dynamics_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<CoreDynamicsResults, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let tick = self.core.physical.tick();
        let social = self
            .social
            .as_ref()
            .filter(|owner| owner.last_tick != Some(tick));
        let health = self
            .health
            .as_ref()
            .filter(|owner| owner.last_tick != Some(tick));
        if tick == 0 || (social.is_none() && health.is_none()) {
            return Ok(CoreDynamicsResults::default());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before social choices".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let opening = game_economy_dynamics::Dynamics {
            social: social.map(|owner| owner.owner.clone()),
            personality: None,
            disease: health.map(|owner| owner.clone().owner),
            treatment: vec![],
            claims: None,
        };
        let mut people: Vec<_> = self.core.people.par_iter().cloned().collect();
        let inquiry_meetings = self
            .taking
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.inquiry_meetings(tick));
        let mut next = opening.prepare(
            &self.core.people,
            &mut people,
            &busy,
            self.opinions.as_ref(),
            (&[], None, &[]),
            game_economy_dynamics::Clock {
                seed: self.core.seed,
                tick,
                travel: None,
                partition: None,
                local: None,
                contact: None,
                speak: None,
                opening: None,
                missions: None,
                inquiry_meetings: &inquiry_meetings,
                personality_override: self.personality.as_ref(),
            },
        )?;
        if let (Some(previous), Some(disease)) = (health, &mut next.disease) {
            disease.events.par_extend(
                previous
                    .owner
                    .events
                    .par_iter()
                    .filter(|event| {
                        event.tick == tick && event.kind == game_disease::EventKind::Treated
                    })
                    .cloned(),
            );
            disease.events.par_sort_unstable_by_key(|event| {
                (
                    event.person,
                    event.strain,
                    match event.kind {
                        game_disease::EventKind::Recovered => 0,
                        game_disease::EventKind::Infected => 1,
                        game_disease::EventKind::Treated => 2,
                    },
                    event.source,
                )
            });
        }
        let meetings: Vec<_> = next
            .social
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.events.par_iter().cloned().collect());
        let disease_events: Vec<_> = next
            .disease
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.events.par_iter().cloned().collect());
        let mut opinions = next.social.as_ref().map(|_| self.opinions.clone());
        if let Some(opinions) = &mut opinions {
            game_economy_dynamics::learn_social(opinions, Some(&next), tick);
        }
        let knowledge = if let (Some(owner), Some(_)) = (&self.knowledge, &next.social) {
            let view = self.core.physical.view(tick)?;
            game_economy_knowledge::advance(
                Some(owner),
                &self.core.people,
                self.core.physical.goods(),
                &view,
                next.social.as_ref(),
                self.opinions.as_ref(),
                game_economy_knowledge::ObservationAccess {
                    now: tick,
                    not_at_stock: &busy,
                },
            )?
        } else {
            None
        };
        let taking = self
            .taking
            .as_ref()
            .filter(|owner| {
                (!owner.report_policies.is_empty() && !meetings.is_empty())
                    || !owner.complaints.is_empty()
            })
            .map(|owner| {
                let mut owner = owner.clone();
                let contacts: Vec<_> = meetings
                    .par_iter()
                    .map(|event| (event.a, event.b))
                    .collect();
                let changes = self.stage_taking_contacts(&mut owner, &contacts, tick);
                if !changes.is_empty() {
                    let current = opinions.get_or_insert_with(|| self.opinions.clone());
                    if let Some(opinions) = current {
                        opinions.apply_personal_changes(&changes, tick);
                    }
                }
                owner
            });
        let teaching = next
            .social
            .as_ref()
            .and_then(|social| self.stage_skill_teaching(social, tick));
        if let Some(frame) = &teaching
            && self
                .opinions
                .as_ref()
                .is_some_and(|owner| owner.tracks_teaching_reactions())
        {
            let current = opinions.get_or_insert_with(|| self.opinions.clone());
            if let Some(opinions) = current {
                crate::teaching::interpret(opinions, &frame.lessons, tick);
            }
        }
        let route_reports = self
            .core
            .travel
            .as_ref()
            .map(|travel| {
                crate::route_sharing::prepare(
                    travel,
                    &self.core.people,
                    self.opinions.as_ref(),
                    &meetings,
                )
            })
            .unwrap_or_default();
        let mut travel = (!route_reports.is_empty())
            .then(|| self.core.travel.clone().expect("reports need travel"));
        if let Some(travel) = &mut travel {
            travel.receive_reports(&route_reports);
        }
        let changed = people
            .par_iter()
            .zip(self.core.people.par_iter())
            .any(|(a, b)| a != b);
        let generation = if changed {
            Some(
                self.core
                    .generation
                    .checked_add(1)
                    .ok_or("core revision overflow")?,
            )
        } else {
            None
        };
        if let Some(owner) = next.social {
            self.social = Some(CoreSocial {
                owner,
                last_tick: Some(tick),
            });
        }
        if let Some(owner) = next.disease {
            let treatment = self
                .health
                .as_mut()
                .map(|health| std::mem::take(&mut health.treatment))
                .unwrap_or_default();
            self.health = Some(CoreHealth {
                owner,
                treatment,
                last_tick: Some(tick),
            });
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        if let Some(taking) = taking {
            self.taking = Some(taking);
        }
        if let Some(frame) = teaching {
            if frame.rate_changed {
                self.skill_rates = Some(frame.skills.owner.index());
            }
            self.skills = Some(frame.skills);
        }
        if let Some(knowledge) = knowledge {
            self.knowledge = Some(knowledge);
        }
        if let Some(travel) = travel {
            self.core.travel = Some(travel);
        }
        if let Some(generation) = generation {
            self.core.people = people;
            self.core.generation = generation;
            self.work_checked_revision = None;
        }
        Ok(CoreDynamicsResults {
            meetings,
            disease_events,
        })
    }
}
