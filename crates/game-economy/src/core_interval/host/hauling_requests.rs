//! Personal requests for hauling, without binding the carrier.
use super::*;

impl CoreHost {
    pub fn request_hauling_autonomously(&mut self, blocked: &[u64]) -> Result<Vec<u64>, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.hauling.as_ref() else {
            return Ok(Vec::new());
        };
        if owner.asking.is_empty() && owner.requests.is_empty() {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before hauling requests".into());
        }
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
        let mut next = owner.clone();
        next.requests = owner
            .requests
            .par_iter()
            .filter(|request| request.expires_at >= tick)
            .cloned()
            .collect();
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let actors = next.ask(&self.core.people, &busy, &locations, tick, None)?;
        self.hauling = Some(next);
        Ok(actors)
    }
}
