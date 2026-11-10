//! Boundary cultivation, staged with the common physical generation.
use super::*;
type CultivationClaims = Option<(game_economy_claims::Claims, Option<game_opinions::Opinions>)>;

impl CoreHost {
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
            .max()
            .map_or(Ok(1), |id| {
                id.checked_add(1).ok_or("irrigation action overflow")
            })?;
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
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
    ) -> Result<
        (
            Option<game_economy_agriculture::Agriculture>,
            CultivationClaims,
        ),
        String,
    > {
        let Some(plan) = plan else {
            return Ok((None, None));
        };
        let owner = self
            .agriculture
            .as_ref()
            .ok_or("missing cultivation owner")?;
        let claims = if let Some(claims) = &self.claims {
            let mut claims = claims.clone();
            let mut opinions = self.opinions.clone();
            game_economy_claims::publish(
                &mut claims,
                &plan.uses,
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
            season: None,
        };
        Ok(Some(context.prepare(1)?))
    }
}
