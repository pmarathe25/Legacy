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
    game_skills::register_components(&mut schema)?;
    game_health::register_components(&mut schema)?;
    game_genetics::register_components(&mut schema)?;
    game_physiology::traits::register_components(&mut schema)?;
    game_characters::temperament::register_components(&mut schema)?;
    game_spatial::register_components(&mut schema)?;
    game_environment::register_components(&mut schema)?;
    game_ecology::register_components(&mut schema)?;
    game_land::register_components(&mut schema)?;
    schema.register::<game_api::Activity>("coordination/activity", |_| Ok(()))?;
    Ok(schema)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Components(Store);
impl Components {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(Store::with_schema(schema()?)))
    }
    pub(crate) fn validate_land(&self) -> Result<(), Error> {
        let fields = self
            .0
            .iter::<game_agriculture::Field>()
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>();
        let plots = self
            .0
            .iter::<game_ecology::Plot>()
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>();
        let soils = self
            .0
            .iter::<game_land::Soil>()
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>();
        if !fields.is_disjoint(&plots) || soils != fields.union(&plots).copied().collect() {
            return Err(Error::InvalidSave(
                "inconsistent shared soil ownership".into(),
            ));
        }
        Ok(())
    }
    pub fn person_genome(&self, id: game_api::PersonId) -> Option<&game_genetics::PersonGenome> {
        game_genetics::person_genome(&self.0, id)
    }
    pub fn person_genome_mut(
        &mut self,
        id: game_api::PersonId,
    ) -> Option<&mut game_genetics::PersonGenome> {
        game_genetics::person_genome_mut(&mut self.0, id)
    }
    pub fn person_genomes(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, &game_genetics::PersonGenome)> {
        game_genetics::all_person_genomes(&self.0)
    }
    pub fn inheritance(&self, id: game_api::PregnancyId) -> Option<&game_genetics::Inheritance> {
        game_genetics::inheritance(&self.0, id)
    }
    pub fn inheritance_mut(
        &mut self,
        id: game_api::PregnancyId,
    ) -> Option<&mut game_genetics::Inheritance> {
        game_genetics::inheritance_mut(&mut self.0, id)
    }
    pub fn inheritances(
        &self,
    ) -> impl Iterator<Item = (game_api::PregnancyId, &game_genetics::Inheritance)> {
        game_genetics::all_inheritances(&self.0)
    }
    pub fn trait_development_row(
        &self,
        id: game_api::PersonId,
    ) -> Option<&game_physiology::traits::Development> {
        game_physiology::traits::development(&self.0, id)
    }
    pub fn trait_development_row_mut(
        &mut self,
        id: game_api::PersonId,
    ) -> Option<&mut game_physiology::traits::Development> {
        game_physiology::traits::development_mut(&mut self.0, id)
    }
    pub fn trait_developments(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, &game_physiology::traits::Development)> {
        game_physiology::traits::all_developments(&self.0)
    }
    pub fn person_temperament(
        &self,
        id: game_api::PersonId,
    ) -> Option<&game_characters::temperament::PersonTemperament> {
        game_characters::temperament::person_temperament(&self.0, id)
    }
    pub fn person_temperament_mut(
        &mut self,
        id: game_api::PersonId,
    ) -> Option<&mut game_characters::temperament::PersonTemperament> {
        game_characters::temperament::person_temperament_mut(&mut self.0, id)
    }
    pub fn person_temperaments(
        &self,
    ) -> impl Iterator<
        Item = (
            game_api::PersonId,
            &game_characters::temperament::PersonTemperament,
        ),
    > {
        game_characters::temperament::all_person_temperaments(&self.0)
    }
    pub fn water_lot(&self, id: game_api::InventoryId) -> Option<&game_health::WaterLot> {
        game_health::water_lot(&self.0, id)
    }
    pub fn water_lot_mut(
        &mut self,
        id: game_api::InventoryId,
    ) -> Option<&mut game_health::WaterLot> {
        game_health::water_lot_mut(&mut self.0, id)
    }
    pub fn water_lots(
        &self,
    ) -> impl Iterator<Item = (game_api::InventoryId, &game_health::WaterLot)> {
        game_health::water_lots(&self.0)
    }
    pub fn insert_water_lot(
        &mut self,
        id: game_api::InventoryId,
        row: game_health::WaterLot,
    ) -> Result<(), Error> {
        game_health::insert_water_lot(&mut self.0, id, row)
    }
    pub fn remove_water_lot(
        &mut self,
        id: game_api::InventoryId,
    ) -> Result<Option<game_health::WaterLot>, Error> {
        game_health::remove_water_lot(&mut self.0, id)
    }
    pub fn shelter_occupancy(&self, id: game_api::PersonId) -> Option<&game_health::Occupancy> {
        game_health::occupancy(&self.0, id)
    }
    pub fn shelter_occupancy_mut(
        &mut self,
        id: game_api::PersonId,
    ) -> Option<&mut game_health::Occupancy> {
        game_health::occupancy_mut(&mut self.0, id)
    }
    pub fn occupancies(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, &game_health::Occupancy)> {
        game_health::occupancies(&self.0)
    }
    pub fn insert_shelter_occupancy(
        &mut self,
        id: game_api::PersonId,
        row: game_health::Occupancy,
    ) -> Result<(), Error> {
        game_health::insert_occupancy(&mut self.0, id, row)
    }
    pub fn remove_shelter_occupancy(
        &mut self,
        id: game_api::PersonId,
    ) -> Result<Option<game_health::Occupancy>, Error> {
        game_health::remove_occupancy(&mut self.0, id)
    }
    pub fn health_knowledge(&self, id: game_api::PersonId) -> Option<&game_health::Knowledge> {
        game_health::knowledge(&self.0, id)
    }
    pub fn health_knowledge_mut(
        &mut self,
        id: game_api::PersonId,
    ) -> Option<&mut game_health::Knowledge> {
        game_health::knowledge_mut(&mut self.0, id)
    }
    pub fn knowledges(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, &game_health::Knowledge)> {
        game_health::knowledges(&self.0)
    }
    pub fn insert_health_knowledge(
        &mut self,
        id: game_api::PersonId,
        row: game_health::Knowledge,
    ) -> Result<(), Error> {
        game_health::insert_knowledge(&mut self.0, id, row)
    }
    pub fn remove_health_knowledge(
        &mut self,
        id: game_api::PersonId,
    ) -> Result<Option<game_health::Knowledge>, Error> {
        game_health::remove_knowledge(&mut self.0, id)
    }
    pub fn ensure_health_knowledge(
        &mut self,
        id: game_api::PersonId,
    ) -> Result<&mut game_health::Knowledge, Error> {
        if self.health_knowledge(id).is_none() {
            self.insert_health_knowledge(id, game_health::Knowledge::default())?;
        }
        self.health_knowledge_mut(id).ok_or(Error::Unavailable)
    }
    pub fn shelter(&self, id: game_api::ShelterId) -> Option<&game_health::Shelter> {
        game_health::shelter(&self.0, id)
    }
    pub fn shelter_mut(&mut self, id: game_api::ShelterId) -> Option<&mut game_health::Shelter> {
        game_health::shelter_mut(&mut self.0, id)
    }
    pub fn shelters(&self) -> impl Iterator<Item = (game_api::ShelterId, &game_health::Shelter)> {
        game_health::shelters(&self.0)
    }
    pub fn insert_shelter(&mut self, row: game_health::Shelter) -> Result<(), Error> {
        game_health::insert_shelter(&mut self.0, row)
    }
    pub fn remove_shelter(
        &mut self,
        id: game_api::ShelterId,
    ) -> Result<Option<game_health::Shelter>, Error> {
        game_health::remove_shelter(&mut self.0, id)
    }
    pub fn health_offer(
        &self,
        giver: game_api::PersonId,
        patient: game_api::PersonId,
    ) -> Option<&game_health::Offer> {
        game_health::offer(&self.0, giver, patient)
    }
    pub fn health_offer_mut(
        &mut self,
        giver: game_api::PersonId,
        patient: game_api::PersonId,
    ) -> Option<&mut game_health::Offer> {
        game_health::offer_mut(&mut self.0, giver, patient)
    }
    pub fn health_offers(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, game_api::PersonId, &game_health::Offer)> {
        game_health::all_offers(&self.0)
    }
    pub fn insert_health_offer(&mut self, offer: game_health::Offer) -> Result<(), Error> {
        game_health::insert_offer(&mut self.0, offer)
    }
    pub fn clinical(&self, actor: game_api::PersonId) -> Option<&game_health::Clinical> {
        game_health::clinical(&self.0, actor)
    }
    pub fn clinical_mut(
        &mut self,
        actor: game_api::PersonId,
    ) -> Option<&mut game_health::Clinical> {
        game_health::clinical_mut(&mut self.0, actor)
    }
    pub fn clinicals(&self) -> impl Iterator<Item = (game_api::PersonId, &game_health::Clinical)> {
        game_health::clinicals(&self.0)
    }
    pub fn insert_clinical(
        &mut self,
        actor: game_api::PersonId,
        clinical: game_health::Clinical,
    ) -> Result<(), Error> {
        game_health::insert_clinical(&mut self.0, actor, clinical)
    }
    pub fn health_job(&self, actor: game_api::PersonId) -> Option<&game_health::Job> {
        game_health::job(&self.0, actor)
    }
    pub fn health_job_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_health::Job> {
        game_health::job_mut(&mut self.0, actor)
    }
    pub fn health_jobs(&self) -> impl Iterator<Item = (game_api::PersonId, &game_health::Job)> {
        game_health::jobs(&self.0)
    }
    pub fn insert_health_job(&mut self, job: game_health::Job) -> Result<(), Error> {
        game_health::insert_job(&mut self.0, job)
    }
    pub fn remove_health_job(
        &mut self,
        actor: game_api::PersonId,
    ) -> Result<Option<game_health::Job>, Error> {
        game_health::remove_job(&mut self.0, actor)
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
    pub fn personal_skills(&self, actor: game_api::PersonId) -> Option<&game_skills::Skills> {
        game_skills::skills(&self.0, actor)
    }
    pub fn personal_skills_mut(
        &mut self,
        actor: game_api::PersonId,
    ) -> Option<&mut game_skills::Skills> {
        game_skills::skills_mut(&mut self.0, actor)
    }
    pub fn all_personal_skills(
        &self,
    ) -> impl Iterator<Item = (game_api::PersonId, &game_skills::Skills)> {
        game_skills::all_skills(&self.0)
    }
    pub fn site(&self, id: game_api::SiteId) -> Option<&game_workshop::Site> {
        game_workshop::site(&self.0, id)
    }
    pub fn site_mut(&mut self, id: game_api::SiteId) -> Option<&mut game_workshop::Site> {
        game_workshop::site_mut(&mut self.0, id)
    }
    pub fn sites(&self) -> impl Iterator<Item = (game_api::SiteId, &game_workshop::Site)> {
        game_workshop::sites(&self.0)
    }
    pub fn insert_site(&mut self, site: game_workshop::Site) -> Result<(), Error> {
        game_workshop::insert_site(&mut self.0, site)
    }
    pub fn remove_site(
        &mut self,
        id: game_api::SiteId,
    ) -> Result<Option<game_workshop::Site>, Error> {
        game_workshop::remove_site(&mut self.0, id)
    }
    pub fn location(&self, actor: game_api::PersonId) -> Option<&game_api::Location> {
        game_spatial::location(&self.0, game_ecs::StableId(actor.0))
    }
    pub fn location_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_api::Location> {
        game_spatial::location_mut(&mut self.0, game_ecs::StableId(actor.0))
    }
    pub fn set_location(
        &mut self,
        actor: game_api::PersonId,
        location: game_api::Location,
    ) -> Result<(), Error> {
        game_spatial::set_location(&mut self.0, game_ecs::StableId(actor.0), location)
    }
    pub fn locations(&self) -> impl Iterator<Item = (game_api::PersonId, &game_api::Location)> {
        game_spatial::locations(&self.0).map(|(id, location)| (game_api::PersonId(id.0), location))
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
        let site = sim
            .state
            .components
            .sites()
            .map(|(id, _)| id)
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
    fn stale_physical_trait_development_fails_before_publication() {
        let mut sim = Simulation::genetics_fixture(85, Execution::default()).unwrap();
        sim.physical_query.refresh(&sim.state.components).unwrap();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        let actor = sim.state.ruler;
        sim.state
            .components
            .trait_development_row_mut(actor)
            .unwrap()
            .physical_training_milli += 1;
        let before = sim.canonical_state().clone();
        assert!(sim.commit_prepared_step(prepared).is_err());
        assert_eq!(&before, sim.canonical_state());
    }

    #[test]
    fn stale_expressed_alleles_fail_before_publication() {
        let mut sim = Simulation::genetics_fixture(86, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let locus = game_genetics::LocusId(
            sim.state
                .physical_traits
                .as_ref()
                .unwrap()
                .definitions
                .traits
                .iter()
                .find(|t| t.kind == game_physiology::traits::PhysicalTrait::Metabolism)
                .unwrap()
                .inheritance
                .locus
                .clone(),
        );
        let opening = sim
            .state
            .genetics
            .as_ref()
            .unwrap()
            .alleles(&sim.state.components, actor, &locus)
            .unwrap();
        sim.physical_query.refresh(&sim.state.components).unwrap();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        let new_value = if opening == [0; 2] { 1_000 } else { 0 };
        sim.state
            .genetics
            .as_mut()
            .unwrap()
            .set_alleles(&mut sim.state.components, actor, &locus, [new_value; 2])
            .unwrap();
        let before = sim.canonical_state().clone();
        assert!(sim.commit_prepared_step(prepared).is_err());
        assert_eq!(&before, sim.canonical_state());
    }

    #[test]
    fn invalid_final_founder_traits_leave_admission_inert() {
        let mut sim = Simulation::skills_fixture(87, Execution::default()).unwrap();
        let last = sim.state.people.last().unwrap().id;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&last)
            .unwrap()
            .personality
            .empathy = 1_001;
        let before = sim.canonical_state().clone();
        let content =
            crate::genetics::Content::read(crate::genetics::Config::default().content_dir).unwrap();
        assert!(sim.initialize_trait_content(content).is_err());
        assert_eq!(&before, sim.canonical_state());
    }

    #[test]
    fn genetic_and_trait_rows_are_authoritative_and_orphans_fail_restore() {
        let sim = Simulation::genetics_fixture(84, Execution::default()).unwrap();
        let saved = sim.save().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        for header in ["genetics", "physical_traits", "temperament"] {
            assert!(json["state"][header].get("people").is_none());
        }
        assert!(json["state"]["genetics"].get("pregnancies").is_none());
        for column in [
            "genetics/genome",
            "physiology/development",
            "characters/temperament",
        ] {
            assert_eq!(
                json["state"]["components"]["columns"][column]
                    .as_array()
                    .unwrap()
                    .len(),
                sim.state.people.len()
            );
            let mut malformed = json.clone();
            malformed["state"]["components"]["columns"][column]
                .as_array_mut()
                .unwrap()
                .remove(0);
            assert!(
                Simulation::load(
                    &serde_json::to_vec(&malformed).unwrap(),
                    Execution::default()
                )
                .is_err()
            );
        }
        let mut orphaned = json;
        for header in ["genetics", "physical_traits", "temperament", "heredity"] {
            orphaned["state"][header] = serde_json::Value::Null;
        }
        assert!(
            Simulation::load(
                &serde_json::to_vec(&orphaned).unwrap(),
                Execution::default()
            )
            .is_err()
        );
        assert_eq!(
            sim.canonical_state(),
            Simulation::load(&saved, Execution::default())
                .unwrap()
                .canonical_state()
        );
    }

    #[test]
    fn pending_npc_death_refreshes_queries_after_occupancy_removal() {
        let mut sim = Simulation::combat_fixture(89, Execution::default()).unwrap();
        sim.initialize_health(game_health::Rules::fixture())
            .unwrap();
        let ruler = sim.state.ruler;
        let enemy = sim.state.people[3].id;
        let area = sim
            .state
            .components
            .location(ruler)
            .unwrap()
            .cell()
            .unwrap();
        let timber = sim
            .debug_add_inventory(
                game_api::Good::Timber,
                crate::InventoryLocation::Area(area),
                game_api::Quantity(10_000),
                game_api::Quantity(10_000),
                std::collections::BTreeSet::from([ruler]),
            )
            .unwrap();
        let shelter = sim
            .seed_shelter(
                area,
                ruler,
                std::collections::BTreeSet::from([ruler]),
                1,
                650,
                timber,
                game_api::Quantity(10_000),
                SimTime(0),
            )
            .unwrap();
        sim.set_initial_shelter_occupancy(ruler, shelter).unwrap();
        let encounter = sim
            .debug_start_encounter(vec![(ruler, 0), (enemy, 1)])
            .unwrap();
        for stock in sim.state.inventories.values_mut() {
            if stock.good == game_api::Good::Binding
                && stock.location == crate::InventoryLocation::Carried(enemy)
            {
                stock.quantity = game_api::Quantity(0);
            }
        }
        let damage = sim.body_rules(ruler).max_health - 1;
        sim.debug_injure(ruler, damage).unwrap();
        let combat = sim.state.combat.as_mut().unwrap();
        let map = combat.encounters[&encounter].map;
        combat.map_mut(map).unwrap().obstacles.clear();
        let battle = combat.encounters.get_mut(&encounter).unwrap();
        battle.fighters.get_mut(&ruler).unwrap().position = game_api::Point { x: 5_000, y: 5_000 };
        battle.fighters.get_mut(&enemy).unwrap().position = game_api::Point { x: 6_000, y: 5_000 };
        // An authored pending NPC turn runs through the normal advance path.
        battle.order = vec![enemy, ruler];
        battle.cursor = 0;
        assert!(sim.state.components.shelter_occupancy(ruler).is_some());
        let advanced = sim.advance(game_api::SimDuration(1)).unwrap();
        assert_eq!(advanced.reached, SimTime(1));
        assert!(sim.state.components.shelter_occupancy(ruler).is_none());
        assert!(sim.state.events.iter().any(|event| matches!(event.kind,game_api::EventKind::Struck {actor,target,..} if actor == enemy && target == ruler)));
        assert!(
            sim.state.events.iter().any(
                |event| matches!(event.kind,game_api::EventKind::Died {actor} if actor == ruler)
            )
        );
    }

    #[test]
    fn health_knowledge_without_the_health_runtime_is_rejected() {
        let mut sim = Simulation::survival_fixture(88, Execution::default()).unwrap();
        assert!(sim.state.health.is_none());
        sim.state
            .components
            .insert_health_knowledge(sim.state.ruler, game_health::Knowledge::default())
            .unwrap();
        assert!(
            matches!(Simulation::load(&sim.save().unwrap(), Execution::default()),
            Err(game_api::Error::InvalidSave(message)) if message == "health components require health runtime")
        );
    }

    #[test]
    fn health_columns_are_authoritative_and_orphans_fail_restore() {
        let sim = Simulation::health_fixture(82, Execution::default()).unwrap();
        let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
        let header = saved["state"]["health"].as_object().unwrap();
        for obsolete in [
            "clinical",
            "jobs",
            "water",
            "shelters",
            "occupancy",
            "known",
            "offers",
        ] {
            assert!(!header.contains_key(obsolete));
        }
        for column in [
            "health/clinical",
            "health/job",
            "health/water",
            "health/shelter",
            "health/occupancy",
            "health/knowledge",
            "health/offers",
        ] {
            assert!(saved["state"]["components"]["columns"][column].is_array());
        }
        assert_eq!(
            saved["state"]["components"]["columns"]["health/clinical"]
                .as_array()
                .unwrap()
                .len(),
            sim.state.people.len()
        );
        let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        saved["state"]["health"] = serde_json::Value::Null;
        assert!(
            Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).is_err()
        );
    }

    #[test]
    fn stale_clinical_and_health_job_inputs_fail_before_publication() {
        for change_job in [false, true] {
            let mut sim = Simulation::health_fixture(83, Execution::default()).unwrap();
            let actor = sim.state.ruler;
            let area = sim
                .state
                .components
                .location(actor)
                .unwrap()
                .cell()
                .unwrap();
            sim.state
                .components
                .insert_health_job(game_health::Job {
                    actor,
                    area,
                    completes: SimTime(60_000),
                    work: game_health::Work::Treat {
                        patient: actor,
                        skill_ms: 0,
                        elapsed: 0,
                    },
                })
                .unwrap();
            sim.state
                .components
                .set_activity(
                    actor,
                    game_api::Activity::HealthWork {
                        kind: game_api::HealthWorkKind::Treat,
                        completes: SimTime(60_000),
                    },
                )
                .unwrap();
            sim.physical_query.refresh(&sim.state.components).unwrap();
            let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
            if change_job {
                sim.state
                    .components
                    .health_job_mut(actor)
                    .unwrap()
                    .completes = SimTime(60_001);
            } else {
                sim.state.components.clinical_mut(actor).unwrap().thirst += 1;
            }
            let before = sim.canonical_state().clone();
            assert!(sim.commit_prepared_step(prepared).is_err());
            assert_eq!(&before, sim.canonical_state());
        }
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
        let location = sim
            .state
            .components
            .location(sim.state.ruler)
            .unwrap()
            .clone();
        sim.state
            .components
            .set_location(newcomer.id, location)
            .unwrap();
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

    #[test]
    fn spatial_column_is_the_only_person_position_and_bad_graphs_fail_restore() {
        let sim = Simulation::kernel_fixture(17, 5, Execution::default()).unwrap();
        let saved = sim.save().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert!(
            json["state"]["people"]
                .as_array()
                .unwrap()
                .iter()
                .all(|person| person.get("location").is_none())
        );
        assert_eq!(
            json["state"]["components"]["columns"]["spatial/location"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        let first = sim.state.people[0].id;
        let second = sim.state.people[1].id;
        let mut state = sim.canonical_state().clone();
        state
            .components
            .set_location(first, game_api::Location::Carried { carrier: second })
            .unwrap();
        state
            .components
            .set_location(second, game_api::Location::Carried { carrier: first })
            .unwrap();
        let mut corrupt = json.clone();
        corrupt["state"] = serde_json::to_value(&state).unwrap();
        assert!(
            Simulation::load(&serde_json::to_vec(&corrupt).unwrap(), Execution::default()).is_err()
        );
        let mut missing = sim.canonical_state().clone();
        game_spatial::remove_location(&mut missing.components, game_ecs::StableId(first.0))
            .unwrap();
        corrupt["state"] = serde_json::to_value(missing).unwrap();
        assert!(
            Simulation::load(&serde_json::to_vec(&corrupt).unwrap(), Execution::default()).is_err()
        );
        assert_eq!(sim.save().unwrap(), saved);
    }

    #[test]
    fn workshop_sites_roundtrip_only_as_shared_rows_and_require_physical_metadata() {
        let sim = Simulation::survival_fixture(18, Execution::default()).unwrap();
        let saved = sim.save().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert!(
            !json["state"]["physical"]
                .as_object()
                .unwrap()
                .contains_key("sites")
        );
        assert_eq!(
            json["state"]["components"]["columns"]["workshop/site"]
                .as_array()
                .unwrap()
                .len(),
            sim.state.components.sites().count()
        );
        let restored = Simulation::load(
            &saved,
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());

        let mut kernel = Simulation::kernel_fixture(19, 5, Execution::default()).unwrap();
        let person = &kernel.state.people[0];
        let id = game_api::SiteId(person.id.0);
        let stock = person.inventory;
        kernel
            .state
            .components
            .insert_site(game_workshop::Site {
                id,
                area: kernel.state.areas[0].id,
                input: stock,
                input_quantity: game_api::Quantity(1),
                tool: stock,
                output: stock,
                output_quantity: game_api::Quantity(1),
                duration: SimDuration(1),
                access: std::collections::BTreeSet::new(),
                last_granted: None,
            })
            .unwrap();
        assert!(Simulation::load(&kernel.save().unwrap(), Execution::default()).is_err());
    }

    #[test]
    fn personal_skills_are_saved_as_components_and_require_their_runtime() {
        let sim = Simulation::skills_fixture(20, Execution::default()).unwrap();
        let saved = sim.save().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let metadata = json["state"]["skills"].as_object().unwrap();
        assert!(!metadata.contains_key("records"));
        assert!(!metadata.contains_key("semantic_ticks"));
        assert_eq!(
            json["state"]["components"]["columns"]["skills/personal"]
                .as_array()
                .unwrap()
                .len(),
            sim.state.people.len()
        );
        let restored = Simulation::load(
            &saved,
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let mut missing_runtime = sim.canonical_state().clone();
        missing_runtime.skills = None;
        assert!(crate::skills::validate(&missing_runtime).is_err());
    }

    #[test]
    fn land_soil_keys_match_one_land_use_and_reject_missing_or_orphan_rows() {
        let mut sim = Simulation::ecology_fixture(21, Execution::default()).unwrap();
        let plot = sim
            .debug_add_plot(
                sim.state.areas[1].id,
                None,
                sim.state.people.iter().map(|person| person.id).collect(),
                900,
            )
            .unwrap();
        sim.state.components.validate_land().unwrap();
        let json = serde_json::to_value(sim.canonical_state()).unwrap();
        assert!(
            json["components"]["columns"]
                .get("agriculture/soil")
                .is_none()
        );
        assert!(json["components"]["columns"]["land/soil"].is_array());

        let mut missing = sim.canonical_state().clone();
        game_land::remove_soil(&mut missing.components, plot).unwrap();
        assert!(missing.components.validate_land().is_err());

        let mut orphan = sim.canonical_state().clone();
        let person = game_core::SiteId(orphan.people[0].id.0);
        game_land::insert_soil(
            &mut orphan.components,
            person,
            game_land::Soil {
                fertility: 500,
                water: 1,
                capacity: 10,
            },
        )
        .unwrap();
        assert!(orphan.components.validate_land().is_err());

        let mut ambiguous = sim.canonical_state().clone();
        let field = ambiguous
            .agriculture
            .as_ref()
            .unwrap()
            .fields(&ambiguous.components)
            .next()
            .unwrap()
            .1
            .clone();
        let mut duplicate = field;
        duplicate.id = plot;
        ambiguous
            .agriculture
            .as_ref()
            .unwrap()
            .insert_field(&mut ambiguous.components, duplicate)
            .unwrap();
        assert!(ambiguous.components.validate_land().is_err());
        let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
        for invalid in [missing, orphan, ambiguous] {
            envelope["state"] = serde_json::to_value(invalid).unwrap();
            assert!(
                Simulation::load(
                    &serde_json::to_vec(&envelope).unwrap(),
                    Execution::default()
                )
                .is_err()
            );
        }
    }
}
