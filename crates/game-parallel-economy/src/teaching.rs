//! Explicit lessons use real meetings and individual teaching choices.
use crate::{Economy, OpinionSensitivity, SubjectiveReaction, TeachingReaction, TeachingRole};
use game_parallel_economy_interaction::Request;
use game_parallel_opinions::{Opinions, TeachingExperience};
use rayon::prelude::*;
use std::sync::Arc;

pub(crate) fn interpret(opinions: &mut Opinions, pairs: &[(u64, u64)], tick: u64) {
    let experiences: Vec<_> = pairs
        .par_iter()
        .flat_map_iter(|&(teacher, student)| {
            [
                TeachingExperience {
                    observer: teacher,
                    other: student,
                    role: TeachingRole::Gave,
                },
                TeachingExperience {
                    observer: student,
                    other: teacher,
                    role: TeachingRole::Received,
                },
            ]
        })
        .collect();
    opinions.learn_teaching(&experiences, tick);
}

impl Economy {
    /// Set this person's own interpretation of giving or receiving a lesson.
    pub fn set_teaching_reaction(
        &mut self,
        person: u64,
        role: TeachingRole,
        value: Option<SubjectiveReaction>,
    ) -> Result<(), String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        let mut sensitivity = owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .cloned()
            .unwrap_or_else(|| OpinionSensitivity {
                person,
                ..Default::default()
            });
        let mut reaction = sensitivity
            .teaching_reaction
            .as_deref()
            .copied()
            .unwrap_or_default();
        match role {
            TeachingRole::Gave => reaction.given = value,
            TeachingRole::Received => reaction.received = value,
        }
        sensitivity.teaching_reaction =
            (reaction != TeachingReaction::default()).then(|| Arc::new(reaction));
        self.set_opinion_sensitivity(sensitivity)
    }
    pub fn teaching_reaction(&self, person: u64) -> Result<Option<TeachingReaction>, String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .is_err()
        {
            return Err("unknown person".into());
        }
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        Ok(owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .and_then(|row| row.teaching_reaction.as_deref().copied()))
    }
    /// Offer a lesson to another person during a real meeting.
    pub fn teach(&mut self, teacher: u64, student: u64, recipe: u32) -> Result<bool, String> {
        self.lesson(teacher, student, recipe, true)
    }

    /// Ask another person for a lesson; they may decline or act under pressure.
    pub fn learn(&mut self, student: u64, teacher: u64, recipe: u32) -> Result<bool, String> {
        self.lesson(student, teacher, recipe, false)
    }

    fn lesson(
        &mut self,
        actor: u64,
        target: u64,
        recipe: u32,
        actor_teaches: bool,
    ) -> Result<bool, String> {
        if self.campaign.as_ref().is_some_and(|campaign| {
            campaign.lineage.current == Some(actor)
                && campaign.lineage.controlled_person(self.tick()) != Some(actor)
        }) {
            return Err("player control is suspended".into());
        }
        game_parallel_economy_campaign::preflight!(self);
        let skills = self.skills.as_ref().ok_or("skills are disabled")?;
        let recipe = skills.canonical(recipe);
        if !skills.rules.iter().any(|rule| rule.recipe == recipe) {
            return Err("skill is not teachable".into());
        }
        let tick = self.tick().checked_add(1).ok_or("tick overflow")?;
        let (teacher, student) = if actor_teaches {
            (actor, target)
        } else {
            (target, actor)
        };
        let willing = actor_teaches
            || skills.teachers.iter().any(|policy| {
                policy.person == teacher
                    && policy.recipe == recipe
                    && policy.enabled
                    && policy.eligible_at <= tick
            })
            || self
                .dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .is_some_and(|social| {
                    social
                        .pressures
                        .iter()
                        .find(|policy| policy.person == teacher)
                        .is_some_and(|policy| {
                            self.opinions.as_ref().is_some_and(|opinions| {
                                let (fear, obligation) =
                                    opinions.index().feelings(teacher, student);
                                policy.responds(fear, obligation)
                            })
                        })
                });
        let index = skills.index();
        let mut teacher_rate = index.rate(teacher, recipe);
        let mut source_life = None;
        if let Some(invocation) = self
            .campaign
            .as_ref()
            .and_then(|c| c.lineage.invocation.as_ref())
            .filter(|invocation| invocation.owner == Some(teacher))
        {
            for &life in &invocation.active {
                let candidate = index.rate(life, recipe);
                if candidate > teacher_rate
                    || (candidate == teacher_rate && source_life.is_some_and(|old| life < old))
                {
                    teacher_rate = candidate;
                    source_life = Some(life);
                }
            }
        }
        let can_teach = teacher_rate > index.rate(student, recipe);
        let mut draft =
            self.prepare_interaction(Some(Request { actor, target }), None, None, None)?;
        let met = draft
            .dynamics
            .as_ref()
            .and_then(|d| d.social.as_ref())
            .is_some_and(|social| {
                social
                    .events
                    .iter()
                    .any(|event| event.a == actor.min(target) && event.b == actor.max(target))
            });
        let mut inserted = false;
        let learned = met
            && willing
            && can_teach
            && draft.skills.as_mut().is_some_and(|skills| {
                let existing = skills.lessons.iter().any(|lesson| {
                    lesson.teacher == teacher
                        && lesson.student == student
                        && lesson.recipe == recipe
                        && lesson.tick == tick
                });
                if !existing {
                    inserted = skills
                        .teach(teacher, student, recipe, tick, teacher_rate, source_life)
                        .is_ok();
                }
                existing || inserted
            });
        if inserted
            && let Some(opinions) = &mut draft.opinions
            && opinions.tracks_teaching_reactions()
        {
            interpret(opinions, &[(teacher, student)], tick);
        }
        self.commit(draft)?;
        Ok(learned)
    }
}
