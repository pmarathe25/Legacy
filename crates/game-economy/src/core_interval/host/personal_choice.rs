//! An opt-in competition between physical construction and local carrying.
//! Other scheduler domains keep their existing behavior until separately migrated.
use super::*;
use game_personal_choice::{Key, Person, Proposal};

pub(super) const CONSTRUCTION: u32 = 1;
pub(super) const CARRYING: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreChoiceIntent {
    pub actor: u64,
    pub key: Key,
    /// Exact physical movements except transient action and new-site identities.
    pub movement: game_lots::Movement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_movement: Option<game_lots::Movement>,
}

impl CoreChoiceIntent {
    pub(super) fn magnitude(&self) -> i64 {
        (u128::from(self.movement.quantity)
            + self
                .second_movement
                .map_or(0, |row| u128::from(row.quantity)))
        .min(i64::MAX as u128) as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreChoiceOutcome {
    NoLocalAction,
    Stale,
    Accepted,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreChoiceEvent {
    pub tick: u64,
    pub offset_millis: u64,
    pub actor: u64,
    pub key: Option<Key>,
    pub outcome: CoreChoiceOutcome,
}

pub(super) fn intention_permitted(person: &Person, intent: &CoreChoiceIntent) -> bool {
    let Some(preference) = person
        .preferences
        .binary_search_by_key(&intent.key.domain, |row| row.domain)
        .ok()
        .map(|index| person.preferences[index])
        .filter(|row| row.enabled)
    else {
        return false;
    };
    let magnitude = intent.magnitude();
    let score =
        i128::from(magnitude) * i128::from(preference.weight) / 1000 + i128::from(preference.bias);
    score > i128::from(person.idle) || (score == i128::from(person.idle) && person.act_on_tie)
}

impl CoreHost {
    /// Inspect an uncommitted choice round from the current opening state.
    /// No result is a physical action or an instruction to another person.
    pub fn preview_personal_choices(
        &self,
        blocked: &[u64],
    ) -> Result<(Vec<CoreChoiceIntent>, Vec<CoreChoiceEvent>), String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let people = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_deref())
            .ok_or("personal choices are not enabled")?;
        self.choose_local_actions(blocked, people)
    }

    /// Enable voluntary competition at a clean decision boundary.
    pub fn enable_personal_choices(&mut self, people: Vec<Person>) -> Result<(), String> {
        let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
        if !matches!(schedule.phase, DecisionPhase::Meals) {
            return Err("finish the pending decision before changing personal choices".into());
        }
        game_personal_choice::validate_people(&people).map_err(str::to_owned)?;
        if people.par_iter().any(|p| {
            self.core
                .people
                .binary_search_by_key(&p.id, |row| row.id)
                .is_err()
        }) {
            return Err("personal choices require known people".into());
        }
        self.schedule
            .as_mut()
            .expect("checked schedule")
            .personal_choices = Some(std::sync::Arc::new(people));
        Ok(())
    }

    fn choice_busy(&self, blocked: &[u64]) -> Vec<u64> {
        let rest = self.blocked_with_rest(blocked);
        let mut busy = rest.as_deref().unwrap_or(blocked).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        busy
    }

    /// Read candidates against one opening state; no resource is reserved here.
    fn choice_candidates(
        &self,
        blocked: &[u64],
        domain: Option<u32>,
    ) -> Result<Vec<CoreChoiceIntent>, String> {
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let busy = self.choice_busy(blocked);
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let construction = || -> Result<Vec<CoreChoiceIntent>, String> {
            if domain.is_none_or(|domain| domain == CONSTRUCTION)
                && let Some(owner) = &self.construction
                && !owner.policies.is_empty()
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.construction");
                let sites = self
                    .sites
                    .as_ref()
                    .ok_or("construction requires work sites")?;
                let opportunities = game_economy_building::Context {
                    owner,
                    sites: &sites.facilities,
                    people: &self.core.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.core.rules.max_work_hunger,
                    tick,
                    manual: None,
                    goal_needs: &[],
                    unwilling: &[],
                }
                .direct_opportunities();
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| {
                        let retained = (opportunity.salvage > 0).then_some(game_lots::Movement {
                            id: 0,
                            from: opportunity.source,
                            to: Some(game_lots::Place::Site(0)),
                            good: opportunity.good,
                            quantity: opportunity.salvage,
                            item: None,
                        });
                        let consumed = (opportunity.salvage < opportunity.quantity).then(|| {
                            game_lots::Movement {
                                id: 0,
                                from: opportunity.source,
                                to: None,
                                good: opportunity.good,
                                quantity: opportunity.quantity - opportunity.salvage,
                                item: None,
                            }
                        });
                        CoreChoiceIntent {
                            actor: opportunity.person,
                            key: Key {
                                domain: CONSTRUCTION,
                                action: u64::from(opportunity.blueprint),
                            },
                            movement: retained
                                .or(consumed)
                                .expect("positive construction quantity"),
                            second_movement: retained.and(consumed),
                        }
                    })
                    .collect();
                Ok(rows)
            } else {
                Ok(Vec::new())
            }
        };
        let carrying = || -> Vec<CoreChoiceIntent> {
            if domain.is_none_or(|domain| domain == CARRYING)
                && let Some(owner) = &self.local_transport
                && !owner.policies.is_empty()
            {
                #[cfg(feature = "profile")]
                let _span = game_diagnostics::span("choice.preview.carrying");
                let opportunities = game_economy_local_transport::Context {
                    owner,
                    people: &self.core.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.core.rules.max_work_hunger,
                    manual: None,
                    walking: None,
                }
                .direct_opportunities();
                let rows: Vec<_> = opportunities
                    .into_par_iter()
                    .map(|opportunity| CoreChoiceIntent {
                        actor: opportunity.person,
                        key: Key {
                            domain: CARRYING,
                            action: u64::from(opportunity.good),
                        },
                        movement: game_lots::Movement {
                            id: 0,
                            from: opportunity.from,
                            to: Some(opportunity.to),
                            good: opportunity.good,
                            quantity: opportunity.quantity,
                            item: None,
                        },
                        second_movement: None,
                    })
                    .collect();
                rows
            } else {
                Vec::new()
            }
        };
        let (construction, carrying) = rayon::join(construction, carrying);
        let mut intents = construction?;
        intents.par_extend(carrying);
        intents.par_sort_unstable_by_key(|intent| (intent.actor, intent.key));
        Ok(intents)
    }

    pub(super) fn choose_local_actions(
        &self,
        blocked: &[u64],
        people: &[Person],
    ) -> Result<(Vec<CoreChoiceIntent>, Vec<CoreChoiceEvent>), String> {
        let candidates = self.choice_candidates(blocked, None)?;
        let busy = self.choice_busy(blocked);
        let available = |id: u64| {
            busy.binary_search(&id).is_err()
                && self
                    .core
                    .people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .is_some_and(|i| {
                        self.core.people[i].health > 0 && !self.core.people[i].controlled
                    })
        };
        let proposals: Vec<_> = candidates
            .par_iter()
            .filter(|intent| people.binary_search_by_key(&intent.actor, |p| p.id).is_ok())
            .map(|intent| {
                let observed = intent.magnitude();
                Proposal {
                    actor: intent.actor,
                    key: intent.key,
                    value: observed,
                }
            })
            .collect();
        let round = self.core.physical.tick().rotate_left(21)
            ^ self.core.elapsed_millis().ok_or("no physical interval")?;
        #[cfg(feature = "profile")]
        let select_span = game_diagnostics::span("choice.select");
        let selected = game_personal_choice::select(people, &proposals, self.core.seed, round)
            .map_err(str::to_owned)?;
        #[cfg(feature = "profile")]
        drop(select_span);
        let intents: Vec<_> = selected
            .par_iter()
            .filter_map(|row| row.action.map(|key| (row.actor, key)))
            .map(|key| {
                candidates
                    .binary_search_by_key(&key, |candidate| (candidate.actor, candidate.key))
                    .map(|i| candidates[i].clone())
                    .map_err(|_| "selected opportunity disappeared".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let idle = selected
            .par_iter()
            .filter(|row| row.action.is_none() && available(row.actor))
            .map(|row| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: row.actor,
                key: None,
                outcome: CoreChoiceOutcome::NoLocalAction,
            })
            .collect();
        Ok((intents, idle))
    }

    /// Revalidate exact selected movements against the current state. A stale
    /// intention never turns into a different action or an implicit fallback.
    pub(super) fn choice_blocked(
        &self,
        blocked: &[u64],
        people: &[Person],
        intents: &[CoreChoiceIntent],
        domain: u32,
    ) -> Result<(Vec<u64>, Vec<CoreChoiceEvent>), String> {
        #[cfg(feature = "profile")]
        let _span = game_diagnostics::span("choice.revalidate");
        let current = self.choice_candidates(blocked, Some(domain))?;
        let checked: Vec<_> = intents
            .par_iter()
            .filter(|row| row.key.domain == domain)
            .map(|intent| {
                let found = current
                    .binary_search_by_key(&(intent.actor, intent.key), |row| (row.actor, row.key))
                    .ok()
                    .is_some_and(|i| {
                        current[i].movement == intent.movement
                            && current[i].second_movement == intent.second_movement
                    });
                (intent, found)
            })
            .collect();
        let mut valid: Vec<_> = checked
            .par_iter()
            .filter(|(_, found)| *found)
            .map(|(intent, _)| intent.actor)
            .collect();
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let stale = checked
            .par_iter()
            .filter(|(_, found)| !*found)
            .map(|(intent, _)| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: intent.actor,
                key: Some(intent.key),
                outcome: CoreChoiceOutcome::Stale,
            })
            .collect();
        valid.par_sort_unstable();
        let mut denied = blocked.to_vec();
        denied.par_extend(
            people
                .par_iter()
                .filter(|p| valid.binary_search(&p.id).is_err())
                .map(|p| p.id),
        );
        denied.par_sort_unstable();
        denied.dedup();
        Ok((denied, stale))
    }
}
