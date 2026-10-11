//! Composition bindings. Feature owners interpret and develop their own traits;
//! the genetic feature supplies only raw, immutable inherited alleles.
use crate::*;
use game_genetics::HeritableCharacteristic;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub content_dir: PathBuf,
    pub expression_period_ms: Option<u64>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            content_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/human"),
            expression_period_ms: None,
        }
    }
}

/// Loaded definitions can be configured before admission; each owner validates
/// its document and the composition checks that locus bindings agree.
pub struct Content {
    pub genetics: game_content::Document<game_genetics::Rules>,
    pub physiology: game_content::Document<game_physiology::traits::Definitions>,
    pub temperament: game_content::Document<game_characters::temperament::Definitions>,
    pub heredity: game_content::Document<game_lifecycle::heredity::Definition>,
}
impl Content {
    pub fn read(dir: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = dir.as_ref();
        Ok(Self {
            genetics: game_content::read(dir.join("genetics.toml"))?,
            physiology: game_content::read(dir.join("physiology.toml"))?,
            temperament: game_content::read(dir.join("temperament.toml"))?,
            heredity: game_content::read(dir.join("heredity.toml"))?,
        })
    }
}

impl Simulation {
    pub fn genetics_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "genetics-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_genetics(Config {
            expression_period_ms: Some(MINUTE.0),
            ..Config::default()
        })?;
        Ok(sim)
    }
    fn trait_development(
        &self,
        traits: &game_physiology::traits::Runtime,
        actor: PersonId,
    ) -> Result<game_physiology::traits::Development, Error> {
        let world = self
            .host
            .state
            .development
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let maturity = game_lifecycle::life(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?
            .maturity_milli(&world.rules, self.host.clock().time)?;
        let levels: Vec<_> = Skill::ALL
            .into_iter()
            .map(|skill| (skill, self.own_skill_level(actor, skill)))
            .collect();
        traits.development_from_facts(maturity, &levels)
    }
    pub fn initialize_genetics(&mut self, config: Config) -> Result<(), Error> {
        let mut content = Content::read(config.content_dir)?;
        if let Some(period) = config.expression_period_ms {
            content.physiology.data.expression_period_ms = period;
        }
        self.initialize_trait_content(content)
    }
    pub fn initialize_trait_content(&mut self, content: Content) -> Result<(), Error> {
        if self.host.state.genetics.is_some()
            || self.host.state.physical_traits.is_some()
            || self.host.state.temperament.is_some()
            || self.host.state.heredity.is_some()
            || self.host.state.development.is_none()
            || self.host.state.decisions.is_none()
            || self.host.state.physical.is_none()
            || game_genetics::all_person_genomes(&self.host.storage)
                .next()
                .is_some()
            || game_genetics::all_inheritances(&self.host.storage)
                .next()
                .is_some()
            || game_physiology::traits::all_developments(&self.host.storage)
                .next()
                .is_some()
            || game_characters::temperament::all_person_temperaments(&self.host.storage)
                .next()
                .is_some()
        {
            return Err(Error::InvalidConfiguration(
                "trait composition requires fresh human domains".into(),
            ));
        }
        let stamps = [
            ("genetics", content.genetics.stamp()?),
            ("physiology", content.physiology.stamp()?),
            ("temperament", content.temperament.stamp()?),
            ("heredity", content.heredity.stamp()?),
        ];
        let mut provenance = BTreeMap::new();
        let mut document_ids = BTreeSet::new();
        for (feature, stamp) in stamps {
            if self.host.state.content.contains_key(feature)
                || !document_ids.insert(stamp.id.clone())
            {
                return Err(Error::InvalidConfiguration(
                    "duplicate content binding or document ID".into(),
                ));
            }
            provenance.insert(feature.to_owned(), stamp);
        }
        content.heredity.data.validate()?;
        let mut loci = content.physiology.data.loci();
        loci.extend(content.temperament.data.loci());
        loci.extend(content.heredity.data.loci());
        let mut genes = game_genetics::Runtime::new(content.genetics.data, loci)?;
        let traits =
            game_physiology::traits::Runtime::new(content.physiology.data, self.host.clock().time)?;
        let temperament = game_characters::temperament::Runtime::new(content.temperament.data)?;
        let heredity = content.heredity.data;
        // Stage plain new rows; no founder is visible until the whole batch is valid.
        let mut batch = game_ecs::Batch::new();
        for person in &self.host.state.people {
            let life = &game_lifecycle::life(&self.host.storage, person.id).expect("life exists");
            let founder = genes.prepare_founder(
                self.host.seed(),
                person.id,
                &[(
                    game_genetics::LocusId(heredity.inheritance.locus.clone()),
                    heredity.founder_alleles(life.reproduction)?,
                )],
            )?;
            genes.apply_founder_variants(&founder)?;
            if heredity
                .interpret_genes(person.id, &genes.genome_source(person.id, &founder.genome))?
                != life.reproduction
                && life.reproduction != development::Reproduction::Neither
            {
                return Err(Error::InvalidConfiguration(
                    "founder reproductive substrate disagrees with its role".into(),
                ));
            }
            let development = self.trait_development(&traits, person.id)?;
            development.validate()?;
            let record = game_characters::temperament::PersonTemperament {
                baseline: (&game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    person.id,
                )
                .expect("mind exists")
                .personality)
                    .into(),
                learned_offsets: [0; 4],
            };
            record.validate()?;
            let id = game_ecs::StableId(person.id.0);
            batch.insert(id, founder.genome);
            batch.insert(id, development);
            batch.insert(id, record);
        }
        self.host.storage.apply_batch(batch)?;
        self.host.state.genetics = Some(genes);
        self.host.state.physical_traits = Some(traits);
        self.host.state.temperament = Some(temperament);
        self.host.state.heredity = Some(heredity);
        self.host.state.content.extend(provenance);
        let actors: Vec<_> = self
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect();
        for actor in actors {
            self.apply_genetic_body_change(actor, 1_000, self.genetic_health_milli(actor));
            self.apply_trait_temperament(actor);
        }
        Ok(())
    }
    pub(crate) fn freeze_genetic_conception(
        &mut self,
        pregnancy: PregnancyId,
    ) -> Result<(), Error> {
        let owner_seed = self.host.seed();
        let owner_now = self.host.clock().time;
        let Some(world) = &mut self.host.state.genetics else {
            return Ok(());
        };
        let parents = {
            let conceived =
                game_lifecycle::pregnancy(&self.host.storage, pregnancy).expect("pregnancy exists");
            [conceived.gestator, conceived.partner]
        };
        world.conceive(
            &mut self.host.storage,
            owner_seed,
            pregnancy,
            parents,
            owner_now,
        )
    }
    pub(crate) fn record_genetic_birth(
        &mut self,
        pregnancy: PregnancyId,
        child: PersonId,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let Some(genes) = &mut self.host.state.genetics else {
            return Ok(());
        };
        genes.born(&mut self.host.storage, pregnancy, child, owner_now)?;
        let role = self
            .host
            .state
            .heredity
            .as_ref()
            .unwrap()
            .interpret_genes(child, &genes.allele_source(&self.host.storage))?;
        game_lifecycle::life_mut(&mut self.host.storage, child)
            .unwrap()
            .reproduction = role;
        self.host.state.physical_traits.as_mut().unwrap().register(
            &mut self.host.storage,
            child,
            game_physiology::traits::Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )?;
        let personality = self
            .host
            .state
            .temperament
            .as_mut()
            .unwrap()
            .register_newborn(&mut self.host.storage, child)?;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut self.host.storage, child)
            .unwrap()
            .personality = personality;
        self.apply_trait_temperament(child);
        self.apply_genetic_body_change(child, 1_000, self.genetic_health_milli(child));
        Ok(())
    }
    fn apply_trait_temperament(&mut self, actor: PersonId) {
        let (Some(temperament), Some(genes)) =
            (&self.host.state.temperament, &self.host.state.genetics)
        else {
            return;
        };
        let Ok(prepared) = temperament.prepare_expression::<Command>(
            &self.host.storage,
            &[actor],
            &genes.allele_source(&self.host.storage),
        ) else {
            return;
        };
        let _ = prepared.commit(
            temperament,
            &mut self.host.storage,
            |store, actor, locus| genes.alleles_named(store, actor, locus),
        );
    }
    pub(crate) fn genetic_phenotype(
        &self,
        actor: PersonId,
    ) -> Option<game_physiology::traits::Phenotype> {
        self.host
            .state
            .physical_traits
            .as_ref()?
            .phenotype_from_genes(
                &self.host.storage,
                actor,
                &self
                    .host
                    .state
                    .genetics
                    .as_ref()?
                    .allele_source(&self.host.storage),
            )
            .ok()
    }
    pub(crate) fn inherited_traits_view(&self, actor: PersonId) -> Option<game_api::TraitsView> {
        Some(game_api::TraitsView {
            physical: self.genetic_phenotype(actor)?,
            temperament: self
                .host
                .state
                .temperament
                .as_ref()?
                .express_from_genes(
                    &self.host.storage,
                    actor,
                    &self
                        .host
                        .state
                        .genetics
                        .as_ref()?
                        .allele_source(&self.host.storage),
                )
                .ok()?,
        })
    }
    pub(crate) fn genetic_strength_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |p| p.strength_milli)
    }
    pub(crate) fn genetic_health_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |p| p.max_health_milli)
    }
    pub(crate) fn genetic_metabolism_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |p| p.metabolism_milli)
    }
    pub(crate) fn genetic_cold_tolerance_centi(&self, actor: PersonId) -> i16 {
        self.genetic_phenotype(actor)
            .map_or(0, |p| p.cold_tolerance_centi)
    }
    pub(crate) fn genetic_work_rate(
        &self,
        actor: PersonId,
        rate: SkillRate,
    ) -> Result<SkillRate, Error> {
        if self.host.state.physical_traits.is_none() {
            return Ok(rate);
        }
        game_physiology::traits::scale_work_rate(rate, self.genetic_strength_milli(actor))
    }

    pub(crate) fn traits_boundary(&self, next: SimTime) -> SimTime {
        self.host
            .state
            .physical_traits
            .as_ref()
            .and_then(|world| world.next_boundary(self.host.clock().time, next))
            .unwrap_or(next)
    }
    pub(crate) fn refresh_traits(&mut self) -> Result<(), Error> {
        let Some(traits) = &self.host.state.physical_traits else {
            return Ok(());
        };
        if !traits.refresh_due(self.host.clock().time) {
            return Ok(());
        }
        let genes = self
            .host
            .state
            .genetics
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let pending = game_physiology::traits::all_developments(&self.host.storage)
            .map(|(actor, _)| actor)
            .map(|actor| {
                self.trait_development(traits, actor)
                    .map(|development| (actor, development))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Some(prepared) = traits.prepare_refresh(
            &self.host.storage,
            self.host.clock().time,
            &pending,
            &genes.allele_source(&self.host.storage),
        )?
        else {
            return Ok(());
        };
        prepared.commit(
            self.host.state.physical_traits.as_mut().unwrap(),
            &mut self.host.storage,
            |store, actor, locus| genes.alleles_named(store, actor, locus),
        )
    }
    pub(crate) fn on_trait_event(&mut self, kind: &EventKind) {
        let owner_now = self.host.clock().time;
        if self.host.state.genetics.is_none() {
            return;
        }
        let learning: Vec<_> = match *kind {
            EventKind::Cared {
                caregiver, child, ..
            }
            | EventKind::Comforted { caregiver, child } => {
                vec![(caregiver, "gave_care"), (child, "received_care")]
            }
            EventKind::Socialized { first, second, .. } => {
                vec![(first, "company"), (second, "company")]
            }
            EventKind::Practiced { actor, .. }
            | EventKind::PracticeCompleted { actor, .. }
            | EventKind::Produced { actor, .. } => vec![(actor, "practice")],
            EventKind::InstructionCompleted {
                teacher, student, ..
            }
            | EventKind::LessonCompleted {
                teacher, student, ..
            } => vec![(teacher, "guidance"), (student, "guidance")],
            EventKind::Struck { target, .. } => vec![(target, "experienced_harm")],
            EventKind::PregnancyEnded { pregnancy } => {
                self.host.state.genetics.as_mut().unwrap().end_pregnancy(
                    &mut self.host.storage,
                    pregnancy,
                    owner_now,
                );
                Vec::new()
            }
            _ => Vec::new(),
        };
        let (Some(temperament), Some(genes)) =
            (&self.host.state.temperament, &self.host.state.genetics)
        else {
            return;
        };
        if let Ok(prepared) = temperament.prepare_learning::<Command>(
            &self.host.storage,
            &learning,
            &genes.allele_source(&self.host.storage),
        ) {
            let _ = prepared.commit(
                temperament,
                &mut self.host.storage,
                |store, actor, locus| genes.alleles_named(store, actor, locus),
            );
        }
    }
}

pub(crate) fn phenotype(
    state: &CanonicalState,
    actor: PersonId,
) -> Option<game_physiology::traits::Phenotype> {
    state
        .state
        .physical_traits
        .as_ref()?
        .phenotype_from_genes(
            state.components,
            actor,
            &state
                .state
                .genetics
                .as_ref()?
                .allele_source(state.components),
        )
        .ok()
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent composed trait references".into());
    let Some(genes) = &state.state.genetics else {
        return if state.state.physical_traits.is_none()
            && state.state.temperament.is_none()
            && state.state.heredity.is_none()
            && game_genetics::all_person_genomes(state.components)
                .next()
                .is_none()
            && game_genetics::all_inheritances(state.components)
                .next()
                .is_none()
            && game_physiology::traits::all_developments(state.components)
                .next()
                .is_none()
            && game_characters::temperament::all_person_temperaments(state.components)
                .next()
                .is_none()
        {
            Ok(())
        } else {
            Err(fail())
        };
    };
    game_genetics::validate(genes, state.components, state.time)?;
    let traits = state.state.physical_traits.as_ref().ok_or_else(fail)?;
    let temperament = state.state.temperament.as_ref().ok_or_else(fail)?;
    let heredity = state.state.heredity.as_ref().ok_or_else(fail)?;
    traits.validate(state.components, state.time)?;
    temperament.validate(state.components)?;
    heredity.validate().map_err(|_| fail())?;
    fn check<T: Serialize>(state: &CanonicalState, feature: &str, data: &T) -> Result<(), Error> {
        let stamp = state
            .state
            .content
            .get(feature)
            .ok_or_else(|| Error::InvalidSave("missing trait content provenance".into()))?;
        let actual = game_content::Document {
            id: stamp.id.clone(),
            version: stamp.version,
            data,
        }
        .stamp()
        .map_err(|_| Error::InvalidSave("invalid trait content provenance".into()))?;
        if actual != *stamp {
            return Err(Error::InvalidSave(
                "trait definitions differ from content provenance".into(),
            ));
        }
        Ok(())
    }
    check(state, "genetics", &genes.rules)?;
    check(state, "physiology", &traits.definitions)?;
    check(state, "temperament", &temperament.definitions)?;
    check(state, "heredity", heredity)?;
    state.state.development.as_ref().ok_or_else(fail)?;
    let people: BTreeSet<_> = state.state.people.iter().map(|person| person.id).collect();
    if game_genetics::all_person_genomes(state.components)
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>()
        != people
        || game_physiology::traits::all_developments(state.components)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != people
        || game_characters::temperament::all_person_temperaments(state.components)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != people
    {
        return Err(fail());
    }
    let mut loci = traits.definitions.loci();
    loci.extend(temperament.definitions.loci());
    loci.extend(heredity.loci());
    if game_genetics::Runtime::new(genes.rules.clone(), loci)
        .map_err(|_| fail())?
        .definitions
        != genes.definitions
    {
        return Err(fail());
    }
    for (actor, life) in game_lifecycle::all_lives(state.components) {
        let role = heredity
            .interpret_genes(actor, &genes.allele_source(state.components))
            .map_err(|_| fail())?;
        if life.reproduction != development::Reproduction::Neither && life.reproduction != role {
            return Err(fail());
        }
        traits
            .phenotype_from_genes(
                state.components,
                actor,
                &genes.allele_source(state.components),
            )
            .map_err(|_| fail())?;
        temperament
            .express_from_genes(
                state.components,
                actor,
                &genes.allele_source(state.components),
            )
            .map_err(|_| fail())?;
    }
    for (id, inheritance) in game_genetics::all_inheritances(state.components) {
        let pregnancy = game_lifecycle::pregnancy(state.components, id).ok_or_else(fail)?;
        if inheritance.parents != [pregnancy.gestator, pregnancy.partner] {
            return Err(fail());
        }
        match pregnancy.state {
            development::PregnancyState::Gestating { .. }
                if inheritance.genome.is_some()
                    && inheritance.born.is_none()
                    && inheritance.ended_at.is_none() => {}
            development::PregnancyState::Born(child) if inheritance.born == Some(child) => {}
            development::PregnancyState::Ended if inheritance.ended_at.is_some() => {}
            _ => return Err(fail()),
        }
    }
    for (id, pregnancy) in game_lifecycle::all_pregnancies(state.components) {
        if !matches!(
            pregnancy.state,
            development::PregnancyState::Conceiving { .. } | development::PregnancyState::Ended
        ) && game_genetics::inheritance(state.components, id).is_none()
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct ContentDir(PathBuf);
    impl ContentDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "legacy-traits-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            for name in [
                "genetics.toml",
                "physiology.toml",
                "temperament.toml",
                "heredity.toml",
            ] {
                std::fs::copy(Config::default().content_dir.join(name), dir.join(name)).unwrap();
            }
            Self(dir)
        }
        fn config(&self) -> Config {
            Config {
                content_dir: self.0.clone(),
                expression_period_ms: Some(MINUTE.0),
            }
        }
    }
    impl Drop for ContentDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn strength_locus(sim: &Simulation) -> game_genetics::LocusId {
        let definition = sim
            .host
            .state
            .physical_traits
            .as_ref()
            .unwrap()
            .definitions
            .traits
            .iter()
            .find(|t| t.kind == game_physiology::traits::PhysicalTrait::Strength)
            .unwrap();
        game_genetics::LocusId(definition.inheritance.locus.clone())
    }

    #[test]
    fn editing_content_on_disk_changes_new_worlds_and_saved_worlds_keep_their_definitions() {
        let dir = ContentDir::new();
        let mut original = Simulation::skills_fixture(19, Execution::default()).unwrap();
        original.initialize_genetics(dir.config()).unwrap();
        let actor = original.host.state.ruler;
        let locus = strength_locus(&original);
        original
            .host
            .state
            .genetics
            .as_mut()
            .unwrap()
            .set_alleles(&mut original.host.storage, actor, &locus, [1_000; 2])
            .unwrap();
        let original_capacity = original.body_rules(actor).carrying_capacity;
        let save = original.save().unwrap();
        let file = dir.0.join("physiology.toml");
        let changed = std::fs::read_to_string(&file).unwrap().replace(
            "kind = \"strength\"\nspread = 200",
            "kind = \"strength\"\nspread = 400",
        );
        std::fs::write(&file, changed).unwrap();
        let mut edited = Simulation::skills_fixture(19, Execution::default()).unwrap();
        edited.initialize_genetics(dir.config()).unwrap();
        let locus = strength_locus(&edited);
        edited
            .host
            .state
            .genetics
            .as_mut()
            .unwrap()
            .set_alleles(&mut edited.host.storage, actor, &locus, [1_000; 2])
            .unwrap();
        assert!(edited.body_rules(actor).carrying_capacity > original_capacity);
        assert_ne!(
            edited.host.state.content["physiology"],
            original.host.state.content["physiology"]
        );
        let mut restored = Simulation::load(
            &save,
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(
            restored.body_rules(actor).carrying_capacity,
            original_capacity
        );
        // Loading and advancing use the saved snapshot even if all source files disappear.
        std::fs::remove_dir_all(&dir.0).unwrap();
        original.advance(MINUTE).unwrap();
        for _ in 0..60 {
            let _ = restored.observe();
            restored.advance(SimDuration(1_000)).unwrap();
        }
        assert_eq!(original.canonical_state(), restored.canonical_state());
    }

    #[test]
    fn removing_an_influence_from_content_removes_its_locus_and_invalid_composition_is_inert() {
        let mut content = Content::read(Config::default().content_dir).unwrap();
        let removed = content
            .physiology
            .data
            .traits
            .iter()
            .find(|t| t.kind == game_physiology::traits::PhysicalTrait::ColdAdaptation)
            .unwrap()
            .inheritance
            .locus
            .clone();
        content
            .physiology
            .data
            .traits
            .retain(|t| t.kind != game_physiology::traits::PhysicalTrait::ColdAdaptation);
        let mut sim = Simulation::skills_fixture(19, Execution::default()).unwrap();
        sim.initialize_trait_content(content).unwrap();
        assert_eq!(sim.genetic_cold_tolerance_centi(sim.host.state.ruler), 0);
        assert!(
            !sim.host
                .state
                .genetics
                .as_ref()
                .unwrap()
                .definitions
                .iter()
                .any(|l| l.id.0 == removed)
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let mut fresh = Simulation::skills_fixture(19, Execution::default()).unwrap();
        let before = fresh.save().unwrap();
        let mut content = Content::read(Config::default().content_dir).unwrap();
        content.temperament.data.traits[0].inheritance.locus =
            content.physiology.data.traits[0].inheritance.locus.clone();
        assert!(fresh.initialize_trait_content(content).is_err());
        assert_eq!(fresh.save().unwrap(), before);
        let mut corrupted = sim.host.state.clone();
        corrupted
            .physical_traits
            .as_mut()
            .unwrap()
            .definitions
            .traits[0]
            .spread += 1;
        assert!(
            validate(&CanonicalState {
                state: &corrupted,
                ..sim.canonical_state()
            })
            .is_err()
        );
    }
    fn fixture(workers: usize) -> Simulation {
        Simulation::genetics_fixture(
            19,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap()
    }
    fn conceive_with_player(sim: &mut Simulation) -> (PersonId, PregnancyId) {
        let player = sim.host.state.ruler;
        let mother = sim.host.state.people[3].id;
        sim.debug_set_relationship(
            mother,
            player,
            decisions::Relationship {
                affection: 800,
                trust: 800,
            },
        )
        .unwrap();
        sim.debug_actor_command(player, Command::Conceive { partner: mother })
            .unwrap();
        let pregnancy = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        (mother, pregnancy)
    }
    #[test]
    fn actual_conception_freezes_both_biological_parents_and_birth_survives_paternal_death_and_save()
     {
        let mut sim = fixture(1);
        let player = sim.host.state.ruler;
        let (mother, pregnancy) = conceive_with_player(&mut sim);
        assert!(
            game_genetics::all_inheritances(&sim.host.storage)
                .next()
                .is_none()
        );
        sim.advance(SimDuration(30_000)).unwrap();
        let before = sim.save().unwrap();
        let mut restored = Simulation::load(
            &before,
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        restored.advance(SimDuration(30_000)).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let frozen = game_genetics::inheritance(&sim.host.storage, pregnancy)
            .unwrap()
            .genome
            .clone()
            .unwrap();
        assert_eq!(
            game_genetics::inheritance(&sim.host.storage, pregnancy)
                .unwrap()
                .parents,
            [mother, player]
        );
        sim.debug_injure(player, sim.body_rules(player).max_health)
            .unwrap();
        sim.host
            .state
            .temperament
            .as_mut()
            .unwrap()
            .learn(&mut sim.host.storage, mother, "gave_care")
            .unwrap();
        sim.host
            .state
            .physical_traits
            .as_mut()
            .unwrap()
            .develop(&mut sim.host.storage, mother, 1_000, 200)
            .unwrap();
        let mid = sim.save().unwrap();
        let mut restored = Simulation::load(
            &mid,
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap();
        let a = sim.advance(SimDuration(180_000)).unwrap();
        let b = restored.advance(SimDuration(180_000)).unwrap();
        assert_eq!(a.reason, b.reason);
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let child = game_genetics::inheritance(&sim.host.storage, pregnancy)
            .unwrap()
            .born
            .unwrap();
        assert_eq!(
            game_genetics::person_genome(&sim.host.storage, child)
                .unwrap()
                .genome,
            frozen
        );
        assert_eq!(
            game_lifecycle::life(&sim.host.storage, child)
                .expect("life exists")
                .parents,
            [mother, player]
        );
        assert!(
            sim.own_skill_view(child)
                .iter()
                .all(|skill| skill.practice_ms == 0)
        );
        assert!(
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, child)
                .expect("mind exists")
                .knowledge
                .values()
                .all(|fact| fact.source == KnowledgeSource::Observed(child)
                    && fact.observed_at
                        >= game_lifecycle::life(&sim.host.storage, child)
                            .expect("life exists")
                            .born)
        );
        assert_eq!(
            game_cargo::inventory(
                &sim.host.storage,
                sim.host.state.people[sim.person_index[&child]].inventory
            )
            .expect("inventory exists")
            .quantity
            .0,
            0
        );
        assert_eq!(
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, child)
                .expect("mind exists")
                .personality
                .honesty,
            500
        );
        assert_eq!(
            game_physiology::body(&sim.host.storage, child)
                .expect("body exists")
                .vitals
                .health,
            sim.body_rules(child).max_health
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn actual_birth_genetics_and_expression_are_worker_chunk_and_query_independent() {
        fn run(workers: usize, sliced: bool) -> Simulation {
            let mut sim = fixture(workers);
            conceive_with_player(&mut sim);
            if sliced {
                for _ in 0..240 {
                    let _ = sim.observe();
                    sim.advance(SimDuration(1_000)).unwrap();
                }
            } else {
                sim.advance(SimDuration(240_000)).unwrap();
            }
            sim
        }
        let whole = run(1, false);
        let sliced = run(4, true);
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        assert!(
            game_genetics::all_inheritances(&whole.host.storage)
                .any(|(_, pregnancy)| pregnancy.born.is_some())
        );
    }
    #[test]
    fn genuine_strength_variants_change_carry_capacity_and_actual_work_performance() {
        fn configured(high: bool) -> Simulation {
            let mut sim = fixture(1);
            let actor = sim.host.state.ruler;
            let locus = sim
                .host
                .state
                .physical_traits
                .as_ref()
                .unwrap()
                .definitions
                .traits
                .iter()
                .find(|definition| {
                    definition.kind == game_physiology::traits::PhysicalTrait::Strength
                })
                .unwrap()
                .inheritance
                .locus
                .clone();
            sim.host
                .state
                .genetics
                .as_mut()
                .unwrap()
                .set_alleles(
                    &mut sim.host.storage,
                    actor,
                    &game_genetics::LocusId(locus),
                    [if high { 1_000 } else { 0 }; 2],
                )
                .unwrap();
            sim
        }
        let mut weak = configured(false);
        let mut strong = configured(true);
        let actor = weak.host.state.ruler;
        assert!(
            strong.body_rules(actor).carrying_capacity > weak.body_rules(actor).carrying_capacity
        );
        let food = game_cargo::inventories(&weak.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Food
                    && stock.quantity.0 >= 9_000
                    && matches!(stock.location, InventoryLocation::Area(_))
            })
            .unwrap()
            .id;
        assert_eq!(
            weak.debug_actor_command(
                actor,
                Command::Carry {
                    source: food,
                    quantity: Quantity(9_000)
                }
            ),
            Err(Error::Unavailable)
        );
        strong
            .debug_actor_command(
                actor,
                Command::Carry {
                    source: food,
                    quantity: Quantity(9_000),
                },
            )
            .unwrap();
        strong.debug_actor_command(actor, Command::Cancel).unwrap();
        let site = game_workshop::sites(&weak.host.storage)
            .map(|(_, site)| site)
            .find(|site| site.area == weak.host.state.areas[0].id)
            .unwrap()
            .id;
        weak.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        strong
            .debug_actor_command(actor, Command::Work { site })
            .unwrap();
        let weak_job = game_workshop::job(&weak.host.storage, actor).unwrap();
        let strong_job = game_workshop::job(&strong.host.storage, actor).unwrap();
        let weak_duration = skills::work_duration(weak_job.remaining, weak_job.rate, 0).unwrap();
        let strong_duration =
            skills::work_duration(strong_job.remaining, strong_job.rate, 0).unwrap();
        assert!(strong_duration < weak_duration);
        weak.advance(SimDuration(strong_duration)).unwrap();
        strong.advance(SimDuration(strong_duration)).unwrap();
        assert!(!weak.host.events().iter().any(|event| matches!(event.event, EventKind::Produced { actor: worker, .. } if worker == actor)));
        assert!(strong.host.events().iter().any(|event| matches!(event.event, EventKind::Produced { actor: worker, .. } if worker == actor)));
    }
}
