//! Staged human life history, including real newborn custody.
use super::*;

pub(super) struct FamilyFrame {
    pub owner: game_economy_family::Families,
    pub newborns: Vec<u64>,
    pub births: Vec<(u64, u64)>,
}

pub(super) struct ElapsedFamilyFrame {
    frame: FamilyFrame,
    children: Vec<game_economy_family::Birth>,
    events: Vec<game_economy_family::FamilyEvent>,
    preferences: (bool, Option<bool>),
    personality: Option<game_economy_dynamics::personality::Personality>,
    skills: Option<CoreSkills>,
    purpose: Option<CorePurpose>,
    health: Option<CoreHealth>,
}

impl ElapsedFamilyFrame {
    pub(super) fn birth_disease_events(&self, now: u128) -> Vec<game_disease::Event> {
        self.health.as_ref().map_or_else(Vec::new, |health| {
            health
                .owner
                .events
                .par_iter()
                .filter(|event| {
                    event.at_millis == Some(now)
                        && event.kind == game_disease::EventKind::Infected
                        && self.frame.newborns.binary_search(&event.person).is_ok()
                })
                .cloned()
                .collect()
        })
    }

    pub(super) fn physical(&self) -> ElapsedBirths<'_> {
        ElapsedBirths {
            children: &self.children,
            seek_meals: self.preferences.0,
            work_preference: self.preferences.1,
        }
    }
}

impl CoreHost {
    pub(super) fn next_natural_death(&self) -> Option<u128> {
        self.family
            .as_ref()?
            .elapsed_lifetimes
            .as_ref()?
            .next_after(self.core.simulated_millis())
    }

    pub(super) fn next_delivery(&self) -> Option<u128> {
        let &(deadline, _) = self.family.as_ref()?.delivery_deadlines().first()?;
        Some(deadline.saturating_sub(self.core.simulated_millis()))
    }

    pub(super) fn next_conception(&self) -> Option<u128> {
        let period = u128::from(
            self.family
                .as_ref()?
                .elapsed_lifetimes
                .as_ref()?
                .conception_period_millis?,
        );
        Some(period - self.core.simulated_millis() % period)
    }

    fn birth_preferences(&self, owner: &game_economy_family::Families) -> (bool, Option<bool>) {
        let birth_choices = owner
            .rules
            .enroll_births
            .then(|| self.schedule.as_ref()?.birth_choices.as_ref())
            .flatten();
        let preference = |domain| {
            birth_choices?
                .preferences
                .iter()
                .find(|row| row.domain == domain)
        };
        (
            self.dependent_care.is_some() || preference(personal_choice::MEAL).is_some(),
            preference(personal_choice::WORK).map(|row| row.enabled),
        )
    }

    pub(super) fn stage_elapsed_family(
        &self,
        people: &[Person],
        now: u128,
        staged_purpose: Option<&CorePurpose>,
        staged_health: Option<&CoreHealth>,
        delivery_due: bool,
        conception_due: bool,
    ) -> Result<Option<ElapsedFamilyFrame>, String> {
        let current = self.family.as_ref().expect("family deadline has an owner");
        let tick = self.core.physical.tick();
        let mut conception = if conception_due {
            let feelings = mate_feelings(self.opinions.as_ref());
            game_economy_family::choose_conceptions_elapsed(
                current,
                people,
                self.core.seed,
                tick,
                now,
                &feelings,
                Some(&|person| self.local_partition(person)),
            )?
        } else {
            None
        };
        let staged = if delivery_due {
            let base = conception.as_ref().map_or(current, |draft| &draft.families);
            let mut delivery =
                game_economy_family::deliver_elapsed(base, people, self.core.seed, tick, now)?;
            if let Some(conception) = conception.take() {
                let mut events = conception.events;
                events.append(&mut delivery.events);
                delivery.events = events;
            }
            Some(delivery)
        } else {
            conception
        };
        let Some(game_economy_family::ElapsedFamilyDraft {
            families,
            births,
            events,
        }) = staged
        else {
            return Ok(None);
        };
        let newborns = births.par_iter().map(|birth| birth.id).collect();
        let maternal = births
            .par_iter()
            .map(|birth| (birth.id, birth.mother))
            .collect();
        let frame = FamilyFrame {
            owner: families,
            newborns,
            births: maternal,
        };
        let personality = self.stage_personality_births(Some(&frame));
        let skills = self.stage_skill_births(None, Some(&frame), tick)?;
        let mut purpose = staged_purpose
            .or(self.purpose.as_ref())
            .filter(|_| !frame.newborns.is_empty())
            .cloned();
        crate::purpose::enroll(
            purpose.as_mut().map(|owner| &mut owner.owner),
            Some(&frame.owner),
            game_economy_family::BirthBatch::for_people(tick, &frame.newborns),
        )?;
        let health =
            self.stage_birth_health_at(Some(&frame), tick, Some((tick, now)), staged_health);
        Ok(Some(ElapsedFamilyFrame {
            frame,
            children: births,
            events,
            preferences: self.birth_preferences(current),
            personality,
            skills,
            purpose,
            health,
        }))
    }

