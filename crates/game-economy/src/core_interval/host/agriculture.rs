//! Boundary cultivation, staged with the common physical generation.
use super::*;
type CultivationClaims = Option<(game_economy_claims::Claims, Option<game_opinions::Opinions>)>;

impl CoreHost {
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
        let round =
            tick.rotate_left(21) ^ self.core.elapsed_millis().ok_or("no physical interval")?;
        game_economy_agriculture::opportunities::discover(
            owner,
            &self.core.people,
            people,
            &busy,
            self.environment
                .as_ref()
                .and_then(|environment| environment.season(tick)),
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
            busy.par_extend(game_economy_work::underage(family));
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
            busy.par_extend(game_economy_work::underage(family));
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
        Ok((Some(plan.publish(owner, events)?), claims))
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
            busy.par_extend(game_economy_work::underage(family));
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
        let context = game_economy_agriculture::Context {
            agriculture: owner,
            agreements: self.agreements.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            contact_blocked: occupied,
            goods: self.core.physical.goods(),
            view: &view,
            tick_millis: self.core.physical.tick_millis(),
            tick,
            manual: None,
            manual_response: None,
            manual_withdrawal: None,
            reserved_field: None,
            season: self
                .environment
                .as_ref()
                .and_then(|environment| environment.season(tick)),
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
