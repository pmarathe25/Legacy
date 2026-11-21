//! Personal proposal and acceptance choices for the limited host.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreSocialResults {
    /// Sorted people whose personal choices used this decision instant.
    pub actors: Vec<u64>,
    pub agreements: Vec<u64>,
}

impl CoreHost {
    /// A living recipient can notice whether the agreed delivery reached them.
    /// This observation is private and leaves the cause and its interpretation open.
    pub(super) fn learn_agreement_outcomes(
        people: &[game_economy_person::Person],
        opinions: Option<&mut game_opinions::Opinions>,
        owner: &game_agreements::Agreements,
    ) {
        use game_agreements::EventKind::{Defaulted, Fulfilled};

        let Some(opinions) = opinions else { return };
        let observed: Vec<_> = owner
            .events
            .par_iter()
            .filter_map(|event| {
                if !matches!(event.kind, Defaulted | Fulfilled) {
                    return None;
                }
                let slot = owner
                    .contracts
                    .binary_search_by_key(&event.contract, |row| row.id)
                    .ok()?;
                let contract = &owner.contracts[slot];
                let creditor = people
                    .binary_search_by_key(&contract.creditor, |row| row.id)
                    .ok()?;
                (people[creditor].health > 0).then(|| (contract.clone(), event.clone()))
            })
            .collect();
        if !observed.is_empty() {
            let (contracts, events): (Vec<_>, Vec<_>) = observed.into_iter().unzip();
            let earlier = std::mem::take(&mut opinions.events);
            opinions.advance_validated(&contracts, &events, events[0].tick);
            let later = std::mem::replace(&mut opinions.events, earlier);
            opinions.events.extend(later);
        }
    }

    pub(super) fn publish_agreements(&mut self, owner: game_agreements::Agreements) {
        self.agreement_deadlines = owner
            .time_unit_millis
            .map(|_| game_agreements::runtime::Runtime::compile(&owner));
        self.agreements = Some(owner);
    }

    pub(super) fn next_agreement_expiry(&self) -> Option<u64> {
        self.agreement_deadlines
            .as_ref()?
            .next_expiration_after(self.core.simulated_millis())
    }

    pub(super) fn expire_due_agreements(&mut self) -> Vec<game_agreements::Event> {
        if self.next_agreement_expiry() != Some(0) {
            return Vec::new();
        }
        let owner = self
            .agreements
            .as_mut()
            .expect("compiled agreement deadlines");
        let due = u64::try_from(self.core.simulated_millis()).unwrap_or(u64::MAX);
        let instant = game_agreements::Instant {
            now: due,
            evidence_tick: self.core.physical.tick(),
        };
        self.agreement_deadlines
            .as_mut()
            .expect("compiled agreement deadlines")
            .advance_at(owner, instant);
        Self::learn_agreement_outcomes(&self.core.people, self.opinions.as_mut(), owner);
        owner.events.clone()
    }

    fn social_busy(&self, blocked: &[u64]) -> Result<Vec<u64>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before social choices".into());
        }
        let mut busy = blocked.to_vec();
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
        Ok(busy)
    }

    /// People choose offers using their own policies and witnessed experiences.
    /// A proposed quantity is not a claim that the goods are in custody.
    pub fn propose_agreements_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreSocialResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self
            .agreements
            .as_ref()
            .filter(|owner| !owner.offering.is_empty() || !owner.work_offering.is_empty())
        else {
            return Ok(CoreSocialResults::default());
        };
        let busy = self.social_busy(blocked)?;
        let instant = owner
            .instant(self.core.simulated_millis(), self.core.physical.tick())
            .map_err(str::to_owned)?;
        let locations = game_economy_person::travel::Colocation::new(None);
        let together = |a, b| {
            let find = |id| {
                self.core
                    .people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .and_then(|index| self.core.people[index].region)
            };
            find(a).is_some_and(|region| find(b) == Some(region))
        };
        let plan =
            game_economy_promises::offers::prepare(game_economy_promises::offers::Context {
                owner: Some(owner),
                people: &self.core.people,
                opinions: self.opinions.as_ref(),
                requests: self
                    .autowork
                    .as_ref()
                    .and_then(|auto| auto.requests.as_ref()),
                help_memories: &[],
                cultivation_memories: &[],
                locations: &locations,
                together: &together,
                busy: &busy,
                max_hunger: self.core.rules.max_work_hunger,
                instant,
                deadlines: self.agreement_deadlines.as_ref(),
            })?;
        let mut actors: Vec<_> = plan.actors().collect();
        actors.par_sort_unstable();
        actors.dedup();
        if plan.proposals().is_empty() {
            return Ok(CoreSocialResults::default());
        }
        let first = owner.next_id;
        let mut next = owner.clone();
        let mut opinions = self
            .opinions
            .as_ref()
            .filter(|owner| owner.tracks_promise_obligation())
            .cloned();
        game_economy_promises::offers::publish(
            &mut next,
            &plan,
            instant.evidence_tick,
            opinions.as_mut(),
        )?;
        let agreements = (first..next.next_id).collect();
        self.publish_agreements(next);
        if let Some(opinions) = opinions {
            self.opinions = Some(opinions);
        }
        Ok(CoreSocialResults { actors, agreements })
    }

    /// Each recipient independently chooses whether to accept an addressed offer.
    pub fn accept_agreements_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreSocialResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.agreements.as_ref().filter(|owner| {
            self.agreement_deadlines.as_ref().map_or_else(
                || !owner.contracts.is_empty(),
                game_agreements::runtime::Runtime::has_open_contracts,
            ) && owner
                .policies
                .par_iter()
                .any(|policy| policy.accept || policy.accept_pressure_threshold.is_some())
        }) else {
            return Ok(CoreSocialResults::default());
        };
        let busy = self.social_busy(blocked)?;
        let instant = owner
            .instant(self.core.simulated_millis(), self.core.physical.tick())
            .map_err(str::to_owned)?;
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| (person.id, person.region, person.health, person.hunger))
            .collect();
        let locations = game_economy_person::travel::Colocation::new(None);
        let plan = game_economy_promises::automatic_with_runtime(
            (owner, self.agreement_deadlines.as_ref()),
            &people,
            self.opinions.as_ref(),
            &locations,
            &busy,
            instant.now,
            None,
        );
        if plan.decisions.is_empty() {
            return Ok(CoreSocialResults::default());
        }
        let agreements = plan
            .decisions
            .par_iter()
            .map(|decision| decision.contract)
            .collect();
        let next = owner
            .decide_at(&plan.decisions, instant)
            .map_err(str::to_owned)?;
        self.publish_agreements(next);
        Ok(CoreSocialResults {
            actors: plan.actors,
            agreements,
        })
    }
}
