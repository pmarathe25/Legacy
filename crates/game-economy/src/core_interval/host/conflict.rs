//! Individually chosen local strikes and physical health consequences.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalAttackPolicy {
    pub id: u64,
    pub actor: u64,
    pub target: u64,
    pub force: u32,
    #[serde(default)]
    pub willing: bool,
    #[serde(default = "default_value")]
    pub value: i64,
}

fn default_value() -> i64 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalDefensePolicy {
    pub person: u64,
    pub reduction: u32,
    #[serde(default)]
    pub willing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeCapability {
    pub person: u64,
    pub max_force: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalHarm {
    pub target: u64,
    pub before: u32,
    pub after: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorePersonalConflict {
    #[serde(default)]
    pub attacks: Vec<PersonalAttackPolicy>,
    #[serde(default)]
    pub defenses: Vec<PersonalDefensePolicy>,
    #[serde(default)]
    pub capabilities: Vec<StrikeCapability>,
    #[serde(default)]
    pub events: Vec<game_personal_conflict::StrikeEvent>,
    #[serde(default)]
    pub harm: Vec<PersonalHarm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_tick: Option<u64>,
}

impl CorePersonalConflict {
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let exists = |person| {
            core.people
                .binary_search_by_key(&person, |row| row.id)
                .is_ok()
        };
        if self
            .last_tick
            .is_some_and(|tick| tick > core.physical.tick())
            || (self.last_tick.is_none() && (!self.events.is_empty() || !self.harm.is_empty()))
            || self
                .attacks
                .par_windows(2)
                .any(|rows| (rows[0].actor, rows[0].id) >= (rows[1].actor, rows[1].id))
            || self
                .defenses
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            || self
                .capabilities
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            || self.attacks.par_iter().any(|row| {
                row.id == 0
                    || row.actor == row.target
                    || row.force == 0
                    || !exists(row.actor)
                    || !exists(row.target)
            })
            || self.defenses.par_iter().any(|row| !exists(row.person))
            || self
                .capabilities
                .par_iter()
                .any(|row| row.max_force == 0 || !exists(row.person))
            || self
                .events
                .par_windows(2)
                .any(|rows| (rows[0].target, rows[0].actor) >= (rows[1].target, rows[1].actor))
            || self.events.par_iter().any(|row| {
                !exists(row.actor)
                    || !exists(row.target)
                    || row.actor == row.target
                    || row.force == 0
                    || row.defended > row.force
                    || row.harm != row.force - row.defended
            })
            || self
                .harm
                .par_windows(2)
                .any(|rows| rows[0].target >= rows[1].target)
            || self.harm.par_iter().any(|row| {
                !exists(row.target) || row.before < row.after || row.before > core.rules.max_health
            })
        {
            return Err("invalid personal conflict state".into());
        }
        Ok(())
    }
}

impl CoreHost {
    /// Commit explicit personal strikes and defenses after checking real reach,
    /// availability, and actor capability. A target's consent is not needed.
    pub fn strike_people(
        &mut self,
        strikes: &[game_personal_conflict::Strike],
        defenses: &[game_personal_conflict::Defense],
        occupied: &[u64],
    ) -> Result<Vec<PersonalHarm>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        if strikes.is_empty() {
            return Ok(Vec::new());
        }
        let owner = self
            .personal_conflict
            .as_ref()
            .ok_or("personal conflict is not enabled")?;
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before strikes".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        if strikes.par_iter().any(|strike| {
            let actor = self
                .core
                .people
                .binary_search_by_key(&strike.actor, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index]);
            let target = self
                .core
                .people
                .binary_search_by_key(&strike.target, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index]);
            let capable = owner
                .capabilities
                .binary_search_by_key(&strike.actor, |row| row.person)
                .ok()
                .is_some_and(|index| owner.capabilities[index].max_force >= strike.force);
            !capable
                || occupied.binary_search(&strike.actor).is_ok()
                || active.contains(&strike.actor)
                || actor.is_none_or(|person| person.health == 0)
                || target.is_none_or(|person| person.health == 0)
                || actor.zip(target).is_none_or(|(actor, target)| {
                    locations
                        .region((actor.id, actor.region), (target.id, target.region))
                        .is_none()
                })
        }) || defenses.par_iter().any(|defense| {
            self.core
                .people
                .binary_search_by_key(&defense.person, |row| row.id)
                .is_err()
        }) {
            return Err("strikes require capable available local actors and living targets".into());
        }
        let resolved = game_personal_conflict::resolve(strikes, defenses).map_err(str::to_owned)?;
        let harm: Vec<_> = resolved
            .impacts
            .par_iter()
            .map(|impact| {
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&impact.target, |row| row.id)
                    .expect("validated target")];
                PersonalHarm {
                    target: impact.target,
                    before: person.health,
                    after: person
                        .health
                        .saturating_sub(impact.harm.min(u64::from(u32::MAX)) as u32),
                }
            })
            .collect();
        let changed = harm.par_iter().any(|row| row.before != row.after);
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
        let mut dead: Vec<_> = harm
            .par_iter()
            .filter(|row| row.before > 0 && row.after == 0)
            .map(|row| row.target)
            .collect();
        dead.par_sort_unstable();
        if !dead.is_empty() {
            let interval = self
                .core
                .physical
                .work_interval()
                .ok_or("no physical interval")?;
            let (paused, mut rates): (Vec<_>, Vec<_>) =
                interval.rates.par_iter().copied().partition(|(job, _)| {
                    self.core
                        .physical
                        .work()
                        .binary_search_by_key(job, |row| row.job.id)
                        .ok()
                        .is_some_and(|index| {
                            dead.binary_search(&self.core.physical.work()[index].job.worker)
                                .is_ok()
                        })
                });
            let mut saved = self.paused_work_rates.clone();
            saved.par_extend(paused);
            saved.par_sort_unstable_by_key(|row| row.0);
            rates.par_sort_unstable_by_key(|row| row.0);
            self.core.physical.replace_interval_rates(&rates)?;
            self.paused_work_rates = saved;
        }
        if let Some(generation) = generation {
            self.core.people.par_iter_mut().for_each(|person| {
                if let Ok(index) = harm.binary_search_by_key(&person.id, |row| row.target) {
                    person.health = harm[index].after;
                }
            });
            self.core.generation = generation;
            self.work_checked_revision = None;
        }
        let owner = self.personal_conflict.as_mut().expect("conflict checked");
        owner.events = resolved.events;
        owner.harm = harm.clone();
        owner.last_tick = Some(tick);
        Ok(harm)
    }

    /// Inspect the person's own willing attack policies without reserving or
    /// requesting consent from the target. Targets may already be occupied.
    pub fn preview_personal_attacks(
        &self,
        occupied: &[u64],
    ) -> Result<Vec<PersonalAttackPolicy>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = &self.personal_conflict else {
            return Ok(Vec::new());
        };
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        Ok(owner
            .attacks
            .par_iter()
            .filter(|policy| {
                let actor = self
                    .core
                    .people
                    .binary_search_by_key(&policy.actor, |row| row.id)
                    .ok()
                    .map(|index| &self.core.people[index]);
                let target = self
                    .core
                    .people
                    .binary_search_by_key(&policy.target, |row| row.id)
                    .ok()
                    .map(|index| &self.core.people[index]);
                let capable = owner
                    .capabilities
                    .binary_search_by_key(&policy.actor, |row| row.person)
                    .ok()
                    .is_some_and(|index| owner.capabilities[index].max_force >= policy.force);
                policy.willing
                    && capable
                    && occupied.binary_search(&policy.actor).is_err()
                    && !active.contains(&policy.actor)
                    && actor.is_some_and(|person| !person.controlled && person.health > 0)
                    && target.is_some_and(|person| person.health > 0)
                    && actor.zip(target).is_some_and(|(actor, target)| {
                        locations
                            .region((actor.id, actor.region), (target.id, target.region))
                            .is_some()
                    })
            })
            .copied()
            .collect())
    }
}
