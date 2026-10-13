//! Regional observation, reconciliation and autonomous work iteration.
use crate::*;
use game_core::*;
use game_ecs::{Batch, Component, Store};
use game_society::institutions::AuthorityFacts;

pub trait RealmExecution<A: Component> {
    fn runtime(&self) -> &Runtime;
    fn store(&self) -> &Store;
    fn store_mut(&mut self) -> &mut Store;
    fn now(&self) -> SimTime;
    fn presence(&self) -> impl Presence;
    fn authority(&self, actor: PersonId) -> AuthorityFacts;
    fn judgment(&self, actor: PersonId) -> JudgmentFacts<'_>;
    /// Bind the selected typed owner operation to the application's command port.
    fn execute_choice(&mut self, actor: PersonId, choice: ReviewChoice) -> Result<(), Error>;
}
fn commit_changes(store: &mut Store, plans: Vec<PreparedChange>) -> Result<(), Error> {
    let mut batch = Batch::new();
    let guard = stage_changes(plans, store, &mut batch)?;
    guard.validate(&batch)?;
    store.apply_batch(batch)
}
pub fn observe_realm<A: Component>(port: &mut impl RealmExecution<A>) -> Result<(), Error> {
    let plans = port
        .runtime()
        .prepare_observation(port.store(), port.now(), &port.presence())?;
    commit_changes(port.store_mut(), plans)?;
    let plans =
        port.runtime()
            .prepare_reconciliation(port.store(), port.now(), &port.presence())?;
    commit_changes(port.store_mut(), plans)
}
pub fn review_realm<A: Component>(port: &mut impl RealmExecution<A>) -> Result<(), Error> {
    let now = port.now();
    let plans = port.runtime().prepare_expirations(port.store(), now)?;
    commit_changes(port.store_mut(), plans)?;
    let actors = port.presence().people().collect::<Vec<_>>();
    for actor in actors {
        let plan = {
            let authority = port.authority(actor);
            let presence = port.presence();
            port.runtime().prepare_review::<A>(
                port.store(),
                actor,
                &RealmContext {
                    now,
                    authority: &authority,
                    presence: &presence,
                },
                &port.judgment(actor),
            )
        };
        let Some(mut plan) = plan else {
            continue;
        };
        while let Some(choice) = plan.next_attempt() {
            plan.receive_result(port.execute_choice(actor, choice).is_ok());
        }
        insert_requests(port.store_mut(), actor, plan.reconsideration)?;
    }
    Ok(())
}
impl Runtime {
    pub fn prepare_reconciliation(
        &self,
        store: &Store,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Vec<PreparedChange>, Error> {
        let ledger = Ledger::new(store);
        ledger
            .reconciliation_ids()
            .into_iter()
            .map(|id| ledger.prepare_reconciliation(id, now, presence))
            .collect()
    }
    pub fn prepare_expirations(
        &self,
        store: &Store,
        now: SimTime,
    ) -> Result<Vec<PreparedChange>, Error> {
        let ledger = Ledger::new(store);
        self.expiry_ids(store, now)
            .into_iter()
            .map(|id| ledger.prepare_expire(id, now))
            .collect()
    }
}
