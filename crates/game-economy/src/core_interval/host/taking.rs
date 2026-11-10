//! Individual taking proposals settle against actual possessions.
use super::*;

pub(super) struct TakingFrame {
    pub owner: game_economy_taking::Taking,
    pub plan: game_economy_taking::Plan,
}

impl CoreHost {
    pub(super) fn plan_taking(
        &self,
        occupied: &[u64],
        base: u64,
    ) -> Result<Option<TakingFrame>, String> {
        let Some(owner) = &self.taking else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
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
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let plan = game_economy_taking::Context {
            owner,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            locations: &locations,
            busy: &busy,
            blocked: occupied,
            can_touch: &|_, _| true,
            max_hunger: self.core.rules.max_work_hunger,
            manual: None,
            action_base: base,
            seed: self.core.seed,
            tick,
        }
        .prepare()?;
        Ok(Some(TakingFrame {
            owner: owner.clone(),
            plan,
        }))
    }
    pub(super) fn stage_taking_reactions(
        &self,
        owner: &mut game_economy_taking::Taking,
    ) -> Vec<game_opinions::PersonalChange> {
        let mut changes = owner.personal_changes(&owner.events);
        if !owner.witnesses.is_empty() && !owner.events.is_empty() {
            let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
            let can_observe = |observer, actor| {
                let find = |id| {
                    self.core
                        .people
                        .binary_search_by_key(&id, |person| person.id)
                        .ok()
                        .map(|index| &self.core.people[index])
                };
                find(observer).zip(find(actor)).is_some_and(|(a, b)| {
                    a.health > 0
                        && b.health > 0
                        && locations
                            .region((a.id, a.region), (b.id, b.region))
                            .is_some()
                })
            };
            let events = std::mem::take(&mut owner.events);
            changes.par_extend(owner.observe_witnesses(&events, &can_observe));
            owner.events = events;
        }
        changes
    }
    /// Resolve communications from opening memories and actual contacts.
    pub(super) fn stage_taking_contacts(
        &self,
        owner: &mut game_economy_taking::Taking,
        contacts: &[(u64, u64)],
        tick: u64,
    ) -> Vec<game_opinions::PersonalChange> {
        let Some(previous) = self.taking.as_ref() else {
            return Vec::new();
        };
        if previous.report_policies.is_empty() && previous.complaints.is_empty() {
            return Vec::new();
        }
        let needs_feelings = previous
            .report_policies
            .par_iter()
            .any(|policy| !policy.willing && policy.pressure_threshold.is_some())
            || previous.receptions.par_iter().any(|reception| {
                matches!(
                    reception.stance,
                    game_economy_taking::Stance::TrustAtLeast(_)
                )
            })
            || previous
                .complaint_preferences
                .par_iter()
                .any(|preference| !preference.willing && preference.pressure_threshold.is_some());
        let feelings = self
            .opinions
            .as_ref()
            .filter(|_| needs_feelings)
            .map(game_opinions::Opinions::index);
        let changes = if contacts.is_empty() {
            Vec::new()
        } else {
            owner.receive_reports(previous, contacts, tick, &|listener, speaker| {
                feelings.as_ref().map_or((0, 0, 0), |index| {
                    let (fear, obligation) = index.feelings(speaker, listener);
                    (index.trust(listener, speaker), fear, obligation)
                })
            })
        };
        owner.advance_complaints(previous, contacts, tick, &|recipient, author| {
            feelings.as_ref().map_or(0, |index| {
                let (fear, obligation) = index.feelings(recipient, author);
                fear.saturating_add(obligation)
            })
        });
        changes
    }

    /// Request consideration of an account the author personally remembers.
    /// The recipient remains free to decide during a later encounter.
    pub fn request_taking_inquiry(
        &mut self,
        author: u64,
        recipient: u64,
        witness: u64,
        actor: u64,
        lifetime: u64,
    ) -> Result<(), String> {
        if author == recipient
            || witness == recipient
            || lifetime == 0
            || [author, recipient, witness].into_iter().any(|id| {
                self.core
                    .people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .is_none_or(|index| self.core.people[index].health == 0)
            })
        {
            return Err(
                "inquiry requires distinct living participants and a positive lifetime".into(),
            );
        }
        let mut owner = self.taking.clone().ok_or("taking is disabled")?;
        let account = owner
            .complaint_account(author, actor)
            .ok_or("person has no remembered account")?;
        let now = self.core.physical.tick();
        let expires_at = now
            .checked_add(lifetime)
            .ok_or("inquiry deadline overflow")?;
        if owner.complaints.iter().any(|row| {
            row.recipient == recipient
                && row.decision == game_economy_taking::ComplaintDecision::Pending
        }) {
            return Err("recipient already has a pending inquiry".into());
        }
        let previous_case = owner
            .complaints
            .iter()
            .find(|row| row.recipient == recipient)
            .and_then(|row| {
                row.answer
                    .as_ref()
                    .and_then(|answer| {
                        answer.assessment.map(|assessment| {
                            std::sync::Arc::new(game_economy_taking::PersonalCase {
                                person: recipient,
                                account: row.account,
                                assessment,
                                remembered_at: now,
                            })
                        })
                    })
                    .or_else(|| row.previous_case.clone())
            });
        let complaints = owner.complaints_mut();
        complaints.retain(|row| row.recipient != recipient);
        complaints.push(game_economy_taking::Complaint {
            author,
            recipient,
            witness,
            account,
            created_at: now,
            expires_at,
            decision: game_economy_taking::ComplaintDecision::Pending,
            decided_at: None,
            previous_case,
            answer: None,
        });
        complaints.par_sort_unstable_by_key(|row| row.recipient);
        self.taking = Some(owner);
        Ok(())
    }
}