    pub(super) fn publish_elapsed_family(
        &mut self,
        staged: ElapsedFamilyFrame,
    ) -> Vec<game_economy_family::FamilyEvent> {
        let FamilyFrame {
            owner,
            newborns,
            births,
        } = staged.frame;
        self.enroll_confined_births(&births);
        if let Some(care) = &mut self.dependent_care {
            game_economy_work::care::inherit(care, &owner, &newborns);
        }
        self.family = Some(owner);
        if let Some(personality) = staged.personality {
            self.personality = Some(personality);
        }
        if let Some(skills) = staged.skills {
            self.skills = Some(skills);
        }
        if let Some(health) = staged.health {
            self.health = Some(health);
        }
        if let Some(purpose) = staged.purpose {
            self.purpose = Some(purpose);
        }
        if self.fatigue.is_some() {
            self.activity.par_extend(newborns.par_iter().map(|&id| {
                game_economy_fatigue::TimedActivity {
                    id,
                    ..Default::default()
                }
            }));
            if self
                .activity
                .par_windows(2)
                .any(|pair| pair[0].id > pair[1].id)
            {
                self.activity.par_sort_unstable_by_key(|row| row.id);
            }
        }
        self.inherit_birth_preferences(&newborns, self.core.physical.tick());
        staged.events
    }

