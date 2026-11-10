//! Personal offers and physical exchanges.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreMarketResults {
    pub actors: Vec<u64>,
    pub offers: Vec<u64>,
}

impl CoreHost {
    pub(super) fn stage_market_boundary(&self) -> Option<game_market::Market> {
        let owner = self.market.as_ref()?;
        let tick = self.core.physical.tick();
        if owner.events.par_iter().all(|event| event.tick == tick) {
            return None;
        }
        let mut next = owner.clone();
        next.events = owner
            .events
            .par_iter()
            .filter(|event| event.tick == tick)
            .cloned()
            .collect();
        Some(next)
    }
    fn exchange_busy(&self, blocked: &[u64]) -> Vec<u64> {
        let mut busy = self
            .blocked_with_rest(blocked)
            .unwrap_or_else(|| blocked.to_vec());
        busy.par_extend(
            self.core
                .people
                .par_iter()
                .filter(|person| {
                    person.controlled
                        || person.health == 0
                        || person.hunger > self.core.rules.max_work_hunger
                })
                .map(|person| person.id),
        );
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
    fn exchange_time(&self) -> Result<u64, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before exchange".into());
        }
        Ok(tick)
    }
    /// Make personal offers without reserving or asserting possession of the offered goods.
    pub fn propose_personal_exchanges(
        &mut self,
        proposals: &[game_market::Proposal],
    ) -> Result<Vec<u64>, String> {
        let tick = self.exchange_time()?;
        let owner = self
            .market
            .as_ref()
            .ok_or("personal exchange is disabled")?;
        if proposals.par_iter().any(|proposal| {
            let participants = [proposal.proposer, proposal.recipient].map(|id| {
                self.core
                    .people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .map(|i| &self.core.people[i])
            });
            match participants {
                [Some(a), Some(b)] => {
                    a.id == b.id
                        || a.health == 0
                        || b.health == 0
                        || a.region.is_none()
                        || a.region != b.region
                }
                _ => true,
            }
        }) {
            return Err("exchange requires local living people".into());
        }
        let (next, ids) = owner.propose_batch(proposals, tick)?;
        let people: HashSet<_> = self.core.people.par_iter().map(|p| p.id).collect();
        let goods: HashSet<_> = self
            .core
            .physical
            .goods()
            .par_iter()
            .map(|g| g.id)
            .collect();
        next.validate(&people, &goods, tick)?;
        self.market = Some(next);
        Ok(ids)
    }
    /// A proposer can withdraw, and a recipient can decline, without a physical transfer.
    pub fn decline_personal_exchanges(
        &mut self,
        decisions: &[game_market::Decision],
    ) -> Result<(), String> {
        let tick = self.exchange_time()?;
        if decisions.par_iter().any(|decision| {
            decision.outcome == game_market::Outcome::Accepted
                || self
                    .core
                    .people
                    .binary_search_by_key(&decision.actor, |person| person.id)
                    .ok()
                    .is_none_or(|i| self.core.people[i].health == 0)
        }) {
            return Err("invalid personal exchange decision".into());
        }
        let owner = self
            .market
            .as_ref()
            .ok_or("personal exchange is disabled")?;
        let draft = owner.stage_decisions(decisions, tick, false)?;
        let mut next = owner.clone();
        draft.commit(&mut next);
        self.market = Some(next);
        Ok(())
    }
    /// Let recipients independently evaluate outstanding offers and exchange actual possessions atomically.
    pub fn respond_to_personal_exchanges(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreMarketResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self
            .market
            .as_ref()
            .filter(|owner| !owner.offers.is_empty() && !owner.policies.is_empty())
        else {
            return Ok(CoreMarketResults::default());
        };
        let tick = self.exchange_time()?;
        let mut busy = self.exchange_busy(blocked);
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|p| (p.id, p.region, p.health, p.hunger))
            .collect();
        let view = self.core.physical.view(tick)?;
        let cargo =
            game_lot_projection::cargo_for_choices(&view, self.core.rules.carrying_capacity)?;
        let context = game_economy_exchange::Context {
            market: Some(owner),
            view: &view,
            capacity: self.core.rules.carrying_capacity,
            cargo: Some(&cargo),
            people: &people,
            goods: self.core.physical.goods(),
            opinions: self.opinions.as_ref(),
            endpoint: None,
            together: None,
        };
        let Some(plan) = context.trade(&mut busy, 1)? else {
            return Ok(CoreMarketResults::default());
        };
        let actors = plan.actors();
        if plan.actions().is_empty() {
            return Ok(CoreMarketResults {
                actors,
                offers: vec![],
            });
        }
        let actions = plan.interval_actions()?;
        let physical = self
            .core
            .physical
            .prepare_interval_transfer(&actions, self.core.seed)?;
        let decisions = plan.interval_decisions(owner, tick, physical.transactions())?;
        let Some(decisions) = decisions else {
            return Ok(CoreMarketResults {
                actors,
                offers: vec![],
            });
        };
        let offers: Vec<_> = decisions
            .events()
            .par_iter()
            .map(|event| event.offer)
            .collect();
        let mut contacts: Vec<_> = owner
            .offers
            .par_iter()
            .filter(|offer| offers.binary_search(&offer.id).is_ok())
            .map(|offer| (offer.proposer, offer.recipient))
            .collect();
        contacts.par_sort_unstable();
        let health = self.stage_contact_health(None, &contacts, tick)?;
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        let mut opinions = self
            .opinions
            .as_ref()
            .filter(|owner| owner.tracks_help() && !decisions.gifts().is_empty())
            .cloned();
        if let Some(opinions) = &mut opinions {
            opinions.learn_help(decisions.gifts());
        }
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        decisions.commit(
            self.market
                .as_mut()
                .expect("checked personal exchange owner"),
        );
        if let Some(owner) = opinions {
            self.opinions = Some(owner);
        }
        if let Some(owner) = health {
            self.health = Some(owner);
        }
        if let Some(owner) = tools {
            self.tool_wear = Some(owner);
        }
        self.work_checked_revision = None;
        Ok(CoreMarketResults { actors, offers })
    }
    /// Let each person choose offers from locally visible possessions and personal values.
    pub fn propose_personal_exchanges_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreMarketResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.market.as_ref().filter(|owner| {
            owner
                .policies
                .par_iter()
                .any(|policy| policy.enabled && (policy.propose || policy.request_help))
        }) else {
            return Ok(CoreMarketResults::default());
        };
        let tick = self.exchange_time()?;
        let mut busy = self.exchange_busy(blocked);
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|p| (p.id, p.region, p.health, p.hunger))
            .collect();
        let view = self.core.physical.view(tick)?;
        let cargo =
            game_lot_projection::cargo_for_choices(&view, self.core.rules.carrying_capacity)?;
        let context = game_economy_exchange::Context {
            market: Some(owner),
            view: &view,
            capacity: self.core.rules.carrying_capacity,
            cargo: Some(&cargo),
            people: &people,
            goods: self.core.physical.goods(),
            opinions: self.opinions.as_ref(),
            endpoint: None,
            together: None,
        };
        let Some(draft) = context.proposals(&mut busy)? else {
            return Ok(CoreMarketResults::default());
        };
        let actors = draft.actors.clone();
        let old = owner.next_id;
        let mut next = owner.clone();
        draft.commit(&mut next);
        let offers = (old..next.next_id).into_par_iter().collect();
        self.market = Some(next);
        Ok(CoreMarketResults { actors, offers })
    }
}
