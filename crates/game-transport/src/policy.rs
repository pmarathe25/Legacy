//! Departure and willing participation use only this owner's rows and physical facts.
use crate::*;
impl Runtime {
    pub fn can_depart(
        &self,
        store: &game_ecs::Store,
        bodies: &impl BodyFacts,
        actor: PersonId,
    ) -> bool {
        if group(store, actor).is_some() || self.busy(store, actor) {
            return false;
        }
        if !bodies
            .body(actor)
            .is_some_and(|body| body.can_walk && body.cargo <= body.capacity)
        {
            return false;
        }
        let Ok(plan) = self.component_plan(store, bodies, actor) else {
            return false;
        };
        let linked = self.component(store, actor);
        plan.unable.is_disjoint(&linked) && plan.overloaded.is_disjoint(&linked)
    }
    pub fn body_load(
        &self,
        store: &game_ecs::Store,
        bodies: &impl BodyFacts,
        actor: PersonId,
    ) -> Quantity {
        let Ok(plan) = self.component_plan(store, bodies, actor) else {
            return Quantity(0);
        };
        Quantity(
            plan.shares
                .iter()
                .filter(|((_, bearer), _)| *bearer == actor)
                .map(|(_, amount)| amount.0)
                .sum(),
        )
    }
    /// Joining retains active offers and existing bearers who can still hold.
    pub fn join_bearers(
        &self,
        store: &game_ecs::Store,
        bodies: &impl BodyFacts,
        passenger: PersonId,
        now: SimTime,
    ) -> BTreeSet<PersonId> {
        let mut bearers = offer_inbox(store, passenger)
            .into_iter()
            .flat_map(|inbox| inbox.by_bearer.values())
            .filter(|offer| offer.expires > now)
            .map(|offer| offer.bearer)
            .collect::<BTreeSet<_>>();
        if let Some(existing) = group(store, passenger) {
            bearers.extend(
                existing
                    .bearers
                    .iter()
                    .filter(|actor| bodies.body(**actor).is_some_and(|body| body.can_hold))
                    .copied(),
            );
        }
        bearers
    }
}
impl Lift {
    /// An escaping passenger retains its action while the consenting bearers lift.
    pub fn participants(&self, passenger_escaping: bool) -> Vec<PersonId> {
        let mut actors = self.bearers.iter().copied().collect::<Vec<_>>();
        if !passenger_escaping {
            actors.push(self.passenger);
        }
        actors
    }
}
#[cfg(test)]
#[path = "policy/tests.rs"]
mod tests;
