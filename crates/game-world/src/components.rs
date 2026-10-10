//! Composition registers feature-owned schemas; the host owns one component store.
use game_api::Error;
use game_ecs::{Schema, Snapshot, Store};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::{Deref, DerefMut};

fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    game_agriculture::register_components(&mut schema)?;
    game_physiology::register_components(&mut schema)?;
    game_workshop::register_components(&mut schema)?;
    schema.register::<game_api::Activity>("coordination/activity", |_| Ok(()))?;
    Ok(schema)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Components(Store);
impl Components {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(Store::with_schema(schema()?)))
    }
    pub fn body(&self, actor: game_api::PersonId) -> Option<&game_physiology::Body> {
        game_physiology::body(&self.0, actor)
    }
    pub fn body_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_physiology::Body> {
        game_physiology::body_mut(&mut self.0, actor)
    }
    pub fn bodies(&self) -> impl Iterator<Item = (game_api::PersonId, &game_physiology::Body)> {
        game_physiology::bodies(&self.0)
    }
    pub fn activity(&self, actor: game_api::PersonId) -> Option<&game_api::Activity> {
        self.0.get(game_ecs::StableId(actor.0))
    }
    pub fn activity_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_api::Activity> {
        self.0.get_mut(game_ecs::StableId(actor.0))
    }
    pub fn activities(&self) -> impl Iterator<Item = (game_api::PersonId, &game_api::Activity)> {
        self.0
            .iter::<game_api::Activity>()
            .map(|(id, activity)| (game_api::PersonId(id.0), activity))
    }
    pub fn set_activity(
        &mut self,
        actor: game_api::PersonId,
        activity: game_api::Activity,
    ) -> Result<(), Error> {
        self.0.insert(game_ecs::StableId(actor.0), activity)
    }
    pub fn job(&self, actor: game_api::PersonId) -> Option<&game_workshop::Job> {
        game_workshop::job(&self.0, actor)
    }
    pub fn job_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_workshop::Job> {
        game_workshop::job_mut(&mut self.0, actor)
    }
    pub fn jobs(&self) -> impl Iterator<Item = (game_api::PersonId, &game_workshop::Job)> {
        game_workshop::jobs(&self.0)
    }
    pub fn set_job(
        &mut self,
        actor: game_api::PersonId,
        job: Option<game_workshop::Job>,
    ) -> Result<(), Error> {
        if let Some(job) = job {
            game_workshop::insert_job(&mut self.0, actor, job)
        } else {
            game_workshop::remove_job(&mut self.0, actor).map(|_| ())
        }
    }
}
impl Deref for Components {
    type Target = Store;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Components {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Serialize for Components {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0
            .snapshot()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Components {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = Snapshot::deserialize(deserializer)?;
        Store::restore(schema().map_err(serde::de::Error::custom)?, snapshot)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Execution, Simulation};
    use game_core::{SimDuration, SimTime};

    #[test]
    fn one_host_snapshot_and_metadata_only_runtime_survive_clone_and_restore() {
        let mut sim = Simulation::farming_fixture(77, Execution::default()).unwrap();
        let state = sim.canonical_state();
        let json = serde_json::to_value(state).unwrap();
        assert!(
            json["components"]["columns"]["agriculture/field"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        let agriculture = json["agriculture"].as_object().unwrap();
        assert!(!agriculture.contains_key("components"));
        assert!(!agriculture.contains_key("fields"));
        assert!(!agriculture.contains_key("crops"));
        assert!(!agriculture.contains_key("soils"));
        let cloned = state.clone();
        assert_eq!(state, &cloned);
        assert!(sim.agriculture_queries.refresh(&cloned.components).is_err());
        let mut rebound = Simulation::from_state(
            cloned,
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
            0,
        )
        .unwrap();
        let mut restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(rebound.canonical_state(), restored.canonical_state());
        rebound.advance(SimDuration(120_000)).unwrap();
        restored.advance(SimDuration(120_000)).unwrap();
        assert_eq!(rebound.canonical_state(), restored.canonical_state());
    }

    #[test]
    fn body_activity_and_workshop_jobs_are_independent_canonical_columns() {
        let mut sim = Simulation::survival_fixture(13, Execution::default()).unwrap();
        let ruler = sim.state.ruler;
        let site = *sim
            .state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .keys()
            .next()
            .unwrap();
        sim.debug_actor_command(ruler, game_api::Command::Work { site })
            .unwrap();
        let json = serde_json::to_value(sim.canonical_state()).unwrap();
        assert!(!json["physical"].as_object().unwrap().contains_key("actors"));
        assert_eq!(
            json["components"]["columns"]["physiology/body"]
                .as_array()
                .unwrap()
                .len(),
            sim.state.people.len()
        );
        assert_eq!(
            json["components"]["columns"]["coordination/activity"]
                .as_array()
                .unwrap()
                .len(),
            sim.state.people.len()
        );
        assert!(
            !json["components"]["columns"]["workshop/job"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert!(restored.state.components.job(ruler).is_some());
    }

    #[test]
    fn obsolete_actor_payload_is_rejected_in_the_physical_metadata_header() {
        let sim = Simulation::survival_fixture(16, Execution::default()).unwrap();
        let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
        saved["state"]["physical"]["actors"] = serde_json::json!({});
        assert!(
            Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).is_err()
        );
    }

    #[test]
    fn a_stale_body_batch_fails_before_clock_or_events_are_published() {
        let mut sim = Simulation::survival_fixture(14, Execution::default()).unwrap();
        sim.physical_query.refresh(&sim.state.components).unwrap();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        sim.state
            .components
            .body_mut(sim.state.ruler)
            .unwrap()
            .vitals
            .hunger += 1;
        let before = sim.canonical_state().clone();
        assert_eq!(
            sim.commit_prepared_step(prepared),
            Err(game_core::Error::InvalidSave(
                "stale physical component batch".into()
            ))
        );
        assert_eq!(&before, sim.canonical_state());
    }

    #[test]
    fn physical_batch_guards_opening_population_membership() {
        let mut sim = Simulation::survival_fixture(15, Execution::default()).unwrap();
        sim.physical_query.refresh(&sim.state.components).unwrap();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        let mut newcomer = sim.state.people[0].clone();
        newcomer.id = game_api::PersonId(crate::allocate(&mut sim.state).unwrap());
        newcomer.role = game_api::Role::Citizen;
        let body = sim.state.components.body(sim.state.ruler).unwrap().clone();
        game_physiology::insert_body(&mut sim.state.components, newcomer.id, body).unwrap();
        sim.state
            .components
            .set_activity(newcomer.id, game_api::Activity::Idle)
            .unwrap();
        let id = newcomer.id;
        let index = sim.state.people.len();
        sim.state.people.push(newcomer);
        sim.person_index.insert(id, index);
        let inventory = sim
            .debug_add_inventory(
                game_api::Good::Food,
                crate::InventoryLocation::Carried(id),
                game_api::Quantity(0),
                game_api::Quantity(10_000),
                std::collections::BTreeSet::from([id]),
            )
            .unwrap();
        sim.state.people[index].inventory = inventory;
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.canonical_state().clone();
        assert_eq!(
            sim.commit_prepared_step(prepared),
            Err(game_core::Error::InvalidSave(
                "stale physical component batch".into()
            ))
        );
        assert_eq!(&before, sim.canonical_state());
    }
}
