//! Cultivation choices and crop growth through physical custody.
use super::*;
use game_economy_agriculture::elapsed_growth::{Growth, Rates};
type CultivationClaims = Option<(game_economy_claims::Claims, Option<game_opinions::Opinions>)>;
type CropProgress = (Option<Growth>, Option<game_lots::BatchAllocation>);

pub(super) struct CultivatedNow {
    pub actors: Vec<u64>,
    pub choices: Vec<CoreChoiceEvent>,
    pub fields: Vec<game_economy_agriculture::opportunities::Opportunity>,
    pub review: Option<FarmReview>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmReview {
    pub work: Vec<game_economy_agriculture::elapsed_actions::FieldWork>,
    pub requests: Vec<game_economy_agriculture::requests::Event>,
}

impl CoreHost {
    fn elapsed_farm_review(&self) -> Option<game_economy_agriculture::elapsed_actions::Review> {
        self.agriculture.as_ref()?.action_clock?
            .review(u64::try_from(self.core.simulated_millis()).ok()?)
    }

    pub(super) fn farm_review_due(&self) -> bool {
        self.elapsed_farm_review().is_some()
    }

    pub(super) fn cultivated_at_decisions(&self) -> bool {
        self.agriculture
            .as_ref()
            .is_some_and(|farm| farm.growth_clock.is_some())
    }

    /// Settle the fields selected by competing personal choices at this decision instant.
    pub(super) fn cultivate_at_decision(
        &mut self,
        blocked: &[u64],
        intents: &[CoreChoiceIntent],
    ) -> Result<CultivatedNow, String> {
        let resolved = self.resolve_cultivation_fields(intents);
        let mut actors: Vec<_> = resolved
            .par_iter()
            .map(|(choice, _)| choice.person)
            .collect();
        let selected: Vec<_> = resolved
            .par_iter()
            .filter_map(|(choice, won)| won.then_some(*choice))
            .collect();
        let mut outcomes = Vec::new();
        let mut recorded_review = None;
        let tick = self.core.physical.tick();
        let offset_millis = self
            .core
            .elapsed_millis()
            .ok_or("no cultivation interval")?;
        let review = self.elapsed_farm_review();
        if !selected.is_empty() || review.is_some() {
            let owner = self
                .agriculture
                .as_ref()
                .expect("elapsed cultivation owner");
            let busy = self.choice_busy(blocked);
            let away = self.block_away_from_stock(&busy);
            let personal_busy = away.as_deref().unwrap_or(&busy);
            let mut activity_busy = Vec::new();
            if review.is_some() {
                activity_busy.extend_from_slice(personal_busy);
                activity_busy.par_extend(intents.par_iter().map(|intent| intent.actor));
                activity_busy.par_sort_unstable();
                activity_busy.dedup();
            }
            let view = self.core.physical.view(tick)?;
            let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
            let agreement_evidence = self.farm_agreement_evidence(tick)?;
            let context = game_economy_agriculture::Context {
                agriculture: owner,
                agreement_evidence,
                people: &self.core.people,
                opinions: self.opinions.as_ref(),
                claims: claims.as_ref(),
                busy: if review.is_some() { &activity_busy } else { personal_busy },
                contact_blocked: blocked,
                catalog: self.core.physical.catalog(),
                view: &view,
                lot_instant: self.core.physical.lot_instant(tick),
                tick,
                manual: None,
                manual_response: None,
                manual_withdrawal: None,
                reserved_field: None,
                season: self.current_season(tick),
            };
            let plan = if let Some(review) = review {
                let profiled: Vec<_> = self.schedule.as_ref()
                    .and_then(|schedule| schedule.personal_choices.as_ref())
                    .map_or_else(Vec::new, |profiles| profiles.par_iter().map(|p| p.id).collect());
                context.prepare_elapsed_review(&selected, &profiled, personal_busy, review)?
            } else {
                let selected_people: Vec<_> = selected.par_iter().map(|choice| choice.person).collect();
                context.prepare_elapsed(&selected, &selected_people)?
            };
            actors.par_extend(plan.actors.par_iter().chain(plan.requesters.par_iter()).copied());
            actors.par_sort_unstable();
            actors.dedup();
            let work: Vec<_> = if review.is_some() {
                plan.actors.par_iter().zip(plan.changes.par_iter())
                    .map(|(&person, &(field, _, resulting_phase, _))| {
                        game_economy_agriculture::elapsed_actions::FieldWork {
                            person, field, resulting_phase,
                        }
                    }).collect()
            } else {
                Vec::new()
            };
            outcomes = self.commit_elapsed_cultivation(plan)?;
            if review.is_some() {
                let work = work.into_par_iter().filter(|row| {
                    outcomes.binary_search_by_key(&row.person, |outcome| outcome.0)
                        .is_ok_and(|slot| outcomes[slot].1)
                }).collect();
                let requests = self.agriculture.as_ref().and_then(|farm| farm.requests.as_ref())
                    .map_or_else(Vec::new, |owner| owner.events.clone());
                recorded_review = Some(FarmReview { work, requests });
            }
        }
        let choices: Vec<_> = resolved
            .par_iter()
            .map(|(choice, won)| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: choice.person,
                key: Some(game_personal_choice::Key {
                    domain: personal_choice::CULTIVATION,
                    action: choice.field,
                }),
                outcome: if *won {
                    outcomes
                        .binary_search_by_key(&choice.person, |row| row.0)
                        .map_or(CoreChoiceOutcome::Stale, |index| {
                            if outcomes[index].1 {
                                CoreChoiceOutcome::Accepted
                            } else {
                                CoreChoiceOutcome::Denied
                            }
                        })
                } else {
                    CoreChoiceOutcome::Denied
                },
            })
            .collect();
        let fields = resolved
            .into_par_iter()
            .zip(choices.par_iter())
            .filter_map(|((field, _), event)| {
                (event.outcome == CoreChoiceOutcome::Accepted).then_some(field)
            })
            .collect();
        Ok(CultivatedNow {
            actors,
            choices,
            fields,
            review: recorded_review,
        })
    }

