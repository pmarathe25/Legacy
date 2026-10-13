//! Archival minds and closing skills publish through guarded component batches.
use crate::*;
use game_characters::decisions::Mind;
use game_core::*;
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use game_society::institutions::{InstitutionView, SocialCaseView};
use std::{collections::BTreeSet, marker::PhantomData};

pub trait ArchiveFacts<A> {
    fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)>;
    fn mind(&self, person: PersonId) -> Option<&Mind<A>>;
    fn stocks(&self, person: PersonId) -> Option<&game_characters::decisions::Stocks>;
    fn skills(&self, person: PersonId) -> Vec<SkillView>;
    fn institutions(&self, person: PersonId) -> Vec<InstitutionView>;
    fn cases(&self, person: PersonId) -> Vec<SocialCaseView>;
}
#[derive(Debug)]
pub struct PreparedArchives<A> {
    now: SimTime,
    incarnations: Vec<(PersonId, Option<SimTime>)>,
    captures: Vec<PastLife<A>>,
    refreshes: Vec<ArchiveSkills>,
}
#[derive(Debug)]
struct ArchiveSkills {
    person: PersonId,
    died_at: SimTime,
    before: Vec<SkillView>,
    after: Vec<SkillView>,
}
#[derive(Debug)]
pub struct ArchiveFinalization<A> {
    action: PhantomData<A>,
    reads: ReadGuard,
}
fn valid_skills(skills: &[SkillView]) -> bool {
    let mut seen = BTreeSet::new();
    skills
        .iter()
        .all(|skill| seen.insert(skill.skill) && skill.level <= 100)
}
impl Runtime {
    pub fn prepare_archives<A: Component>(
        &self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        now: SimTime,
    ) -> Result<PreparedArchives<A>, Error> {
        self.prepare_archives_selected(store, facts, now, None)
    }
    pub fn archive_sources_for_death<A: Component>(
        &self,
        store: &Store,
        incarnations: impl IntoIterator<Item = (PersonId, Option<SimTime>)>,
        actor: PersonId,
        now: SimTime,
    ) -> Vec<PersonId> {
        incarnations
            .into_iter()
            .filter_map(|(person, death)| death.map(|death| (person, death)))
            .filter(|(person, death)| {
                *person == actor && (archive::<A>(store, *person).is_none() || *death == now)
            })
            .map(|(person, _)| person)
            .collect()
    }
    pub fn prepare_death_archives<A: Component>(
        &self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedArchives<A>, Error> {
        self.prepare_archives_selected(store, facts, now, Some(actor))
    }
    fn prepare_archives_selected<A: Component>(
        &self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        now: SimTime,
        selected: Option<PersonId>,
    ) -> Result<PreparedArchives<A>, Error> {
        let incarnations = facts.incarnations().collect::<Vec<_>>();
        let mut captures = Vec::new();
        let mut refreshes = Vec::new();
        for (person, died_at) in incarnations
            .iter()
            .copied()
            .filter_map(|(person, death)| death.map(|death| (person, death)))
            .filter(|(person, _)| selected.is_none_or(|actor| *person == actor))
        {
            if let Some(past) = archive::<A>(store, person) {
                if died_at == now {
                    let skills = facts.skills(person);
                    if !valid_skills(&skills) {
                        return Err(Error::InvalidConfiguration(
                            "invalid closing historical skills".into(),
                        ));
                    }
                    if skills != past.skills {
                        refreshes.push(ArchiveSkills {
                            person,
                            died_at,
                            before: past.skills.clone(),
                            after: skills,
                        });
                    }
                }
            } else {
                if died_at > now {
                    return Err(Error::Unavailable);
                }
                let snapshot = PastLife {
                    person,
                    died_at,
                    mind: facts.mind(person).ok_or(Error::Unavailable)?.clone(),
                    stocks: facts.stocks(person).ok_or(Error::Unavailable)?.clone(),
                    skills: facts.skills(person),
                    institutions: facts.institutions(person),
                    cases: facts.cases(person),
                };
                snapshot.validate()?;
                captures.push(snapshot);
            }
        }
        Ok(PreparedArchives {
            now,
            incarnations,
            captures,
            refreshes,
        })
    }
}
impl<A: Component> PreparedArchives<A> {
    pub fn stage_into(
        self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<ArchiveFinalization<A>, Error> {
        self.stage_sources_into(store, facts, now, batch, false)
    }
    pub fn stage_projected_into(
        self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<ArchiveFinalization<A>, Error> {
        self.stage_sources_into(store, facts, now, batch, true)
    }
    fn stage_sources_into(
        self,
        store: &Store,
        facts: &impl ArchiveFacts<A>,
        now: SimTime,
        batch: &mut Batch,
        projected: bool,
    ) -> Result<ArchiveFinalization<A>, Error> {
        if self.now != now || facts.incarnations().collect::<Vec<_>>() != self.incarnations {
            return Err(Error::Unavailable);
        }
        for snapshot in &self.captures {
            if archive::<A>(store, snapshot.person).is_some()
                || facts.skills(snapshot.person) != snapshot.skills
                || facts.mind(snapshot.person) != Some(&snapshot.mind)
                || facts.stocks(snapshot.person) != Some(&snapshot.stocks)
                || facts.institutions(snapshot.person) != snapshot.institutions
                || facts.cases(snapshot.person) != snapshot.cases
            {
                return Err(Error::Unavailable);
            }
        }
        for refresh in &self.refreshes {
            let past = archive::<A>(store, refresh.person).ok_or(Error::Unavailable)?;
            if past.died_at != refresh.died_at
                || past.skills != refresh.before
                || facts.skills(refresh.person) != refresh.after
            {
                return Err(Error::Unavailable);
            }
        }
        let mut reads = ReadSet::new();
        let mut admitted = ReadFootprint::new();
        for snapshot in &self.captures {
            let id = StableId(snapshot.person.0);
            reads.read::<PastLife<A>>(store, id);
            reads.read::<Mind<A>>(store, id);
            reads.read::<game_characters::decisions::Stocks>(store, id);
            if projected {
                if batch.projected::<Mind<A>>(store, id)? != Some(&snapshot.mind)
                    || batch.projected::<game_characters::decisions::Stocks>(store, id)?
                        != Some(&snapshot.stocks)
                {
                    return Err(Error::Unavailable);
                }
                if batch.touches::<Mind<A>>(id) {
                    admitted.protect::<Mind<A>>(id);
                }
                if batch.touches::<game_characters::decisions::Stocks>(id) {
                    admitted.protect::<game_characters::decisions::Stocks>(id);
                }
            }
            admitted.protect::<PastLife<A>>(id);
        }
        for refresh in &self.refreshes {
            let id = StableId(refresh.person.0);
            reads.read::<PastLife<A>>(store, id);
            admitted.protect::<PastLife<A>>(id);
        }
        for snapshot in self.captures {
            batch.insert(StableId(snapshot.person.0), snapshot);
        }
        for refresh in self.refreshes {
            let mut snapshot = archive::<A>(store, refresh.person)
                .ok_or(Error::Unavailable)?
                .clone();
            snapshot.skills = refresh.after;
            snapshot.validate()?;
            batch.insert(StableId(refresh.person.0), snapshot);
        }
        Ok(ArchiveFinalization {
            action: PhantomData,
            reads: reads.bind(store, batch, &admitted)?,
        })
    }
}
impl<A: Component> ArchiveFinalization<A> {
    pub fn guard(&self) -> &ReadGuard {
        &self.reads
    }
    /// Closing skills cannot compose with another write/removal of that archive.
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.reads.validate(store, batch)?;
        store.validate_batch(batch)
    }
    pub fn finish(self, _: &mut Store) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_start::tests::{ACTOR, fixture};

    struct Facts<'a> {
        store: &'a Store,
        skills: Vec<SkillView>,
    }
    impl ArchiveFacts<u8> for Facts<'_> {
        fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)> {
            [(ACTOR, Some(SimTime(0)))].into_iter()
        }
        fn mind(&self, person: PersonId) -> Option<&Mind<u8>> {
            game_characters::decisions::mind(self.store, person)
        }
        fn stocks(&self, person: PersonId) -> Option<&game_characters::decisions::Stocks> {
            game_characters::decisions::stocks(self.store, person)
        }
        fn skills(&self, _: PersonId) -> Vec<SkillView> {
            self.skills.clone()
        }
        fn institutions(&self, _: PersonId) -> Vec<InstitutionView> {
            Vec::new()
        }
        fn cases(&self, _: PersonId) -> Vec<SocialCaseView> {
            Vec::new()
        }
    }
    #[test]
    fn archival_capture_and_closing_skills_use_atomic_owner_projections_after_restore() {
        let (runtime, mut store, _) = fixture();
        for attack in 0..2 {
            let facts = Facts {
                store: &store,
                skills: vec![],
            };
            let plan = runtime
                .prepare_archives(&store, &facts, SimTime(0))
                .unwrap();
            let before = store.clone();
            let mut batch = Batch::new();
            let finish = plan
                .stage_into(&store, &facts, SimTime(0), &mut batch)
                .unwrap();
            if attack == 0 {
                batch.remove::<Mind<u8>>(StableId(ACTOR.0));
            } else {
                batch.remove::<PastLife<u8>>(StableId(ACTOR.0));
            }
            assert!(finish.validate_batch(&store, &batch).is_err());
            assert_eq!(store, before);
        }
        let facts = Facts {
            store: &store,
            skills: vec![],
        };
        let plan = runtime
            .prepare_archives(&store, &facts, SimTime(0))
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into(&store, &facts, SimTime(0), &mut batch)
            .unwrap();
        finish.validate_batch(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        finish.finish(&mut store);
        let mut restored =
            Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
        for store in [&mut store, &mut restored] {
            let skills = vec![SkillView {
                skill: Skill::Craft,
                level: 7,
                practice_ms: 0,
                progress_ms: 0,
            }];
            let facts = Facts {
                store,
                skills: skills.clone(),
            };
            let plan = runtime.prepare_archives(store, &facts, SimTime(0)).unwrap();
            let mut batch = Batch::new();
            let finish = plan
                .stage_into(store, &facts, SimTime(0), &mut batch)
                .unwrap();
            finish.validate_batch(store, &batch).unwrap();
            store.apply_batch(batch).unwrap();
            assert_eq!(archive::<u8>(store, ACTOR).unwrap().skills, skills);
            let published = store.clone();
            finish.finish(store);
            assert_eq!(*store, published);
        }
        assert_eq!(store, restored);
    }

    #[test]
    fn closing_skill_refresh_is_guarded_and_schema_admitted_before_body_publication() {
        let (runtime, source, _) = fixture();
        let mut past = archive::<u8>(&source, crate::memory_start::tests::LIFE)
            .unwrap()
            .clone();
        past.person = ACTOR;
        past.died_at = SimTime(0);
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<PastLife<u8>>("invocation/archive", |past| {
                past.validate()?;
                if past.skills.iter().any(|skill| skill.level > 5) {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        game_physiology::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        store.insert_or_spawn(StableId(ACTOR.0), past).unwrap();
        let body = game_physiology::Body {
            vitals: Vitals {
                alive: true,
                health: 1000,
                hunger: 0,
                fatigue: 0,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        };
        store
            .insert_or_spawn(StableId(ACTOR.0), body.clone())
            .unwrap();
        for level in [7, 5] {
            let facts = Facts {
                store: &store,
                skills: vec![SkillView {
                    skill: Skill::Craft,
                    level,
                    practice_ms: 0,
                    progress_ms: 0,
                }],
            };
            let plan = runtime
                .prepare_archives(&store, &facts, SimTime(0))
                .unwrap();
            let before = store.clone();
            let mut batch = Batch::new();
            let mut dead = body.clone();
            dead.vitals.alive = false;
            dead.vitals.health = 0;
            batch.insert(StableId(ACTOR.0), dead);
            let receipt = plan
                .stage_into(&store, &facts, SimTime(0), &mut batch)
                .unwrap();
            assert_eq!(
                batch
                    .projected::<PastLife<u8>>(&store, StableId(ACTOR.0))
                    .unwrap()
                    .unwrap()
                    .skills,
                facts.skills
            );
            if level == 7 {
                assert!(receipt.validate_batch(&store, &batch).is_err());
                assert!(store.apply_batch(batch).is_err());
                assert_eq!(store, before);
            } else {
                receipt.validate_batch(&store, &batch).unwrap();
                batch.remove::<PastLife<u8>>(StableId(ACTOR.0));
                assert!(receipt.validate_batch(&store, &batch).is_err());
                assert_eq!(store, before);
            }
        }
    }
}

#[cfg(test)]
mod projected_tests {
    use super::*;
    use crate::memory_start::tests::{ACTOR, fixture};
    use game_characters::decisions::{Aspiration, Stocks};

    struct Facts {
        mind: Mind<u8>,
        stocks: Stocks,
    }
    impl ArchiveFacts<u8> for Facts {
        fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)> {
            [(ACTOR, Some(SimTime(0))), (PersonId(50), Some(SimTime(0)))].into_iter()
        }
        fn mind(&self, actor: PersonId) -> Option<&Mind<u8>> {
            assert_eq!(actor, ACTOR);
            Some(&self.mind)
        }
        fn stocks(&self, actor: PersonId) -> Option<&Stocks> {
            assert_eq!(actor, ACTOR);
            Some(&self.stocks)
        }
        fn skills(&self, _: PersonId) -> Vec<SkillView> {
            vec![]
        }
        fn institutions(&self, _: PersonId) -> Vec<InstitutionView> {
            vec![]
        }
        fn cases(&self, _: PersonId) -> Vec<SocialCaseView> {
            vec![]
        }
    }
    #[test]
    fn death_capture_uses_exact_projected_mind_and_guards_it_without_capturing_other_actors() {
        let (runtime, mut store, _) = fixture();
        let mut mind = game_characters::decisions::mind::<u8>(&store, ACTOR)
            .unwrap()
            .clone();
        mind.dream = Aspiration::Comfort;
        let facts = Facts {
            mind: mind.clone(),
            stocks: game_characters::decisions::stocks(&store, ACTOR)
                .unwrap()
                .clone(),
        };
        let mut batch = Batch::new();
        batch.insert(StableId(ACTOR.0), mind.clone());
        let plan = runtime
            .prepare_death_archives(&store, &facts, ACTOR, SimTime(0))
            .unwrap();
        let receipt = plan
            .stage_projected_into(&store, &facts, SimTime(0), &mut batch)
            .unwrap();
        receipt.validate_batch(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        receipt.finish(&mut store);
        assert_eq!(archive::<u8>(&store, ACTOR).unwrap().mind, mind);
        assert!(archive::<u8>(&store, PersonId(50)).is_none());
        let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
        assert_eq!(store, restored);
    }
}
