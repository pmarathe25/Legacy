//! Personal carrying projections use the owner's actual native rows.
use crate::*;

impl Runtime {
    pub fn view(
        &self,
        store: &game_ecs::Store,
        bodies: &impl BodyFacts,
        actor: PersonId,
        now: SimTime,
    ) -> Option<View> {
        let body = bodies.body(actor)?;
        Some(View {
            mass: body.mass,
            carried_by: group(store, actor)
                .map_or_else(Vec::new, |group| group.bearers.iter().copied().collect()),
            passengers: groups(store)
                .filter(|(_, group)| group.bearers.contains(&actor))
                .map(|(_, group)| group.passenger)
                .collect(),
            body_load: self.body_load(store, bodies, actor),
            offers: offer_inboxes(store)
                .flat_map(|(_, offers)| offers.by_bearer.values())
                .filter(|offer| {
                    (offer.passenger == actor || offer.bearer == actor) && offer.expires > now
                })
                .cloned()
                .collect(),
            responses: self
                .responses
                .iter()
                .filter(|response| response.passenger == actor || response.bearer == actor)
                .cloned()
                .collect(),
        })
    }
}
