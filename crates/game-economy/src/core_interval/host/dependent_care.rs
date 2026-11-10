//! Staged care, including real food transfers to dependents.
use super::*;

impl CoreHost {
    pub(super) fn plan_dependent_care(
        &self,
        occupied: &[u64],
    ) -> Result<Option<game_economy_work::care::Plan>, String> {
        let Some((care, family)) = self.dependent_care.as_ref().zip(self.family.as_ref()) else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
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
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| (person.id, person.region, person.health, person.hunger))
            .collect();
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        game_economy_work::care::Context {
            care,
            family,
            people: &people,
            rules: &self.core.rules,
            goods: self.core.physical.goods(),
            view: &view,
            travel: self.core.travel.as_ref(),
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            access: game_economy_work::care::Access {
                contact: &|_, _| true,
                local_stock: &|_| true,
            },
            eating: &[],
            seed: self.core.seed,
            tick,
        }
        .prepare(1)
        .map(Some)
    }
}