    pub fn cultivate_controlled(
        &mut self,
        person: u64,
        field: u64,
        carried_batch: Option<u64>,
    ) -> Result<Option<game_economy_agriculture::Phase>, String> {
        let actor = self.controlled_person(person)?;
        let region = actor.region.ok_or("field is outside physical reach")?;
        if !self.regional_stock_reachable(person, region) {
            return Err("field is outside physical reach".into());
        }
        let busy = self.choice_busy(&[]);
        if busy.binary_search(&person).is_ok()
            || self.assisting_people().binary_search(&person).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before cultivating".into());
        }
        let owner = self
            .agriculture
            .as_ref()
            .filter(|farm| farm.growth_clock.is_some())
            .ok_or("elapsed cultivation is disabled")?;
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = game_economy_agriculture::Context {
            agriculture: owner,
            agreement_evidence: None,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            contact_blocked: &[],
            catalog: self.core.physical.catalog(),
            view: &view,
            lot_instant: self.core.physical.lot_instant(tick),
            tick,
            manual: Some((person, field)),
            manual_response: None,
            manual_withdrawal: None,
            reserved_field: None,
            season: self.current_season(tick),
        }
        .prepare_controlled_elapsed(carried_batch)?;
        let phase = plan.changes.first().map(|change| change.2);
        let outcomes = self.commit_elapsed_cultivation(plan)?;
        Ok(phase.filter(|_| outcomes.first() == Some(&(person, true))))
    }

    fn commit_elapsed_cultivation(
        &mut self,
        mut plan: game_economy_agriculture::Plan,
    ) -> Result<Vec<(u64, bool)>, String> {
        if plan.changes.is_empty() && !plan.advances_review() {
            return Ok(Vec::new());
        }
        let mut actions = std::mem::take(&mut plan.plant);
        actions.append(&mut plan.harvest);
        let seed = self.core.seed ^ self.core.decision_time_key();
        let (draft, events) = self
            .core
            .physical
            .prepare_interval_cultivation(actions, seed)?;
        let mut outcomes: Vec<_> = plan
            .actors
            .par_iter()
            .zip(plan.changes.par_iter())
            .map(|(&actor, &(_, action, _, _))| {
                let accepted = events
                    .physical_actions
                    .binary_search_by_key(&action, |row| row.id)
                    .is_ok_and(|index| events.physical_actions[index].accepted);
                (actor, accepted)
            })
            .collect();
        outcomes.par_sort_unstable_by_key(|row| row.0);
        let moved_material = outcomes.par_iter().any(|row| row.1);
        if plan.advances_review() || moved_material {
            let (farm, claims) = self.stage_cultivation(Some(plan), &events, &[])?;
            if moved_material {
                self.core.physical.commit_interval_consumption(draft)?;
            }
            self.agriculture = farm;
            if let Some((claims, opinions)) = claims {
                self.claims = Some(claims);
                self.opinions = opinions;
            }
        }
        Ok(outcomes)
    }

    fn farm_agreement_evidence(
        &self,
        evidence_tick: u64,
    ) -> Result<Option<(&game_agreements::Agreements, u64)>, String> {
        self.agreements.as_ref().map(|owner| {
            owner.instant(self.core.simulated_millis(), evidence_tick)
                .map(|instant| (owner, instant.now))
        }).transpose().map_err(str::to_owned)
    }

    pub(super) fn next_dry_field(&self) -> Option<u64> {
        u64::try_from(self.irrigation.as_ref()?.next_dry_after()?).ok()
    }

    pub(super) fn plan_crop_segment(
        &self,
        requested: u64,
    ) -> Result<(Option<Rates<'_>>, u64), String> {
        let Some(owner) = self
            .agriculture
            .as_ref()
            .filter(|owner| owner.growth_clock.is_some())
        else {
            return Ok((None, requested));
        };
        if !owner
            .fields
            .par_iter()
            .any(|field| field.phase == game_economy_agriculture::Phase::Growing)
        {
            return Ok((None, requested));
        }
        let now = self.core.simulated_millis();
        let season_change = self
            .environment
            .as_ref()
            .and_then(|environment| environment.next_season_change(now));
        let rates = owner.elapsed_growth_rates(
            self.current_season(self.core.physical.tick()),
            self.irrigation.as_ref(),
        )?;
        let maturation = rates.as_ref().and_then(Rates::next_maturation);
        let consumed = [season_change, maturation]
            .into_iter()
            .flatten()
            .fold(u128::from(requested), u128::min) as u64;
        Ok((rates, consumed))
    }

    pub(super) fn stage_crop_growth(
        &self,
        rates: Option<Rates<'_>>,
        millis: u64,
    ) -> Result<CropProgress, String> {
        let Some(rates) = rates.filter(|_| millis > 0) else {
            return Ok((None, None));
        };
        let owner = self.agriculture.as_ref().expect("elapsed crop owner");
        let draft = owner.stage_elapsed_growth(rates, millis, self.core.seed)?;
        let batch_count = draft.batch_count();
        let batch_owner = game_economy_agriculture::reproduction::BATCH_OWNER;
        let allocation = (batch_count > 0)
            .then(|| {
                self.core.physical.plan_batches(&[game_lots::BatchCount {
                    owner: batch_owner,
                    count: batch_count,
                }])
            })
            .transpose()?;
        let first = allocation
            .as_ref()
            .and_then(|allocation| allocation.range(batch_owner).map(|range| range.start));
        let growth = draft.resolve(first)?;
        growth.validate(owner)?;
        Ok((Some(growth), allocation))
    }

    pub(super) fn publish_soil_recovery(
        &mut self,
        recovery: Option<game_economy_agriculture::soil::Recovery>,
    ) {
        if let Some(recovery) = recovery {
            let soil = self
                .agriculture
                .as_mut()
                .and_then(|farm| farm.soil.as_mut())
                .expect("prepared soil recovery");
            recovery.publish(soil);
        }
    }

    pub(super) fn publish_crop_growth(&mut self, growth: Option<Growth>) {
        if let Some(growth) = growth {
            growth.publish_validated(self.agriculture.as_mut().expect("staged crop growth"));
        }
    }

    pub(super) fn current_season(&self, legacy_tick: u64) -> Option<&game_environment::Season> {
        self.environment
            .as_ref()
            .and_then(|owner| owner.season_at(self.core.simulated_millis(), legacy_tick))
    }

    /// Boundary owners observe the conditions immediately before settlement.
    pub(super) fn settlement_season(&self, legacy_tick: u64) -> Option<&game_environment::Season> {
        self.environment.as_ref().and_then(|owner| {
            owner.season_at(self.core.simulated_millis().saturating_sub(1), legacy_tick)
        })
    }

    /// Inspect bounded local farming opportunities without selecting or executing them.
    pub fn preview_cultivation_opportunities(
        &self,
        people: &[u64],
        blocked: &[u64],
    ) -> Result<Vec<game_economy_agriculture::opportunities::Opportunity>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = &self.agriculture else {
            return Ok(Vec::new());
        };
        let mut busy = self
            .blocked_with_rest(blocked)
            .as_deref()
            .unwrap_or(blocked)
            .to_vec();
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
        let tick = self.core.physical.tick();
        let round = self.core.decision_time_key();
        game_economy_agriculture::opportunities::discover(
            owner,
            &self.core.people,
            people,
            &busy,
            self.current_season(tick),
            self.core.seed,
            round,
        )
        .map_err(str::to_owned)
    }

    /// Stage individually chosen soil work with the same physical boundary.
    pub(super) fn plan_soil_amendment(
        &self,
        occupied: &[u64],
        cultivation: &mut Option<game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        actions: &mut Vec<game_lots::Transaction>,
    ) -> Result<Option<game_economy_agriculture::amendment::Plan>, String> {
        let Some((agriculture, soil, amendment)) = self.agriculture.as_ref().and_then(|farm| {
            farm.soil
                .as_ref()
                .and_then(|soil| soil.amendment.as_ref().map(|owner| (farm, soil, owner)))
        }) else {
            return Ok(None);
        };
        let farm = cultivation
            .as_mut()
            .ok_or("soil improvement needs a cultivation plan")?;
        let mut busy = occupied.to_vec();
        busy.par_extend(farm.actors.par_iter().copied());
        busy.par_extend(farm.requesters.par_iter().copied());
        if let Some(irrigation) = irrigation {
            busy.par_extend(irrigation.parallel_actors());
        }
        if let Some(family) = &self.family {
            busy.par_extend(game_economy_work::underage_at(
                family,
                Some(self.core.simulated_millis()),
            ));
        }
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
        let mut fields: Vec<_> = farm.changes.par_iter().map(|row| row.0).collect();
        fields.par_sort_unstable();
        fields.dedup();
        // Irrigation also allocates IDs for failed attempts which have no
        // physical action. Leave room for all of them before assigning soil IDs.
        let last = actions
            .par_iter()
            .map(|action| action.id)
            .chain(farm.changes.par_iter().map(|row| row.1))
            .max()
            .unwrap_or(0);
        let reserved = u64::try_from(irrigation.map_or(0, |plan| plan.len()))
            .map_err(|_| "soil action overflow")?;
        let base = last
            .checked_add(reserved)
            .and_then(|id| id.checked_add(1))
            .ok_or("soil action overflow")?;
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = amendment.prepare(
            game_economy_agriculture::amendment::Context {
                soil,
                agriculture,
                people: &self.core.people,
                busy: &busy,
                nonstock: &[],
                occupied_fields: &fields,
                claims: claims.as_ref(),
                opinions: self.opinions.as_ref(),
                view: &view,
                max_hunger: self.core.rules.max_work_hunger,
                seed: self.core.seed,
                tick,
                manual: None,
            },
            base,
        )?;
        actions.par_extend(plan.actions().cloned());
        farm.uses.par_extend(plan.uses());
        Ok(Some(plan))
    }

    pub(super) fn plan_irrigation(
        &self,
        occupied: &[u64],
        cultivation: &mut Option<game_economy_agriculture::Plan>,
        actions: &mut Vec<game_lots::Transaction>,
    ) -> Result<Option<game_economy_agriculture::irrigation::Plan>, String> {
        let Some(owner) = &self.irrigation else {
            return Ok(None);
        };
        let agriculture = self
            .agriculture
            .as_ref()
            .ok_or("irrigation requires agriculture")?;
        let farm = cultivation
            .as_mut()
            .ok_or("irrigation needs a cultivation plan")?;
        farm.gate_dry_growth(agriculture, owner);
        let base = farm
            .changes
            .par_iter()
            .map(|row| row.1)
            .chain(actions.par_iter().map(|row| row.id))
            .max()
            .map_or(Ok(1), |id| {
                id.checked_add(1).ok_or("irrigation action overflow")
            })?;
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        if let Some(family) = &self.family {
            busy.par_extend(game_economy_work::underage_at(
                family,
                Some(self.core.simulated_millis()),
            ));
        }
        busy.par_extend(farm.actors.par_iter().copied());
        busy.par_extend(farm.requesters.par_iter().copied());
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
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = owner.prepare(
            game_economy_agriculture::irrigation::Context {
                agriculture,
                people: &self.core.people,
                busy: &busy,
                claims: claims.as_ref(),
                opinions: self.opinions.as_ref(),
                view: &view,
                max_hunger: self.core.rules.max_work_hunger,
                manual: None,
                seed: self.core.seed,
                tick,
            },
            base,
        )?;
        actions.par_extend(plan.actions().cloned());
        farm.uses.par_extend(plan.uses());
        Ok(Some(plan))
    }
    pub(super) fn stage_cultivation(
        &self,
        plan: Option<game_economy_agriculture::Plan>,
        events: &Events,
        care_uses: &[game_economy_claims::Use],
    ) -> Result<
        (
            Option<game_economy_agriculture::Agriculture>,
            CultivationClaims,
        ),
        String,
    > {
        let Some(plan) = plan else {
            let claims = self
                .claims
                .as_ref()
                .filter(|_| !care_uses.is_empty())
                .map(|owner| {
                    let mut claims = owner.clone();
                    let mut opinions = self.opinions.clone();
                    game_economy_claims::publish(
                        &mut claims,
                        care_uses,
                        events,
                        &self.core.people,
                        &mut opinions,
                        None,
                    );
                    (claims, opinions)
                });
            return Ok((None, claims));
        };
        let owner = self
            .agriculture
            .as_ref()
            .ok_or("missing cultivation owner")?;
        let claims = if let Some(claims) = &self.claims {
            let mut claims = claims.clone();
            let mut opinions = self.opinions.clone();
            let uses = (!care_uses.is_empty()).then(|| {
                let mut uses = plan.uses.clone();
                uses.par_extend(care_uses.par_iter().cloned());
                uses
            });
            game_economy_claims::publish(
                &mut claims,
                uses.as_deref().unwrap_or(&plan.uses),
                events,
                &self.core.people,
                &mut opinions,
                None,
            );
            let field_uses: Vec<_> = plan
                .changes
                .par_iter()
                .zip(plan.actors.par_iter())
                .filter_map(|(&(field, action, phase, _), &person)| {
                    events
                        .physical_actions
                        .binary_search_by_key(&action, |row| row.id)
                        .ok()
                        .filter(|&index| events.physical_actions[index].accepted)
                        .map(|_| game_economy_claims::FieldUse {
                            person,
                            field,
                            harvest: phase == game_economy_agriculture::Phase::Fallow,
                        })
                })
                .collect();
            let mut fields: Vec<_> = owner
                .fields
                .par_iter()
                .map(|field| (field.id, field.region))
                .collect();
            fields.par_sort_unstable();
            game_economy_claims::publish_field_uses(
                &mut claims,
                &field_uses,
                &fields,
                &self.core.people,
                events.tick,
                None,
                None,
            );
            game_economy_claims::publish_cultivated_field_claims(&mut claims, &field_uses);
            if let Some(opinions) = &mut opinions {
                let witnessed: Vec<_> = claims
                    .field_events
                    .par_iter()
                    .map(|event| (event.claimant, event.person))
                    .collect();
                opinions.learn_field_affinity(&witnessed);
            }
            Some((claims, opinions))
        } else {
            None
        };
        let planting_time = owner
            .growth_clock
            .is_some()
            .then(|| self.core.simulated_millis());
        Ok((Some(plan.publish_at(owner, events, planting_time)?), claims))
    }
    pub(super) fn plan_cultivation(
        &self,
        occupied: &[u64],
        external: &[u64],
        base: u64,
    ) -> Result<Option<game_economy_agriculture::Plan>, String> {
        let Some(owner) = &self.agriculture else {
            return Ok(None);
        };
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        if let Some(family) = &self.family {
            busy.par_extend(game_economy_work::underage_at(
                family,
                Some(self.core.simulated_millis()),
            ));
        }
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
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let agreement_evidence = self.farm_agreement_evidence(tick)?;
        let context = game_economy_agriculture::Context {
            agriculture: owner,
            agreement_evidence,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            contact_blocked: occupied,
            catalog: self.core.physical.catalog(),
            view: &view,
            lot_instant: self.core.physical.lot_instant(tick),
            tick,
            manual: None,
            manual_response: None,
            manual_withdrawal: None,
            reserved_field: None,
            season: self.settlement_season(tick),
        };
        let selected = self
            .schedule
            .as_ref()
            .map_or(&[][..], |schedule| schedule.pending_cultivation.as_slice());
        let choice_people: Vec<_> = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_ref())
            .map_or_else(Vec::new, |people| {
                people.par_iter().map(|person| person.id).collect()
            });
        let mut selected_blocked = self
            .blocked_with_rest(external)
            .as_deref()
            .unwrap_or(external)
            .to_vec();
        selected_blocked.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        selected_blocked.par_sort_unstable();
        selected_blocked.dedup();
        Ok(Some(context.prepare_choices(
            base,
            selected,
            &choice_people,
            &selected_blocked,
        )?))
    }
}
