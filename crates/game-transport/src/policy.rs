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

/// Sparse anchored descendants for receiving movement reactions. Body reads
/// and unrelated actor rows are unnecessary; validation makes these links
/// reciprocal with Spatial's actual carried locations.
pub fn carrier_family(store: &game_ecs::Store, actor: PersonId) -> Vec<PersonId> {
    let mut children = BTreeMap::<PersonId, Vec<PersonId>>::new();
    for (_, group) in groups(store) {
        children
            .entry(group.anchor)
            .or_default()
            .push(group.passenger);
    }
    let mut result = Vec::new();
    let mut pending = vec![actor];
    let mut seen = BTreeSet::new();
    while let Some(actor) = pending.pop() {
        if !seen.insert(actor) {
            continue;
        }
        result.push(actor);
        if let Some(children) = children.get(&actor) {
            pending.extend(children);
        }
    }
    result
}

pub trait GrowingBodyFacts {
    fn body_at(&self, actor: PersonId, at: SimTime) -> Option<Body>;
}
impl Runtime {
    pub fn growth_boundary(
        &self,
        store: &game_ecs::Store,
        now: SimTime,
        limit: SimTime,
        facts: &impl GrowingBodyFacts,
    ) -> Result<SimTime, Error> {
        let elapsed = limit.0.checked_sub(now.0).ok_or(Error::Overflow)?;
        let moving = groups(store)
            .filter(|(_, group)| {
                matches!(
                    game_spatial::resolve(store, game_ecs::StableId(group.anchor.0)),
                    Ok(Location::InTransit { .. })
                )
            })
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        if moving.is_empty() || elapsed == 0 {
            return Ok(limit);
        }
        let linked = super::changes::connected(store, &moving);
        let rows = groups(store)
            .filter(|(_, group)| {
                linked.contains(&group.passenger) || !group.bearers.is_disjoint(&linked)
            })
            .map(|(_, group)| group)
            .collect::<Vec<_>>();
        struct At<'a, F> {
            facts: &'a F,
            at: SimTime,
        }
        impl<F: GrowingBodyFacts> BodyFacts for At<'_, F> {
            fn body(&self, actor: PersonId) -> Option<Body> {
                self.facts.body_at(actor, self.at)
            }
        }
        let overloaded = |elapsed| -> Result<bool, Error> {
            let plan = capacity_plan_rows(
                rows.iter().copied(),
                &At {
                    facts,
                    at: SimTime(now.0 + elapsed),
                },
                None,
            )?;
            Ok(!plan.overloaded.is_disjoint(&moving))
        };
        if !overloaded(elapsed)? {
            return Ok(limit);
        }
        let (mut lower, mut upper) = (1, elapsed);
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            if overloaded(middle)? {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        Ok(SimTime(now.0 + lower))
    }
}
