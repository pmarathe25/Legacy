//! Checked consent-based teaching and learning actions.
#[macro_export]
macro_rules! teaching_methods {
    () => {
        /// Offer a real lesson through a consenting local encounter and one world tick.
        pub fn teach(
            &mut self,
            actor: u64,
            target: u64,
            recipe: u32,
        ) -> Result<bool, &'static str> {
            if self.template.lineage.as_ref().is_some_and(|lineage| {
                lineage.current == Some(actor)
                    && lineage.controlled_person(self.template.tick) != Some(actor)
            }) {
                return Err("player control is suspended");
            }
            let skills = self.template.skills.as_ref().ok_or("skills are disabled")?;
            let recipe = skills.canonical(recipe);
            if !skills.rules.iter().any(|rule| rule.recipe == recipe) {
                return Err("skill is not teachable");
            }
            let index = skills.index();
            let own = index.rate(actor, recipe);
            let (teacher_rate, source_life) = self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.invocation.as_ref())
                .filter(|invocation| invocation.owner == Some(actor))
                .map_or((own, None), |invocation| {
                    invocation
                        .active
                        .iter()
                        .fold((own, None), |(rate, source), &life| {
                            let candidate = index.rate(life, recipe);
                            if candidate > rate
                                || (candidate == rate && source.is_some_and(|old| life < old))
                            {
                                (candidate, Some(life))
                            } else {
                                (rate, source)
                            }
                        })
                });
            if teacher_rate <= index.rate(target, recipe) {
                return Err("teacher must have a higher skill rate");
            }
            let mut draft = self.prepare_inner(Some((actor, target)))?;
            let accepted = draft
                .decisions
                .conversation_accepted
                .ok_or("conversation was not prepared")?;
            if accepted {
                let skills = draft.state.skills.get_or_insert_with(|| {
                    let mut skills = self
                        .template
                        .skills
                        .as_ref()
                        .expect("enabled skills")
                        .clone();
                    skills.events.clear();
                    skills
                });
                skills.teach(actor, target, recipe, draft.tick, teacher_rate, source_life)?;
            }
            self.commit(draft)?;
            Ok(accepted)
        }
        /// Ask a local NPC teacher for a lesson and advance one real tick.
        pub fn learn(
            &mut self,
            student: u64,
            teacher: u64,
            recipe: u32,
        ) -> Result<bool, &'static str> {
            if self.template.lineage.as_ref().is_some_and(|lineage| {
                lineage.current == Some(student)
                    && lineage.controlled_person(self.template.tick) != Some(student)
            }) {
                return Err("player control is suspended");
            }
            let skills = self.template.skills.as_ref().ok_or("skills are disabled")?;
            let recipe = skills.canonical(recipe);
            if !skills.rules.iter().any(|rule| rule.recipe == recipe) {
                return Err("skill is not teachable");
            }
            let next = self.template.tick.checked_add(1).ok_or("tick overflow")?;
            let teacher_willing = self.template.socially_pressured(teacher, student)
                || skills.teachers.iter().any(|policy| {
                    policy.person == teacher
                        && policy.recipe == recipe
                        && policy.enabled
                        && policy.eligible_at <= next
                });
            let index = skills.index();
            let teacher_rate = index.rate(teacher, recipe);
            let can_learn = teacher_willing && teacher_rate > index.rate(student, recipe);
            let mut draft = self.prepare_inner(Some((student, teacher)))?;
            let accepted = draft
                .decisions
                .conversation_accepted
                .ok_or("conversation was not prepared")?
                && can_learn;
            if accepted {
                let skills = draft.state.skills.get_or_insert_with(|| {
                    let mut skills = self
                        .template
                        .skills
                        .as_ref()
                        .expect("enabled skills")
                        .clone();
                    skills.events.clear();
                    skills
                });
                skills.teach(teacher, student, recipe, draft.tick, teacher_rate, None)?;
            }
            self.commit(draft)?;
            Ok(accepted)
        }
    };
}
