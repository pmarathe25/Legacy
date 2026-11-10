//! Personal meetings, subjective relationships, and communicated observations.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSocial {
    pub owner: game_social::Social,
    #[serde(default)]
    pub last_tick: Option<u64>,
}

impl CoreHost {
    /// Run at most one personal social generation per physical tick.
    pub fn meet_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<game_social::Encounter>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(social) = &self.social else {
            return Ok(Vec::new());
        };
        let tick = self.core.physical.tick();
        if tick == 0 || social.last_tick == Some(tick) {
            return Ok(Vec::new());
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
            social: Some(social.owner.clone()),
            personality: None,
            disease: None,
            treatment: vec![],
            claims: None,
        };
        let mut people: Vec<_> = self.core.people.par_iter().cloned().collect();
        let next = opening.prepare(
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
                inquiry_meetings: &[],
            },
        )?;
        let meetings: Vec<_> = next
            .social
            .as_ref()
            .expect("social owner")
            .events
            .par_iter()
            .cloned()
            .collect();
        let mut opinions = self.opinions.clone();
        game_economy_dynamics::learn_social(&mut opinions, Some(&next), tick);
        let knowledge = if let Some(owner) = &self.knowledge {
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
        self.social = Some(CoreSocial {
            owner: next.social.expect("social owner"),
            last_tick: Some(tick),
        });
        self.opinions = opinions;
        if let Some(knowledge) = knowledge {
            self.knowledge = Some(knowledge);
        }
        if let Some(travel) = travel {
            self.core.travel = Some(travel);
        }
        Ok(meetings)
    }
}