    /// Enroll only configured personal preferences; relationships and claims are not inherited.
    pub(super) fn inherit_birth_preferences(&mut self, newborns: &[u64], tick: u64) {
        let Some(family) = self
            .family
            .as_ref()
            .filter(|family| family.rules.enroll_births && !newborns.is_empty())
        else {
            return;
        };
        if let Some(opinions) = &mut self.opinions {
            opinions.enroll_newborn_development(newborns);
        }
        let needs_members = self.schedule.as_ref().is_some_and(|schedule| {
            schedule.birth_choices.is_some() && schedule.personal_choices.is_some()
        }) || self
            .social
            .as_ref()
            .and_then(|social| social.signals.as_ref())
            .is_some_and(|signals| signals.birth_profile.is_some());
        let members = needs_members.then(|| {
            game_family_index::MemberIndex::new(&family.members).expect("validated family")
        });
        if let Some(schedule) = &mut self.schedule
            && let Some(profile) = &schedule.birth_choices
            && let Some(people) = &mut schedule.personal_choices
        {
            let members = members.as_ref().expect("newborn choice index");
            let births: Vec<_> = newborns
                .par_iter()
                .filter_map(|person| {
                    if people.binary_search_by_key(person, |row| row.id).is_ok() {
                        return None;
                    }
                    let member = members.get(person)?;
                    Some(game_personal_choice::Newborn {
                        id: *person,
                        genes: std::array::from_fn(|locus| member.genome.expressed(locus)),
                    })
                })
                .collect();
            if !births.is_empty() {
                let inherited = profile.enroll(&births);
                if std::sync::Arc::get_mut(people).is_none() {
                    *people = std::sync::Arc::new(people.par_iter().cloned().collect());
                }
                let people = std::sync::Arc::get_mut(people).expect("exclusive choice generation");
                people.par_extend(inherited);
                if people.par_windows(2).any(|rows| rows[0].id > rows[1].id) {
                    people.par_sort_unstable_by_key(|row| row.id);
                }
            }
        }
        if let Some(signals) = self
            .social
            .as_mut()
            .and_then(|social| social.signals.as_mut())
            && signals.birth_profile.is_some()
        {
            let members = members.as_ref().expect("newborn signal index");
            let births: Vec<_> = newborns
                .par_iter()
                .map(|person| {
                    let member = members.get(person).expect("known newborn");
                    game_signals::Newborn {
                        person: *person,
                        mother: *member.parents.first().expect("newborn has a mother"),
                        genes: std::array::from_fn(|locus| member.genome.expressed(locus)),
                    }
                })
                .collect();
            signals
                .enroll_births(&births)
                .expect("validated signal births");
        }
        if let Some(social) = self
            .social
            .as_mut()
            .filter(|social| social.owner.inherit_births)
        {
            let mut parents: Vec<_> = social
                .owner
                .participants
                .par_iter()
                .map(|row| (row.person, row.willing))
                .collect();
            parents.par_sort_unstable_by_key(|row| row.0);
            let choices: Vec<_> = newborns
                .par_iter()
                .filter_map(|person| {
                    let member = &family.members[family
                        .members
                        .binary_search_by_key(person, |member| member.id)
                        .ok()?];
                    let willing = member.parents.iter().find_map(|parent| {
                        parents
                            .binary_search_by_key(parent, |row| row.0)
                            .ok()
                            .map(|index| parents[index].1)
                    })?;
                    Some((*person, willing))
                })
                .collect();
            social.owner.enroll_inherited(&choices);
        }
        let batch = game_economy_family::BirthBatch::for_people(tick, newborns);
        crate::animal_restraint_inheritance::enroll(
            self.animal_restraints.as_mut(),
            Some(family),
            batch,
        );
        crate::giving_inheritance::enroll(self.giving.as_mut(), Some(family), batch);
        let hunting = crate::hunting::enroll(self.hunting.as_ref(), Some(family), batch);
        let animal_care =
            crate::animal_care::enroll(self.animal_care.as_ref(), Some(family), batch);
        let handling =
            crate::animal_handling::enroll(self.animal_handling.as_ref(), Some(family), batch);
        if !hunting.is_empty() {
            self.hunting
                .as_mut()
                .expect("hunting owner")
                .policies
                .par_extend(hunting);
        }
        if !animal_care.is_empty() {
            self.animal_care
                .as_mut()
                .expect("animal care owner")
                .policies
                .par_extend(animal_care);
        }
        if !handling.is_empty() {
            self.animal_handling
                .as_mut()
                .expect("handling owner")
                .policies
                .par_extend(handling);
        }
        crate::gathering::enroll(self.gathering.as_mut(), Some(family), batch);
        game_economy_agriculture::enroll(self.agriculture.as_mut(), Some(family), batch);
        game_economy_claims::enroll(self.claims.as_mut(), Some(family), batch);
        game_economy_knowledge::inherit(&mut self.knowledge, family, newborns);
        crate::food_search::enroll(
            self.food_search.as_mut().map(|owner| &mut owner.search),
            Some(family),
            batch,
        );
    }

