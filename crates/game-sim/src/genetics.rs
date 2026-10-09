//! Coordinator adapters for the independent genetic domain. Actual biological
//! parents provide genomes; caregivers contribute experiences, never DNA.
use crate::*;
pub use game_genetics::{Expression, Learning, Phenotype, Rules, Sex, Temperament, World};

impl Simulation {
    pub fn genetics_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "genetics-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_genetics(Rules {
            expression_period_ms: MINUTE.0,
            ..Rules::default()
        })?;
        Ok(sim)
    }
    fn genetic_development(&self, actor: PersonId) -> Result<(u16, u16), Error> {
        let world = self.state.development.as_ref().ok_or(Error::Unavailable)?;
        let age = world
            .lives
            .get(&actor)
            .ok_or(Error::Unavailable)?
            .age_at(self.state.time)?;
        let maturity =
            (u128::from(age) * 1_000 / u128::from(world.rules.adulthood.0)).min(1_000) as u16;
        // Real personal practice conditions the body. Borrowed mastery never
        // enters this input, and an offspring starts with no training.
        let training = [Skill::Craft, Skill::Farming, Skill::Combat]
            .into_iter()
            .map(|skill| u32::from(self.own_skill_level(actor, skill)))
            .sum::<u32>()
            .saturating_mul(2)
            .min(200) as u16;
        Ok((maturity, training))
    }
    pub fn initialize_genetics(&mut self, rules: Rules) -> Result<(), Error> {
        if self.state.genetics.is_some()
            || self.state.development.is_none()
            || self.state.decisions.is_none()
            || self.state.physical.is_none()
        {
            return Err(Error::InvalidConfiguration(
                "genetics requires fresh human domains".into(),
            ));
        }
        let mut world = World::new(rules, self.state.time)?;
        for person in &self.state.people {
            let life = &self.state.development.as_ref().unwrap().lives[&person.id];
            let sex = match life.reproduction {
                development::Reproduction::Gestates => Sex::Female,
                development::Reproduction::SuppliesSeed => Sex::Male,
                development::Reproduction::Neither => {
                    if person.id.0.is_multiple_of(2) {
                        Sex::Female
                    } else {
                        Sex::Male
                    }
                }
            };
            let personality = &self.state.decisions.as_ref().unwrap().minds[&person.id].personality;
            let baseline = Temperament {
                discipline: personality.discipline,
                sociability: personality.sociability,
                empathy: personality.empathy,
                risk_tolerance: personality.risk_tolerance,
            };
            let (maturity, training) = self.genetic_development(person.id)?;
            world.seed_person(
                self.state.seed,
                person.id,
                sex,
                baseline,
                Expression {
                    maturity_milli: maturity,
                    physical_training_milli: training,
                    ..Expression::default()
                },
            )?;
        }
        self.state.genetics = Some(world);
        let actors: Vec<_> = self.state.people.iter().map(|person| person.id).collect();
        for actor in actors {
            let health = self.genetic_health_milli(actor);
            self.apply_genetic_body_change(actor, 1_000, health);
            self.apply_genetic_temperament(actor);
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
        let Some(world) = &mut self.state.genetics else {
            return Ok(());
        };
        let phenotype = world.born(pregnancy, child, self.state.time)?;
        self.state
            .development
            .as_mut()
            .unwrap()
            .lives
            .get_mut(&child)
            .unwrap()
            .reproduction = match phenotype.sex {
            Sex::Female => development::Reproduction::Gestates,
            Sex::Male => development::Reproduction::SuppliesSeed,
        };
        // Non-encoded attitudes are neutral starting state, not a copy of a
        // biological parent's learned temperament, goals, or life experience.
        let personality = &mut self
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&child)
            .unwrap()
            .personality;
        personality.ambition = 500;
        personality.hedonism = 400;
        personality.aggression = 200;
        personality.honesty = 500;
        personality.patience = 500;
        self.apply_genetic_temperament(child);
        self.apply_genetic_body_change(child, 1_000, phenotype.max_health_milli);
        Ok(())
    }
    fn apply_genetic_temperament(&mut self, actor: PersonId) {
        let Some(phenotype) = self.genetic_phenotype(actor) else {
            return;
        };
        if let Some(mind) = self
            .state
            .decisions
            .as_mut()
            .and_then(|world| world.minds.get_mut(&actor))
        {
            mind.personality.discipline = phenotype.temperament.discipline;
            mind.personality.sociability = phenotype.temperament.sociability;
            mind.personality.empathy = phenotype.temperament.empathy;
            mind.personality.risk_tolerance = phenotype.temperament.risk_tolerance;
        }
    }
    pub(crate) fn genetic_phenotype(&self, actor: PersonId) -> Option<Phenotype> {
        self.state.genetics.as_ref()?.phenotype(actor).ok()
    }
    pub(crate) fn genetic_strength_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |phenotype| phenotype.strength_milli)
    }
    pub(crate) fn genetic_health_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |phenotype| phenotype.max_health_milli)
    }
    pub(crate) fn genetic_metabolism_milli(&self, actor: PersonId) -> u16 {
        self.genetic_phenotype(actor)
            .map_or(1_000, |phenotype| phenotype.metabolism_milli)
    }
    pub(crate) fn genetic_cold_tolerance_centi(&self, actor: PersonId) -> i16 {
        self.genetic_phenotype(actor)
            .map_or(0, |phenotype| phenotype.cold_tolerance_centi)
    }
    pub(crate) fn genetic_work_rate(
        &self,
        actor: PersonId,
        rate: SkillRate,
    ) -> Result<SkillRate, Error> {
        if self.state.genetics.is_none() {
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
    pub(crate) fn genetics_boundary(&self, next: SimTime) -> SimTime {
        self.state
            .genetics
            .as_ref()
            .filter(|world| world.next_expression > self.state.time)
            .map_or(next, |world| next.min(world.next_expression))
    }
    pub(crate) fn refresh_genetics(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.genetics else {
            return Ok(());
        };
        if world.next_expression > self.state.time {
            return Ok(());
        }
        let steps =
            (self.state.time.0 - world.next_expression.0) / world.rules.expression_period_ms + 1;
        let next = SimTime(
            world
                .next_expression
                .0
                .checked_add(
                    steps
                        .checked_mul(world.rules.expression_period_ms)
                        .ok_or(Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?,
        );
        let actors: Vec<_> = world.people.keys().copied().collect();
        let pending: Vec<_> = actors
            .into_iter()
            .map(|actor| {
                self.genetic_development(actor).map(|(maturity, training)| {
                    (actor, maturity, training, self.genetic_health_milli(actor))
                })
            })
            .collect::<Result<_, _>>()?;
        for (actor, maturity, training, before) in pending {
            self.state
                .genetics
                .as_mut()
                .unwrap()
                .develop(actor, maturity, training)?;
            self.apply_genetic_body_change(actor, before, self.genetic_health_milli(actor));
        }
        self.state.genetics.as_mut().unwrap().next_expression = next;
        Ok(())
    }
    pub(crate) fn on_genetic_event(&mut self, kind: &EventKind) {
        if self.state.genetics.is_none() {
            return;
        }
        let learning: Vec<_> = match *kind {
            EventKind::Cared {
                caregiver, child, ..
            }
            | EventKind::Comforted { caregiver, child } => vec![
                (caregiver, Learning::GaveCare),
                (child, Learning::ReceivedCare),
            ],
            EventKind::Socialized { first, second, .. } => {
                vec![(first, Learning::Company), (second, Learning::Company)]
            }
            EventKind::Practiced { actor, .. }
            | EventKind::PracticeCompleted { actor, .. }
            | EventKind::Produced { actor, .. } => vec![(actor, Learning::Practice)],
            EventKind::InstructionCompleted {
                teacher, student, ..
            }
            | EventKind::LessonCompleted {
                teacher, student, ..
            } => vec![(teacher, Learning::Guidance), (student, Learning::Guidance)],
            EventKind::Struck { target, .. } => vec![(target, Learning::ExperiencedHarm)],
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
            self.state
                .genetics
                .as_mut()
                .unwrap()
                .learn(actor, experience);
            self.apply_genetic_temperament(actor);
        }
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.genetics else {
        return Ok(());
    };
    game_genetics::validate(world, state.time)?;
    let fail = || Error::InvalidSave("inconsistent genetic world references".into());
    let development = state.development.as_ref().ok_or_else(fail)?;
    if world.people.len() != state.people.len()
        || state
            .people
            .iter()
            .any(|person| !world.people.contains_key(&person.id))
    {
        return Err(fail());
    }
    for (actor, life) in &development.lives {
        let sex = world
            .people
            .get(actor)
            .ok_or_else(fail)?
            .genome
            .sex()
            .map_err(|_| fail())?;
        if matches!(
            (life.reproduction, sex),
            (development::Reproduction::Gestates, Sex::Male)
                | (development::Reproduction::SuppliesSeed, Sex::Female)
        ) {
            return Err(fail());
        }
    }
    for (id, inheritance) in &world.pregnancies {
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
        ) && !world.pregnancies.contains_key(id)
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
            .genetics
            .as_mut()
            .unwrap()
            .learn(mother, Learning::GaveCare);
        sim.state
            .genetics
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
            sim.state.physical.as_ref().unwrap().actors[&child]
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
            let genome = &mut sim
                .state
                .genetics
                .as_mut()
                .unwrap()
                .people
                .get_mut(&actor)
                .unwrap()
                .genome;
            for homolog in &mut genome.autosomes[0].homologs {
                homolog.variants[0] = game_genetics::VariantId(if high { 21 } else { 1 });
            }
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
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|site| site.area == weak.state.areas[0].id)
            .unwrap()
            .id;
        weak.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        strong
            .debug_actor_command(actor, Command::Work { site })
            .unwrap();
        let weak_job = weak.state.physical.as_ref().unwrap().actors[&actor]
            .job
            .as_ref()
            .unwrap();
        let strong_job = strong.state.physical.as_ref().unwrap().actors[&actor]
            .job
            .as_ref()
            .unwrap();
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
