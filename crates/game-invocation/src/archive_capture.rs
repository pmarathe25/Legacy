//! Archival minds are captured once at death; closing skills use sparse receipts.
use crate::*;
use game_characters::decisions::Mind;
use game_core::*;
use game_ecs::{Batch, Component, StableId, Store};
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
    refreshes: Vec<ArchiveSkills>,
    action: PhantomData<A>,
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
        let incarnations = facts.incarnations().collect::<Vec<_>>();
        let mut captures = Vec::new();
        let mut refreshes = Vec::new();
        for (person, died_at) in incarnations
            .iter()
            .copied()
            .filter_map(|(person, death)| death.map(|death| (person, death)))
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
        for snapshot in self.captures {
            batch.insert(StableId(snapshot.person.0), snapshot);
        }
        Ok(ArchiveFinalization {
            refreshes: self.refreshes,
            action: PhantomData,
        })
    }
}
impl<A: Component> ArchiveFinalization<A> {
    /// Closing skills cannot compose with another write/removal of that archive.
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        for refresh in &self.refreshes {
            let past = archive::<A>(store, refresh.person).ok_or(Error::Unavailable)?;
            if past.died_at != refresh.died_at
                || past.skills != refresh.before
                || batch.touches::<PastLife<A>>(StableId(refresh.person.0))
            {
                return Err(Error::Unavailable);
            }
        }
        store.validate_batch(batch)
    }
    pub fn finish(self, store: &mut Store) {
        for refresh in self.refreshes {
            archive_mut::<A>(store, refresh.person)
                .expect("validated historical skills")
                .skills = refresh.after;
        }
    }
}
