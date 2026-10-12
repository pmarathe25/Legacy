//! Dismissal retains only routes supported by the current person's knowledge.
use game_core::{AreaId, Location, SimDuration};
use std::collections::BTreeMap;

pub fn retained_route_after_dismissal(
    current: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    location: &Location,
    pending: &[AreaId],
) -> Option<Vec<AreaId>> {
    let destination = pending.last().copied()?;
    let origin = match location {
        Location::AtArea(area) => *area,
        Location::InTransit { to, .. } => *to,
        Location::StoppedOnRoad { .. } | Location::Carried { .. } => return None,
    };
    Some(
        game_characters::decisions::known_path(current, origin, destination)
            .map_or_else(|_| Vec::new(), |(path, _)| path),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dismissal_replans_only_future_legs_from_current_knowledge() {
        let current = BTreeMap::from([(AreaId(2), BTreeMap::from([(AreaId(3), SimDuration(50))]))]);
        assert_eq!(
            retained_route_after_dismissal(
                &current,
                &Location::AtArea(AreaId(1)),
                &[AreaId(2), AreaId(3)]
            ),
            Some(Vec::new())
        );
        assert_eq!(
            retained_route_after_dismissal(&current, &Location::AtArea(AreaId(2)), &[AreaId(3)]),
            Some(vec![AreaId(3)])
        );
        assert_eq!(
            retained_route_after_dismissal(
                &current,
                &Location::Carried {
                    carrier: game_core::PersonId(1)
                },
                &[AreaId(3)]
            ),
            None
        );
        assert_eq!(
            retained_route_after_dismissal(&current, &Location::AtArea(AreaId(2)), &[]),
            None
        );
    }
}

/// The only current-memory rows required to reconcile dismissed recollection.
#[derive(Clone, Debug)]
pub struct PreparedRecallLoss {
    actor: game_core::PersonId,
    routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    location: Location,
    pending: Vec<AreaId>,
    replacement: Vec<AreaId>,
}
pub fn prepare_recall_loss<A: game_ecs::Component>(
    store: &game_ecs::Store,
    actor: game_core::PersonId,
    pending: &[AreaId],
) -> Result<Option<PreparedRecallLoss>, game_core::Error> {
    let location = store
        .get::<Location>(game_ecs::StableId(actor.0))
        .ok_or(game_core::Error::Unavailable)?;
    let mind =
        game_characters::decisions::mind::<A>(store, actor).ok_or(game_core::Error::Unavailable)?;
    Ok(
        retained_route_after_dismissal(&mind.routes, location, pending).map(|replacement| {
            PreparedRecallLoss {
                actor,
                routes: mind.routes.clone(),
                location: location.clone(),
                pending: pending.to_vec(),
                replacement,
            }
        }),
    )
}
impl PreparedRecallLoss {
    pub fn replacement(&self) -> &[AreaId] {
        &self.replacement
    }
    pub fn validate_batch<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        pending: &[AreaId],
        batch: &game_ecs::Batch,
    ) -> Result<(), game_core::Error> {
        let key = game_ecs::StableId(self.actor.0);
        if pending != self.pending
            || store.get::<Location>(key) != Some(&self.location)
            || game_characters::decisions::mind::<A>(store, self.actor)
                .is_none_or(|mind| mind.routes != self.routes)
            || batch.touches::<Location>(key)
            || batch.touches::<game_characters::decisions::Mind<A>>(key)
        {
            return Err(game_core::Error::Unavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use crate::memory_start::tests::{ACTOR, LIFE, fixture};
    use crate::*;
    use game_characters::decisions::mind;
    use game_core::*;
    use game_ecs::{Batch, Schema, StableId, Store};
    use std::collections::BTreeSet;

    #[test]
    fn recall_loss_and_native_route_receiver_reject_foreign_reads_before_session_publication() {
        let (runtime, source, _) = fixture();
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        game_transport::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        game_characters::decisions::insert_mind(
            &mut store,
            ACTOR,
            mind::<u8>(&source, ACTOR).unwrap().clone(),
        )
        .unwrap();
        store
            .insert_or_spawn(StableId(ACTOR.0), Location::AtArea(AreaId(3)))
            .unwrap();
        store
            .insert_or_spawn(StableId(ACTOR.0), session(&source, ACTOR).unwrap().clone())
            .unwrap();
        insert_burden(
            &mut store,
            ACTOR,
            Burden {
                permanent_identity: 10,
                ..Burden::default()
            },
        )
        .unwrap();
        game_transport::insert_route(
            &mut store,
            ACTOR,
            game_transport::Route {
                steps: vec![AreaId(4)],
            },
        )
        .unwrap();
        let facts = AdmissionFacts {
            actor: ACTOR,
            now: SimTime(0),
            current: Some(ACTOR),
            alive: true,
            health: 100_000,
            independent: true,
            learning: 100,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100_000,
            archived: BTreeSet::from([LIFE]),
        };
        // Current routes are empty: dismissal must remove the historical next leg.
        for attack in 0..3 {
            let recall = prepare_recall_loss::<u8>(
                &store,
                ACTOR,
                game_transport::route_steps(&store, ACTOR),
            )
            .unwrap()
            .unwrap();
            assert!(recall.replacement().is_empty());
            let route = game_transport::prepare_route_replacement(
                &store,
                ACTOR,
                recall.replacement().to_vec(),
            )
            .unwrap();
            let control = runtime
                .control
                .prepare_action(&store, ACTOR, &ControlAction::Dismiss, &facts, None)
                .unwrap();
            let effects = control.effects.clone();
            let before = store.clone();
            let mut next_id = 1_u64;
            let mut published = Vec::new();
            let mut batch = Batch::new();
            let control_receipt = control
                .stage_into(&runtime.control, &store, &mut batch)
                .unwrap();
            let route_receipt = route.stage_into(&store, &mut batch).unwrap();
            match attack {
                0 => {
                    batch.insert(StableId(ACTOR.0), Location::AtArea(AreaId(4)));
                }
                1 => {
                    batch.insert(
                        StableId(ACTOR.0),
                        mind::<u8>(&store, ACTOR).unwrap().clone(),
                    );
                }
                _ => {
                    batch.remove::<game_transport::Route>(StableId(ACTOR.0));
                }
            }
            let publication = (|| -> Result<(), Error> {
                control_receipt.validate_batch(&runtime.control, &store, &batch)?;
                recall.validate_batch::<u8>(
                    &store,
                    game_transport::route_steps(&store, ACTOR),
                    &batch,
                )?;
                route_receipt.validate_footprint(&batch)?;
                store.apply_batch(batch)?;
                next_id += 1;
                published.extend(effects);
                Ok(())
            })();
            assert!(publication.is_err());
            assert_eq!(store, before);
            assert_eq!(next_id, 1);
            assert!(published.is_empty());
        }
        let recall =
            prepare_recall_loss::<u8>(&store, ACTOR, game_transport::route_steps(&store, ACTOR))
                .unwrap()
                .unwrap();
        let route =
            game_transport::prepare_route_replacement(&store, ACTOR, recall.replacement().to_vec())
                .unwrap();
        let control = runtime
            .control
            .prepare_action(&store, ACTOR, &ControlAction::Dismiss, &facts, None)
            .unwrap();
        let mut batch = Batch::new();
        let control_receipt = control
            .stage_into(&runtime.control, &store, &mut batch)
            .unwrap();
        let route_receipt = route.stage_into(&store, &mut batch).unwrap();
        assert!(route_receipt.validate_footprint(&Batch::new()).is_err());
        control_receipt
            .validate_batch(&runtime.control, &store, &batch)
            .unwrap();
        recall
            .validate_batch::<u8>(&store, game_transport::route_steps(&store, ACTOR), &batch)
            .unwrap();
        route_receipt.validate_footprint(&batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert!(session(&store, ACTOR).is_none());
        assert!(game_transport::route_steps(&store, ACTOR).is_empty());
        assert_eq!(burden(&store, ACTOR).unwrap().permanent_identity, 10);
        assert!(mind::<u8>(&store, ACTOR).is_some());
        game_transport::insert_route(
            &mut store,
            ACTOR,
            game_transport::Route {
                steps: vec![AreaId(4)],
            },
        )
        .unwrap();
        let recall =
            prepare_recall_loss::<u8>(&store, ACTOR, game_transport::route_steps(&store, ACTOR))
                .unwrap()
                .unwrap();
        let batch = Batch::new();
        assert!(recall.validate_batch::<u8>(&store, &[], &batch).is_err());
        game_characters::decisions::mind_mut::<u8>(&mut store, ACTOR)
            .unwrap()
            .routes
            .insert(AreaId(3), BTreeMap::from([(AreaId(4), SimDuration(100))]));
        assert!(
            recall
                .validate_batch::<u8>(&store, game_transport::route_steps(&store, ACTOR), &batch)
                .is_err()
        );
    }
}
