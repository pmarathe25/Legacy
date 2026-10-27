use super::*;
pub(super) struct Guard {
    body: Option<Body>,
    plan: Option<game_provisioning::Plan>,
    failures: u64,
    action: Option<u64>,
}
impl Guard {
    pub(super) fn capture(tx: &Transaction<'_>, person: PersonId) -> Result<Self> {
        let mind = &tx.get::<Minds>()?.people[&person];
        Ok(Self {
            body: tx.get::<Bodies>()?.people.get(&person).copied(),
            plan: mind.plan.clone(),
            failures: mind.failures,
            action: tx.get::<Actions>()?.0.get(&person).map(|a| a.id),
        })
    }
    fn current(&self, tx: &Transaction<'_>, person: PersonId) -> Result<bool> {
        let state = tx.get::<Minds>()?;
        Ok(game_diagnostics::interaction_checks!(
            state.people.get(&person).is_some_and(|m| m.plan == self.plan && m.failures == self.failures) => "interaction.guard.mind",
            state.cohort_urgency.get(&person) == Some(&0) => "interaction.guard.urgency",
            !choices::controlled(tx, person)? => "interaction.guard.control",
            tx.get::<Bodies>()?.people.get(&person).copied() == self.body => "interaction.guard.body",
        ))
    }
}
type Ordered = (
    choices::ReviewCursor,
    game_utility::Decision,
    game_choice::Proposal,
    bool,
    Guard,
);
pub(super) enum Deferred {
    Finish(bool),
    Ordered(Box<Ordered>),
    Meal(Selected, bool),
    Refused(Selected),
}
pub(super) fn refresh(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    game_diagnostics::interaction_profile!(count "interaction.refresh.calls", 1);
    tx.call(game_choice::InvalidateFramePerson(person))?;
    if tx.get::<Minds>()?.people.contains_key(&person)
        && tx
            .get::<Bodies>()?
            .people
            .get(&person)
            .is_some_and(|b| b.alive && b.health > 0)
    {
        choices::reconsider(tx, person, true)?;
    }
    Ok(())
}
/// Settle every admitted physical wave first. Then urgent evidence takes precedence
/// over the original cohort order; each remaining actor is resumed at most once.
pub(super) fn settle(
    tx: &mut Transaction<'_>,
    people: &[PersonId],
    mut pending: BTreeMap<PersonId, Deferred>,
) -> Result<()> {
    while !pending.is_empty() {
        let urgency = &tx.get::<Minds>()?.cohort_urgency;
        let person = people
            .iter()
            .find(|p| pending.contains_key(p) && urgency.get(p).is_some_and(|n| *n > 0))
            .or_else(|| people.iter().find(|p| pending.contains_key(p)))
            .copied()
            .unwrap();
        match pending.remove(&person).unwrap() {
            Deferred::Meal(row, admitted) => {
                fallback(tx, row, admitted)?;
                continue;
            }
            Deferred::Finish(fresh) => {
                if fresh
                    || tx
                        .get::<Minds>()?
                        .cohort_urgency
                        .get(&person)
                        .is_some_and(|n| *n > 0)
                {
                    game_diagnostics::interaction_profile!(count if fresh { "interaction.refresh.finish_requested" } else { "interaction.refresh.finish_urgent" }, 1);
                    refresh(tx, person)?;
                }
            }
            Deferred::Refused(row) => {
                game_diagnostics::interaction_profile!(count "interaction.review.retained_refusal", 1);
                choices::record_failure(tx, person, row.decision.id)?;
                choices::continue_review(tx, row.review)?;
            }
            Deferred::Ordered(row) => {
                let (review, decision, proposal, continues, guard) = *row;
                let valid = guard.current(tx, person)?
                    && !choices::stale_selection(tx, &review, &proposal)?;
                let interrupted = if tx.get::<Minds>()?.paused_reviews.contains_key(&person) {
                    tx.call(clock::FinishPausedReview(person))?
                } else {
                    true
                };
                if valid && !interrupted && guard.current(tx, person)? {
                    if tx.get::<Actions>()?.0.get(&person).map(|a| a.id) != guard.action {
                        game_diagnostics::interaction_profile!(count "interaction.deferred.changed_action", 1);
                        choices::continue_review(tx, review)?;
                    } else {
                        choices::apply_and_continue(tx, review, decision, proposal, continues)?;
                    }
                } else {
                    game_diagnostics::interaction_profile!(count "interaction.deferred.invalidated", 1);
                    game_diagnostics::interaction_profile!(count if interrupted { "interaction.refresh.ordered_interrupted" } else { "interaction.refresh.ordered_invalidated" }, 1);
                    refresh(tx, person)?;
                }
            }
        }
        finish(tx, person)?;
    }
    Ok(())
}
