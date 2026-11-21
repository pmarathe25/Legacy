//! Connect local fight rounds to shared physical health and availability.
use super::*;
use game_economy_combat::{
    Combat,
    elapsed::{Actor, Choice, Engagement, Event, Start, Tactical},
    elapsed_structures::SiteStrike,
};
use game_lots::Place;
use game_personal_conflict::{Defense, Strike};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreTacticalResult {
    pub events: Vec<Event>,
    pub strikes: Vec<game_personal_conflict::StrikeEvent>,
    pub harm: Vec<PersonalHarm>,
    pub sites: Vec<SiteStrike>,
    pub interrupted_jobs: Vec<u64>,
    pub released: Vec<game_economy_confinement::Event>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerTactics {
    pub engagement: u64,
    pub next_round_millis: u128,
    pub position: game_economy_combat::Point,
    pub visible: Vec<(u64, game_economy_combat::Point)>,
    pub visible_sites: Vec<(u64, game_economy_combat::Point)>,
}

impl CoreHost {
    pub(super) fn tactical(&self) -> Option<&Tactical> {
        self.personal_conflict.as_ref()?.tactical.as_ref()
    }

    pub(super) fn has_tactical_engagement(&self, person: u64) -> bool {
        self.tactical()
            .is_some_and(|owner| owner.engagement(person).is_some())
    }

    pub fn tactical_engagement(&self, person: u64) -> Option<&Engagement> {
        self.tactical()?.engagement(person)
    }

    pub(super) fn visible_tactics(&self, person: u64) -> Option<CorePlayerTactics> {
        let owner = self.tactical()?;
        let fight = owner.engagement(person)?;
        let index = fight
            .people
            .binary_search_by_key(&person, |row| row.person)
            .ok()?;
        let position = fight.people[index].position;
        let map = owner.map(fight.region)?;
        let visible = fight
            .people
            .par_iter()
            .filter_map(|row| {
                (row.person != person && map.visible(position, row.position))
                    .then_some((row.person, row.position))
            })
            .collect();
        let visible_sites = owner
            .structures
            .targets
            .par_iter()
            .filter_map(|site| {
                (site.region == fight.region && map.visible(position, site.position))
                    .then_some((site.site, site.position))
            })
            .collect();
        Some(CorePlayerTactics {
            engagement: fight.id,
            next_round_millis: fight.next_millis,
            position,
            visible,
            visible_sites,
        })
    }

    pub(super) fn next_tactical_deadline(&self) -> Option<u64> {
        let at = self.tactical()?.next_round()?;
        u64::try_from(at.saturating_sub(self.core.simulated_millis())).ok()
    }

    pub(super) fn decision_occupations(&self, external: &[u64]) -> Option<Vec<u64>> {
        let tactical = self
            .tactical()
            .filter(|owner| !owner.engagements.is_empty());
        if self.fatigue.is_none() && tactical.is_none() {
            return None;
        }
        let mut people = external.to_vec();
        if let Some(owner) = &self.fatigue {
            people.par_extend(owner.fatigue.resting());
        }
        if let Some(owner) = tactical {
            people.extend(owner.actors());
        }
        people.par_sort_unstable();
        people.dedup();
        Some(people)
    }

    /// Explicitly start a physically reachable encounter, independent of the
    /// target's response. Each participant acts on their own choice at a round.
    pub fn start_controlled_engagement(
        &mut self,
        actor: u64,
        target: u64,
    ) -> Result<Event, String> {
        let actor_state = self.controlled_person(actor)?;
        let target_state = self
            .core
            .people
            .binary_search_by_key(&target, |row| row.id)
            .ok()
            .map(|index| &self.core.people[index])
            .ok_or("unknown confrontation target")?;
        let region = actor_state
            .region
            .filter(|region| {
                actor_state.health > 0
                    && target_state.health > 0
                    && target_state.region == Some(*region)
            })
            .ok_or("confrontation requires local living people")?;
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending physical events before initiating a confrontation".into());
        }
        let busy = self.local_walk_exclusions(&[]);
        if busy.binary_search(&actor).is_ok()
            || !self.regional_stock_reachable(actor, region)
            || !self.regional_stock_reachable(target, region)
            || !self.same_local_endpoint(actor, target)
        {
            return Err("confrontation requires an available actor within physical reach".into());
        }
        let admissions = self
            .tactical()
            .ok_or("tactical encounters are disabled")?
            .prepare_starts(
                &[Start {
                    actor,
                    target,
                    region,
                }],
                self.core.simulated_millis(),
            )?;
        if admissions.events().is_empty() {
            return Err("no shared open tactical space".into());
        }
        let mut interrupted: Vec<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter_map(|row| {
                (row.phase == game_lot_runtime::Phase::Working && row.job.worker == target)
                    .then_some((row.job.id, target))
            })
            .collect();
        interrupted.par_sort_unstable();
        if !interrupted.is_empty() {
            self.abandon_work(&interrupted)?;
        }
        let events = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.tactical.as_mut())
            .expect("tactical owner")
            .publish_starts(admissions)?;
        Ok(events[0])
    }

    pub(super) fn confront_autonomously(
        &mut self,
        blocked: &[u64],
        period_millis: u64,
    ) -> Result<Vec<Event>, String> {
        let (Some(owner), Some(opinions)) = (self.tactical(), self.opinions.as_ref()) else {
            return Ok(Vec::new());
        };
        let now = self.core.simulated_millis();
        let review = now / u128::from(period_millis);
        let due = |policy: &game_opinions::Confrontation| {
            policy.enabled
                && policy.interval != 0
                && review.is_multiple_of(u128::from(policy.interval))
        };
        if !opinions
            .sensitivities
            .par_iter()
            .any(|row| row.confrontation.as_ref().is_some_and(due))
        {
            return Ok(Vec::new());
        }
        let busy = self.local_walk_exclusions(blocked);
        let locate = |id| {
            self.core
                .people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index])
        };
        let proposals: Vec<_> = opinions
            .records
            .par_chunk_by(|a, b| a.observer == b.observer)
            .filter_map(|known| {
                let actor = locate(known[0].observer)?;
                let region = actor.region?;
                if actor.controlled
                    || actor.health == 0
                    || busy.binary_search(&actor.id).is_ok()
                    || !self.regional_stock_reachable(actor.id, region)
                    || owner.map(region).is_none()
                {
                    return None;
                }
                let preference = opinions
                    .sensitivities
                    .binary_search_by_key(&actor.id, |row| row.person)
                    .ok()
                    .and_then(|index| opinions.sensitivities[index].confrontation.as_ref())
                    .filter(|policy| due(policy))?;
                let target = known
                    .iter()
                    .filter(|row| {
                        row.trust <= preference.trust_at_most
                            && row.fear <= preference.fear_at_most
                            && busy.binary_search(&row.target).is_err()
                            && locate(row.target).is_some_and(|target| {
                                target.health > 0
                                    && target.region == Some(region)
                                    && self.regional_stock_reachable(target.id, region)
                            })
                    })
                    .min_by_key(|row| (row.trust, row.fear, row.target))?
                    .target;
                Some(Start {
                    actor: actor.id,
                    target,
                    region,
                })
            })
            .collect();
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let mut incidents: Vec<_> = proposals
            .par_iter()
            .enumerate()
            .flat_map_iter(|(slot, row)| {
                let priority = game_genetics::random(
                    self.core.seed,
                    self.core.decision_time_key() ^ row.target,
                    row.actor,
                );
                [(row.actor, priority, slot), (row.target, priority, slot)]
            })
            .collect();
        incidents.par_sort_unstable();
        let winners: Vec<_> = incidents
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| (group[0].0, group[0].2))
            .collect();
        let mut starts: Vec<_> = proposals
            .par_iter()
            .enumerate()
            .filter_map(|(slot, row)| {
                let won = |person| {
                    winners
                        .binary_search_by_key(&person, |winner| winner.0)
                        .ok()
                        .is_some_and(|index| winners[index].1 == slot)
                };
                (won(row.actor) && won(row.target)).then_some(*row)
            })
            .collect();
        starts.par_sort_unstable_by_key(|row| (row.actor, row.target));
        let admissions = owner.prepare_starts(&starts, now)?;
        self.personal_conflict
            .as_mut()
            .and_then(|owner| owner.tactical.as_mut())
            .expect("tactical owner")
            .publish_starts(admissions)
    }

    pub fn choose_controlled_tactics(&mut self, actor: u64, choice: Choice) -> Result<(), String> {
        let person = self.controlled_person(actor)?;
        if person.health == 0 {
            return Err("an incapacitated person cannot act in a tactical round".into());
        }
        self.personal_conflict
            .as_mut()
            .and_then(|owner| owner.tactical.as_mut())
            .ok_or("tactical encounters are disabled")?
            .choose(actor, choice)
    }

    pub fn publish_tactical_rounds(&mut self) -> Result<Option<CoreTacticalResult>, String> {
        let Some(owner) = self.tactical() else {
            return Ok(None);
        };
        let now = self.core.simulated_millis();
        let Some(next) = owner.next_round() else {
            return Ok(None);
        };
        if next != now {
            return if next > now {
                Ok(None)
            } else {
                Err("missed tactical deadline".into())
            };
        }
        let wounds = self
            .personal_conflict
            .as_ref()
            .and_then(|owner| owner.wounds.as_ref());
        let resting = self.blocked_with_rest(&[]);
        let tick = self.core.physical.tick();
        let people: Vec<_> = owner
            .actors()
            .into_par_iter()
            .filter_map(|id| {
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&id, |row| row.id)
                    .ok()?];
                let supplies = self.core.physical.custody(Place::Person(id));
                let carries = |good| {
                    supplies.is_some_and(|holder| {
                        holder.lots.iter().any(|lot| {
                            lot.good == good
                                && lot.expires_at.is_none_or(|expiry| {
                                    expiry > self.core.physical.lot_instant(tick)
                                })
                        })
                    })
                };
                let force = owner.base_force.saturating_add(
                    owner
                        .weapons
                        .iter()
                        .filter(|row| carries(row.good))
                        .map(|row| row.bonus)
                        .max()
                        .unwrap_or(0),
                );
                let armor = owner
                    .protection
                    .iter()
                    .filter(|row| carries(row.good))
                    .map(|row| row.reduction)
                    .max()
                    .unwrap_or(0);
                Some(Actor {
                    id,
                    region: person.region,
                    health: person.health,
                    can_act: resting
                        .as_ref()
                        .is_none_or(|rows| rows.binary_search(&id).is_err()),
                    controlled: person.controlled,
                    force: wounds.map_or(force, |wounds| wounds.available_force(id, force)),
                    armor,
                })
            })
            .collect();
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let mut draft = owner.prepare_rounds(now, &people, self.core.seed, &|actor, target| {
            opinions
                .as_ref()
                .map_or((0, 0), |index| index.feelings(actor, target))
        })?;
        let mut site_strikes = std::mem::take(&mut draft.site_strikes);
        let site_damage = self.stage_tactical_sites(&site_strikes)?;
        let strikes: Vec<_> = draft
            .hits
            .par_iter()
            .map(|hit| Strike {
                actor: hit.actor,
                target: hit.target,
                force: hit.force,
            })
            .collect();
        let mut result = CoreTacticalResult::default();
        if !strikes.is_empty() {
            let defenses: Vec<_> = draft
                .protection
                .par_iter()
                .map(|&(person, reduction)| Defense { person, reduction })
                .collect();
            let resolved =
                game_personal_conflict::resolve(&strikes, &defenses).map_err(str::to_owned)?;
            result.strikes = resolved.events.clone();
            result.harm = self.apply_personal_harm(resolved)?;
        }
        let mut unable: Vec<_> = result
            .harm
            .par_iter()
            .filter_map(|row| (row.after == 0).then_some(row.target))
            .collect();
        unable.par_sort_unstable();
        result.events = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.tactical.as_mut())
            .expect("tactical owner")
            .publish_rounds(draft, &unable);
        if let Some(damage) = site_damage {
            (result.interrupted_jobs, result.released) =
                self.publish_tactical_sites(damage, &mut site_strikes)?;
            result.sites = site_strikes;
        }
        self.work_checked_revision = None;
        Ok(Some(result))
    }
}

pub(super) fn import_tactical(
    combat: Combat,
    round_millis: u64,
) -> Result<(Tactical, Option<game_personal_conflict::wounds::Wounds>), String> {
    let wounds = combat.injuries.as_ref().map(|injuries| {
        let rules = &injuries.rules;
        game_personal_conflict::wounds::Wounds {
            rules: game_personal_conflict::wounds::Rules {
                minimum_harm: rules.minimum_damage,
                recovery_ticks: rules.recovery_ticks,
                force_loss_per_severity: u32::from(rules.immobilizes),
                work_strength_loss_per_severity: u32::from(rules.work_percent < 100),
            },
            cases: Vec::new(),
            remedies: vec![game_personal_conflict::wounds::Remedy {
                good: rules.medicine,
                quantity: rules.medicine_quantity,
                severity_relief: u32::MAX,
            }],
        }
    });
    Ok((Tactical::from_fresh(combat, round_millis)?, wounds))
}
