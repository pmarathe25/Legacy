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
        sim.state.scenario = ScenarioInfo {
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
        actor: PersonId,
    ) -> Result<game_physiology::traits::Development, Error> {
        let world = self.state.development.as_ref().ok_or(Error::Unavailable)?;
        let maturity = world
            .lives
            .get(&actor)
            .ok_or(Error::Unavailable)?
            .maturity_milli(&world.rules, self.state.time)?;
        let training = self.state.physical_traits.as_ref().map_or(0, |traits| {
            traits.training_from_levels(
                [Skill::Craft, Skill::Farming, Skill::Combat]
                    .into_iter()
                    .map(|skill| self.own_skill_level(actor, skill)),
            )
        });
        Ok(game_physiology::traits::Development {
            maturity_milli: maturity,
            physical_training_milli: training,
        })
    }
    pub fn initialize_genetics(&mut self, config: Config) -> Result<(), Error> {
        let mut content = Content::read(config.content_dir)?;
        if let Some(period) = config.expression_period_ms {
            content.physiology.data.expression_period_ms = period;
        }
        self.initialize_trait_content(content)
    }
    pub fn initialize_trait_content(&mut self, content: Content) -> Result<(), Error> {
        if self.state.genetics.is_some()
            || self.state.physical_traits.is_some()
            || self.state.temperament.is_some()
            || self.state.heredity.is_some()
            || self.state.development.is_none()
            || self.state.decisions.is_none()
            || self.state.physical.is_none()
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
            if self.state.content.contains_key(feature) || !document_ids.insert(stamp.id.clone()) {
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
        let mut genes = game_genetics::World::new(content.genetics.data, loci)?;
        let mut traits =
            game_physiology::traits::World::new(content.physiology.data, self.state.time)?;
        let mut temperament = game_characters::temperament::World::new(content.temperament.data)?;
        let heredity = content.heredity.data;
        for person in &self.state.people {
            let life = &self.state.development.as_ref().unwrap().lives[&person.id];
            genes.seed_person(self.state.seed, person.id)?;
            genes.set_alleles(
                person.id,
                &game_genetics::LocusId(heredity.inheritance.locus.clone()),
                heredity.founder_alleles(life.reproduction)?,
            )?;
            let mut development = self.trait_development(person.id)?;
            development.physical_training_milli = traits.training_from_levels(
                [Skill::Craft, Skill::Farming, Skill::Combat]
                    .into_iter()
                    .map(|skill| self.own_skill_level(person.id, skill)),
            );
            traits.register(person.id, development)?;
            temperament.register(
                person.id,
                (&self.state.decisions.as_ref().unwrap().minds[&person.id].personality).into(),
            )?;
            traits.phenotype_from_genes(person.id, &genes)?;
            temperament.express_from_genes(person.id, &genes)?;
        }
        self.state.genetics = Some(genes);
        self.state.physical_traits = Some(traits);
        self.state.temperament = Some(temperament);
        self.state.heredity = Some(heredity);
        self.state.content.extend(provenance);
        let actors: Vec<_> = self.state.people.iter().map(|person| person.id).collect();
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
        let Some(world) = &mut self.state.genetics else {
            return Ok(());
        };
        let conceived = &self.state.development.as_ref().unwrap().pregnancies[&pregnancy];
        world.conceive(
            self.state.seed,
            pregnancy,
            [conceived.gestator, conceived.partner],
            self.state.time,
        )
    }
    pub(crate) fn record_genetic_birth(
        &mut self,
        pregnancy: PregnancyId,
        child: PersonId,
    ) -> Result<(), Error> {
        let Some(genes) = &mut self.state.genetics else {
            return Ok(());
        };
        genes.born(pregnancy, child, self.state.time)?;
        let role = self
            .state
            .heredity
            .as_ref()
            .unwrap()
            .interpret_genes(child, genes)?;
        self.state
            .development
            .as_mut()
            .unwrap()
            .lives
            .get_mut(&child)
            .unwrap()
            .reproduction = role;
        self.state.physical_traits.as_mut().unwrap().register(
            child,
            game_physiology::traits::Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )?;
        let personality = self
            .state
            .temperament
            .as_mut()
            .unwrap()
            .register_newborn(child)?;
        self.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&child)
            .unwrap()
            .personality = personality;
        self.apply_trait_temperament(child);
        self.apply_genetic_body_change(child, 1_000, self.genetic_health_milli(child));
        Ok(())
    }
    fn apply_trait_temperament(&mut self, actor: PersonId) {
        let Some(temperament) = self.state.temperament.as_ref().and_then(|world| {
            world
                .express_from_genes(actor, self.state.genetics.as_ref()?)
                .ok()
        }) else {
            return;
        };
        if let Some(mind) = self
            .state
            .decisions
            .as_mut()
            .and_then(|world| world.minds.get_mut(&actor))
        {
            temperament.apply_to(&mut mind.personality);
        }
    }
    pub(crate) fn genetic_phenotype(
        &self,
        actor: PersonId,
    ) -> Option<game_physiology::traits::Phenotype> {
        self.state
            .physical_traits
            .as_ref()?
            .phenotype_from_genes(actor, self.state.genetics.as_ref()?)
            .ok()
    }
    pub(crate) fn inherited_traits_view(&self, actor: PersonId) -> Option<game_api::TraitsView> {
        Some(game_api::TraitsView {
            physical: self.genetic_phenotype(actor)?,
            temperament: self
                .state
                .temperament
                .as_ref()?
                .express_from_genes(actor, self.state.genetics.as_ref()?)
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
        if self.state.physical_traits.is_none() {
            return Ok(rate);
        }
        Ok(SkillRate {
            numerator: rate
                .numerator
                .checked_mul(u64::from(self.genetic_strength_milli(actor)))
                .ok_or(Error::Overflow)?,
            denominator: rate.denominator.checked_mul(1_000).ok_or(Error::Overflow)?,
        })
    }
    pub(crate) fn traits_boundary(&self, next: SimTime) -> SimTime {
        self.state
            .physical_traits
            .as_ref()
            .and_then(|world| world.next_boundary(self.state.time, next))
            .unwrap_or(next)
    }
    pub(crate) fn refresh_traits(&mut self) -> Result<(), Error> {
        let Some(traits) = &self.state.physical_traits else {
            return Ok(());
        };
        if traits.next_expression > self.state.time {
            return Ok(());
        }
        let next = traits.prepare_refresh_time(self.state.time)?;
        let pending = traits
            .people
            .keys()
            .copied()
            .map(|actor| {
                self.trait_development(actor)
                    .map(|d| (actor, d, self.genetic_health_milli(actor)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (actor, d, before) in pending {
            self.state.physical_traits.as_mut().unwrap().develop(
                actor,
                d.maturity_milli,
                d.physical_training_milli,
            )?;
            self.apply_genetic_body_change(actor, before, self.genetic_health_milli(actor));
        }
        let committed_next = self
            .state
            .physical_traits
            .as_mut()
            .unwrap()
            .refresh_time(self.state.time)?;
        debug_assert_eq!(committed_next, next);
        Ok(())
    }
    pub(crate) fn on_trait_event(&mut self, kind: &EventKind) {
        if self.state.genetics.is_none() {
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
                self.state
                    .genetics
                    .as_mut()
                    .unwrap()
                    .end_pregnancy(pregnancy, self.state.time);
                Vec::new()
            }
            _ => Vec::new(),
        };
        for (actor, experience) in learning {
            // Removing a learning definition removes that effect; event handling
            // remains available and never changes inherited alleles.
            if self
                .state
                .temperament
                .as_mut()
                .unwrap()
                .learn(actor, experience)
                .is_ok()
            {
                self.apply_trait_temperament(actor);
            }
        }
    }
}

pub(crate) fn phenotype(
    state: &CanonicalState,
    actor: PersonId,
) -> Option<game_physiology::traits::Phenotype> {
    state
        .physical_traits
        .as_ref()?
        .phenotype_from_genes(actor, state.genetics.as_ref()?)
        .ok()
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent composed trait references".into());
    let Some(genes) = &state.genetics else {
        return if state.physical_traits.is_none()
            && state.temperament.is_none()
            && state.heredity.is_none()
        {
            Ok(())
        } else {
            Err(fail())
        };
    };
    game_genetics::validate(genes, state.time)?;
    let traits = state.physical_traits.as_ref().ok_or_else(fail)?;
    let temperament = state.temperament.as_ref().ok_or_else(fail)?;
    let heredity = state.heredity.as_ref().ok_or_else(fail)?;
    traits.validate(state.time)?;
    temperament.validate()?;
    heredity.validate().map_err(|_| fail())?;
    fn check<T: Serialize>(state: &CanonicalState, feature: &str, data: &T) -> Result<(), Error> {
        let stamp = state
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
    let development = state.development.as_ref().ok_or_else(fail)?;
    if genes.people.len() != state.people.len()
        || traits.people.len() != genes.people.len()
        || temperament.people.len() != genes.people.len()
        || state.people.iter().any(|p| {
            !genes.people.contains_key(&p.id)
                || !traits.people.contains_key(&p.id)
                || !temperament.people.contains_key(&p.id)
        })
    {
        return Err(fail());
    }
    let mut loci = traits.definitions.loci();
    loci.extend(temperament.definitions.loci());
    loci.extend(heredity.loci());
    if game_genetics::World::new(genes.rules.clone(), loci)
        .map_err(|_| fail())?
        .definitions
        != genes.definitions
    {
        return Err(fail());
    }
    for (actor, life) in &development.lives {
        let role = heredity
            .interpret_genes(*actor, genes)
            .map_err(|_| fail())?;
        if life.reproduction != development::Reproduction::Neither && life.reproduction != role {
            return Err(fail());
        }
        traits
            .phenotype_from_genes(*actor, genes)
            .map_err(|_| fail())?;
        temperament
            .express_from_genes(*actor, genes)
            .map_err(|_| fail())?;
    }
    for (id, inheritance) in &genes.pregnancies {
        let pregnancy = development.pregnancies.get(id).ok_or_else(fail)?;
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
    for (id, pregnancy) in &development.pregnancies {
        if !matches!(
            pregnancy.state,
            development::PregnancyState::Conceiving { .. } | development::PregnancyState::Ended
        ) && !genes.pregnancies.contains_key(id)
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
        let actor = original.state.ruler;
        let locus = strength_locus(&original);
        original
            .state
            .genetics
            .as_mut()
            .unwrap()
            .set_alleles(actor, &locus, [1_000; 2])
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
            .state
            .genetics
            .as_mut()
            .unwrap()
            .set_alleles(actor, &locus, [1_000; 2])
            .unwrap();
        assert!(edited.body_rules(actor).carrying_capacity > original_capacity);
        assert_ne!(
            edited.state.content["physiology"],
            original.state.content["physiology"]
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
        assert_eq!(original.state, restored.state);
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
        assert_eq!(sim.genetic_cold_tolerance_centi(sim.state.ruler), 0);
        assert!(
            !sim.state
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
        let mut corrupted = sim.state.clone();
        corrupted
            .physical_traits
            .as_mut()
            .unwrap()
            .definitions
            .traits[0]
            .spread += 1;
        assert!(validate(&corrupted).is_err());
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
        let player = sim.state.ruler;
        let mother = sim.state.people[3].id;
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
        let pregnancy = *sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .keys()
            .next_back()
            .unwrap();
        (mother, pregnancy)
    }
    #[test]
    fn actual_conception_freezes_both_biological_parents_and_birth_survives_paternal_death_and_save()
     {
        let mut sim = fixture(1);
        let player = sim.state.ruler;
        let (mother, pregnancy) = conceive_with_player(&mut sim);
        assert!(sim.state.genetics.as_ref().unwrap().pregnancies.is_empty());
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
        assert_eq!(sim.state, restored.state);
        let frozen = sim.state.genetics.as_ref().unwrap().pregnancies[&pregnancy]
            .genome
            .clone()
            .unwrap();
        assert_eq!(
            sim.state.genetics.as_ref().unwrap().pregnancies[&pregnancy].parents,
            [mother, player]
        );
        sim.debug_injure(player, sim.body_rules(player).max_health)
            .unwrap();
        sim.state
            .temperament
            .as_mut()
            .unwrap()
            .learn(mother, "gave_care")
            .unwrap();
        sim.state
            .physical_traits
            .as_mut()
            .unwrap()
            .develop(mother, 1_000, 200)
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
        assert_eq!(sim.state, restored.state);
        let child = sim.state.genetics.as_ref().unwrap().pregnancies[&pregnancy]
            .born
            .unwrap();
        assert_eq!(
            sim.state.genetics.as_ref().unwrap().people[&child].genome,
            frozen
        );
        assert_eq!(
            sim.state.development.as_ref().unwrap().lives[&child].parents,
            [mother, player]
        );
        assert!(
            sim.own_skill_view(child)
                .iter()
                .all(|skill| skill.practice_ms == 0)
        );
        assert!(
            sim.state.decisions.as_ref().unwrap().minds[&child]
                .knowledge
                .values()
                .all(|fact| fact.source == KnowledgeSource::Observed(child)
                    && fact.observed_at
                        >= sim.state.development.as_ref().unwrap().lives[&child].born)
        );
        assert_eq!(
            sim.state.inventories[&sim.state.people[sim.person_index[&child]].inventory]
                .quantity
                .0,
            0
        );
        assert_eq!(
            sim.state.decisions.as_ref().unwrap().minds[&child]
                .personality
                .honesty,
            500
        );
        assert_eq!(
            sim.state
                .components
                .body(child)
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
        assert_eq!(whole.state, sliced.state);
        assert!(
            whole
                .state
                .genetics
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
                .any(|pregnancy| pregnancy.born.is_some())
        );
    }
    #[test]
    fn genuine_strength_variants_change_carry_capacity_and_actual_work_performance() {
        fn configured(high: bool) -> Simulation {
            let mut sim = fixture(1);
            let actor = sim.state.ruler;
            let locus = sim
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
            sim.state
                .genetics
                .as_mut()
                .unwrap()
                .set_alleles(
                    actor,
                    &game_genetics::LocusId(locus),
                    [if high { 1_000 } else { 0 }; 2],
                )
                .unwrap();
            sim
        }
        let mut weak = configured(false);
        let mut strong = configured(true);
        let actor = weak.state.ruler;
        assert!(
            strong.body_rules(actor).carrying_capacity > weak.body_rules(actor).carrying_capacity
        );
        let food = weak
            .state
            .inventories
            .values()
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
        let site = weak
            .state
            .components
            .sites()
            .map(|(_, site)| site)
            .find(|site| site.area == weak.state.areas[0].id)
            .unwrap()
            .id;
        weak.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        strong
            .debug_actor_command(actor, Command::Work { site })
            .unwrap();
        let weak_job = weak.state.components.job(actor).unwrap();
        let strong_job = strong.state.components.job(actor).unwrap();
        let weak_duration = skills::work_duration(weak_job.remaining, weak_job.rate, 0).unwrap();
        let strong_duration =
            skills::work_duration(strong_job.remaining, strong_job.rate, 0).unwrap();
        assert!(strong_duration < weak_duration);
        weak.advance(SimDuration(strong_duration)).unwrap();
        strong.advance(SimDuration(strong_duration)).unwrap();
        assert!(!weak.state.events.iter().any(|event| matches!(event.kind, EventKind::Produced { actor: worker, .. } if worker == actor)));
        assert!(strong.state.events.iter().any(|event| matches!(event.kind, EventKind::Produced { actor: worker, .. } if worker == actor)));
    }
}