    pub(super) fn stage_family(
        &mut self,
        draft: &mut CoreSettlementDraft,
    ) -> Result<Option<FamilyFrame>, String> {
        let Some(owner) = &self.family else {
            return Ok(None);
        };
        let tick = draft.events().tick;
        let feelings = mate_feelings(
            self.opinions
                .as_ref()
                .filter(|_| owner.rules.subjective_mating),
        );
        let (personal_meals, personal_work) = self.birth_preferences(owner);
        let next = game_economy_family::advance_with_clock(
            owner,
            &mut draft.people.people,
            &mut draft.travel,
            self.core.seed,
            tick,
            personal_meals,
            Some(&|person| self.local_partition(person)),
            Some(&feelings),
            Some(self.core.simulated_millis()),
        )?;
        if let Some(available) = personal_work
            && !next.newborns.is_empty()
        {
            let newborns: std::collections::HashSet<_> =
                next.newborns.par_iter().copied().collect();
            draft
                .people
                .people
                .par_iter_mut()
                .filter(|person| newborns.contains(&person.id))
                .for_each(|person| person.willing_to_work = available);
        }
        let newborns: Vec<_> = next
            .newborns
            .par_iter()
            .map(|&id| (id, self.core.rules.carrying_capacity))
            .collect();
        let working: std::collections::HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        let deceased: Vec<_> = draft
            .people
            .people
            .par_iter()
            .filter(|person| {
                person.health == 0
                    && (working.contains(&person.id)
                        || self
                            .core
                            .people
                            .binary_search_by_key(&person.id, |old| old.id)
                            .ok()
                            .is_some_and(|index| self.core.people[index].health > 0))
            })
            .map(|person| person.id)
            .collect();
        if !newborns.is_empty() || !deceased.is_empty() {
            let (eligible, until) =
                self.succession_death_suspensions(&deceased, draft.events().tick)?;
            self.core.physical.stage_interval_people_with_suspension(
                &mut draft.physical,
                &newborns,
                &deceased,
                &eligible,
                until,
            )?;
        }
        let births = if self.confinement().is_some()
            || self.health.as_ref().is_some_and(|health| {
                !health.owner.cases.is_empty()
                    && health
                        .owner
                        .strains
                        .iter()
                        .any(|strain| strain.birth_transmission_per_million > 0)
            }) {
            next.births
                .par_iter()
                .map(|birth| (birth.id, birth.mother))
                .collect()
        } else {
            Vec::new()
        };
        Ok(Some(FamilyFrame {
            owner: next.family,
            newborns: next.newborns,
            births,
        }))
    }

    pub(super) fn stage_birth_health(
        &self,
        family: Option<&FamilyFrame>,
        tick: u64,
    ) -> Option<CoreHealth> {
        self.stage_birth_health_at(family, tick, None, None)
    }

    fn stage_birth_health_at(
        &self,
        family: Option<&FamilyFrame>,
        tick: u64,
        elapsed: Option<(u64, u128)>,
        staged: Option<&CoreHealth>,
    ) -> Option<CoreHealth> {
        let family = family.filter(|family| !family.births.is_empty())?;
        let health = staged.or(self.health.as_ref()).filter(|health| {
            !health.owner.cases.is_empty()
                && health
                    .owner
                    .strains
                    .iter()
                    .any(|strain| strain.birth_transmission_per_million > 0)
        })?;
        let mut next = health.clone();
        if health.owner.time_unit_millis.is_some() {
            let at_millis = elapsed.map_or_else(|| self.core.simulated_millis(), |(_, at)| at);
            next.owner.admit_elapsed_births(
                &health.owner,
                &family.births,
                self.core.seed,
                tick,
                at_millis,
            );
        } else if let Some((now, _)) = elapsed {
            next.owner
                .admit_births_at(&health.owner, &family.births, self.core.seed, now, tick);
        } else {
            next.owner
                .admit_births_validated(&health.owner, &family.births, self.core.seed, tick);
        }
        (next.owner.cases.len() != health.owner.cases.len()).then_some(next)
    }
}

fn mate_feelings(
    opinions: Option<&game_opinions::Opinions>,
) -> impl Fn(u64, u64) -> game_economy_family::MateFeelings + Sync + '_ {
    let opinions = opinions.map(game_opinions::Opinions::index);
    move |observer, target| {
        opinions
            .as_ref()
            .map_or_else(game_economy_family::MateFeelings::default, |index| {
                let (trust, affinity, fear, obligation) = index.subjective(observer, target);
                game_economy_family::MateFeelings {
                    trust,
                    affinity,
                    fear,
                    obligation,
                }
            })
    }
}
