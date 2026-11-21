//! Personal meetings, subjective relationships, and communicated observations.
use super::*;

fn person(people: &[game_economy_person::Person], id: u64) -> Option<&game_economy_person::Person> {
    people
        .first()
        .and_then(|first| id.checked_sub(first.id))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| people.get(index).is_some_and(|person| person.id == id))
        .or_else(|| people.binary_search_by_key(&id, |person| person.id).ok())
        .map(|index| &people[index])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSocial {
    pub owner: game_social::Social,
    #[serde(default)]
    pub last_tick: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<game_language::Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signals: Option<game_signals::Signals>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreHealth {
    pub owner: game_disease::Disease,
    #[serde(default)]
    pub treatment: Vec<game_economy_treatment::Policy>,
    #[serde(default)]
    pub last_tick: Option<u64>,
}
impl Clone for CoreHealth {
    fn clone(&self) -> Self {
        Self {
            owner: game_disease::Disease {
                time_unit_millis: self.owner.time_unit_millis,
                strains: self.owner.strains.par_iter().cloned().collect(),
                treatments: self.owner.treatments.par_iter().cloned().collect(),
                cases: self.owner.cases.par_iter().cloned().collect(),
                events: self.owner.events.par_iter().cloned().collect(),
            },
            treatment: self.treatment.par_iter().cloned().collect(),
            last_tick: self.last_tick,
        }
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreDynamicsResults {
    pub meetings: Vec<game_social::Encounter>,
    pub disease_events: Vec<game_disease::Event>,
    pub written_explanations: Vec<game_language::writing::Explanation>,
}

impl CoreHost {
    /// A person chooses a visible form for a sound they personally know.
    /// This does not create a page or teach anyone else the association.
    pub fn choose_written_form(
        &mut self,
        person: u64,
        association: game_language::writing::Association,
    ) -> Result<(), String> {
        self.choose_written_forms(&[(person, association)])
    }

    /// Commit disjoint people’s independent written-form choices together.
    pub fn choose_written_forms(
        &mut self,
        choices: &[(u64, game_language::writing::Association)],
    ) -> Result<(), String> {
        if choices.par_windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("invalid written-form choices".into());
        }
        if choices.is_empty() {
            return Ok(());
        }
        let language = self
            .social
            .as_ref()
            .and_then(|social| social.language.as_ref())
            .ok_or("personal language is disabled")?;
        if choices.par_iter().any(|(person, association)| {
            self.core
                .people
                .binary_search_by_key(person, |row| row.id)
                .ok()
                .is_none_or(|index| self.core.people[index].health == 0)
                || language
                    .minds
                    .binary_search_by_key(person, |mind| mind.person)
                    .ok()
                    .is_none_or(|index| {
                        !language.minds[index]
                            .words
                            .iter()
                            .any(|word| word.sounds == association.sounds)
                    })
        }) {
            return Err("person cannot choose that spoken form".into());
        }
        let language = self
            .social
            .as_mut()
            .and_then(|social| social.language.as_mut())
            .expect("language checked");
        std::sync::Arc::make_mut(
            language
                .writing
                .as_mut()
                .ok_or("personal writing forms are disabled")?,
        )
        .assign_batch(choices, language.rules.sounds.len())
        .map_err(str::to_owned)
    }

    /// Choose an association supported by each person's own retained observation.
    pub fn choose_remembered_forms(
        &mut self,
        choices: &[(u64, u64, game_language::writing::Association)],
    ) -> Result<(), String> {
        if choices.par_windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("invalid remembered-form choices".into());
        }
        if choices.is_empty() {
            return Ok(());
        }
        let language = self
            .social
            .as_ref()
            .and_then(|social| social.language.as_ref())
            .ok_or("personal language is disabled")?;
        let forms = language
            .writing
            .as_ref()
            .ok_or("personal writing forms are disabled")?;
        if choices.par_iter().any(|(person, page, form)| {
            self.core
                .people
                .binary_search_by_key(person, |row| row.id)
                .ok()
                .is_none_or(|index| self.core.people[index].health == 0)
                || forms
                    .observations
                    .binary_search_by_key(&(*person, *page), |row| (row.person, row.page))
                    .ok()
                    .is_none_or(|index| {
                        let observation = &forms.observations[index];
                        observation.heard.as_ref() != Some(&form.sounds)
                            || observation.marks != form.marks
                    })
        }) {
            return Err("person lacks that remembered observation".into());
        }
        let assignments: Vec<_> = choices
            .par_iter()
            .map(|(person, _, form)| (*person, form.clone()))
            .collect();
        let language = self
            .social
            .as_mut()
            .and_then(|social| social.language.as_mut())
            .expect("language checked");
        std::sync::Arc::make_mut(language.writing.as_mut().expect("writing checked"))
            .assign_batch(&assignments, language.rules.sounds.len())
            .map_err(str::to_owned)
    }

    /// Run at most one personal social generation per physical tick.
    pub fn meet_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<game_social::Encounter>, String> {
        Ok(self.advance_dynamics_autonomously(occupied)?.meetings)
    }
    /// Advance social and disease owners once per tick, sharing actual meetings.
    pub fn advance_dynamics_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<CoreDynamicsResults, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let tick = self.core.physical.tick();
        let social = self
            .social
            .as_ref()
            .filter(|owner| owner.last_tick != Some(tick));
        let health = self.health.as_ref().filter(|owner| {
            owner.last_tick != Some(tick) && owner.owner.time_unit_millis.is_none()
        });
        if tick == 0 || (social.is_none() && health.is_none()) {
            return Ok(CoreDynamicsResults::default());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before social choices".into());
        }
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let opening = game_economy_dynamics::Dynamics {
            social: social.map(|owner| owner.owner.clone()),
            personality: None,
            disease: health.map(|owner| owner.clone().owner),
            treatment: vec![],
            claims: None,
        };
        let mut people: Vec<_> = self.core.people.par_iter().cloned().collect();
        let inquiry_meetings = self
            .taking
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.inquiry_meetings(tick));
        let partition = |person| self.local_partition(person);
        let contact = |a, b| self.same_local_endpoint(a, b);
        let walking = self.walking().is_some() || self.confinement().is_some();
        let mut next = opening.prepare(
            &self.core.people,
            &mut people,
            &busy,
            self.opinions.as_ref(),
            (&[], None, &[]),
            game_economy_dynamics::Clock {
                seed: self.core.seed,
                tick,
                travel: None,
                partition: walking.then_some(&partition),
                local: None,
                contact: walking.then_some(&contact),
                speak: walking.then_some(&contact),
                opening: None,
                missions: None,
                inquiry_meetings: &inquiry_meetings,
                personality_override: self.personality.as_ref(),
            },
        )?;
        if let (Some(previous), Some(disease)) = (health, &mut next.disease) {
            disease.events.par_extend(
                previous
                    .owner
                    .events
                    .par_iter()
                    .filter(|event| {
                        event.tick == tick && event.kind == game_disease::EventKind::Treated
                    })
                    .cloned(),
            );
            disease.events.par_sort_unstable_by_key(|event| {
                (
                    event.person,
                    event.strain,
                    match event.kind {
                        game_disease::EventKind::Recovered => 0,
                        game_disease::EventKind::Infected => 1,
                        game_disease::EventKind::Treated => 2,
                    },
                    event.source,
                )
            });
        }
        let meetings: Vec<_> = next
            .social
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.events.par_iter().cloned().collect());
        let (elapsed_health, elapsed_disease_events) = self
            .health
            .as_ref()
            .filter(|health| {
                health.owner.time_unit_millis.is_some()
                    && !meetings.is_empty()
                    && !health.owner.cases.is_empty()
                    && health
                        .owner
                        .strains
                        .iter()
                        .any(|strain| strain.transmission_per_million > 0)
            })
            .map(|health| self.stage_encounter_health(health, None, &meetings, tick))
            .transpose()?
            .map_or_else(
                || (None, Vec::new()),
                |(staged, events)| (Some(staged), events),
            );
        let mechanical_claims = social
            .and_then(|owner| owner.language.as_ref())
            .and_then(|_| self.mechanical_claim_offers(&meetings));
        let mut language = social
            .and_then(|social| social.language.as_ref())
            .map(|language| {
                if language
                    .minds
                    .par_iter()
                    .all(|mind| mind.topic_policy.is_none())
                {
                    let rows: Vec<_> = meetings
                        .par_iter()
                        .map(|meeting| game_language::Meeting {
                            a: meeting.a,
                            b: meeting.b,
                            meaning: game_language::Meaning::Place(meeting.region),
                        })
                        .collect();
                    return language
                        .prepare(&rows, self.core.seed, tick)
                        .map_err(str::to_owned);
                }
                // Topics use a person's own observations and interpretations. An
                // interpretation can differ from the signaler's private intention.
                let signal_topics = language
                    .minds
                    .par_iter()
                    .any(|mind| {
                        mind.topic_policy.as_ref().is_some_and(|policy| {
                            policy.preferences.iter().any(|row| {
                                row.enabled
                                    && matches!(
                                        row.meaning,
                                        game_language::Meaning::Signal(_)
                                            | game_language::Meaning::Need(_)
                                    )
                            })
                        })
                    })
                    .then(|| {
                        let mut rows = Vec::new();
                        if let Some(signals) = social.and_then(|owner| owner.signals.as_ref()) {
                            rows.par_extend(
                                signals
                                    .events
                                    .par_iter()
                                    .filter(|event| {
                                        event.noticed && event.tick.checked_add(1) == Some(tick)
                                    })
                                    .flat_map_iter(|event| {
                                        std::iter::once((
                                            event.observer,
                                            event.actor,
                                            game_language::Meaning::Signal(event.form),
                                        ))
                                        .chain(
                                            event.interpreted.map(|need| {
                                                (
                                                    event.observer,
                                                    event.actor,
                                                    game_language::Meaning::Need(need),
                                                )
                                            }),
                                        )
                                    }),
                            );
                            rows.par_extend(signals.minds.par_iter().flat_map_iter(|mind| {
                                mind.observations.iter().flat_map(move |event| {
                                    std::iter::once((
                                        mind.person,
                                        event.actor,
                                        game_language::Meaning::Signal(event.form),
                                    ))
                                    .chain(
                                        event.interpreted.map(|need| {
                                            (
                                                mind.person,
                                                event.actor,
                                                game_language::Meaning::Need(need),
                                            )
                                        }),
                                    )
                                })
                            }));
                            rows.par_sort_unstable();
                            rows.dedup();
                        }
                        rows
                    });
                let mechanical_topics = language
                    .minds
                    .par_iter()
                    .any(|mind| {
                        mind.topic_policy.as_ref().is_some_and(|policy| {
                            mind.mechanical_interest.is_some()
                                || policy.preferences.iter().any(|row| {
                                    row.enabled
                                        && matches!(
                                            row.meaning,
                                            game_language::Meaning::MechanicalResult { .. }
                                        )
                                })
                        })
                    })
                    .then(|| self.joint_mechanical_topics())
                    .flatten();
                let topics = |person: u64, other: u64, region: u64| {
                    let mut topics = vec![game_language::Meaning::Place(region)];
                    let mind = language
                        .minds
                        .binary_search_by_key(&person, |mind| mind.person)
                        .ok()
                        .map(|index| &language.minds[index]);
                    let policy = mind.and_then(|mind| mind.topic_policy.as_ref());
                    if let Some(policy) = policy
                        && policy.preferences.iter().any(|row| {
                            row.enabled && matches!(row.meaning, game_language::Meaning::Object(_))
                        })
                        && let Some(holder) =
                            self.core.physical.custody(game_lots::Place::Person(person))
                    {
                        for preference in &policy.preferences {
                            let game_language::Meaning::Object(good) = preference.meaning else {
                                continue;
                            };
                            if !preference.enabled {
                                continue;
                            }
                            let start = holder.lots.partition_point(|lot| lot.good < good);
                            if holder.lots[start..]
                                .iter()
                                .take_while(|lot| lot.good == good)
                                .any(|lot| {
                                    lot.expires_at.is_none_or(|expires| {
                                        expires > self.core.physical.lot_instant(tick)
                                    })
                                })
                            {
                                topics.push(preference.meaning);
                            }
                        }
                    }
                    if let Some(policy) = policy {
                        topics.extend(policy.preferences.iter().filter_map(|preference| {
                            match preference.meaning {
                                game_language::Meaning::Person(target)
                                    if preference.enabled
                                        && (target == person || target == other) =>
                                {
                                    Some(preference.meaning)
                                }
                                game_language::Meaning::Signal(_)
                                | game_language::Meaning::Need(_)
                                    if preference.enabled
                                        && signal_topics.as_ref().is_some_and(|rows| {
                                            rows.binary_search(&(person, other, preference.meaning))
                                                .is_ok()
                                        }) =>
                                {
                                    Some(preference.meaning)
                                }
                                game_language::Meaning::MechanicalResult { .. }
                                    if preference.enabled
                                        && mechanical_topics.as_ref().is_some_and(|rows| {
                                            rows.binary_search(&(
                                                person.min(other),
                                                person.max(other),
                                                preference.meaning,
                                            ))
                                            .is_ok()
                                        }) =>
                                {
                                    Some(preference.meaning)
                                }
                                _ => None,
                            }
                        }));
                    }
                    if mind.is_some_and(|mind| {
                        mind.mechanical_interest.is_some() && mind.topic_policy.is_some()
                    }) && let Some(rows) = &mechanical_topics
                    {
                        let pair = (person.min(other), person.max(other));
                        let begin = rows.partition_point(|row| (row.0, row.1) < pair);
                        topics.extend(
                            rows[begin..]
                                .iter()
                                .take_while(|row| (row.0, row.1) == pair)
                                .map(|row| row.2),
                        );
                    }
                    topics.sort_unstable();
                    topics.dedup();
                    topics
                };
                let rows: Vec<_> = meetings
                    .par_iter()
                    .map(|meeting| game_language::GroundedMeeting {
                        a: meeting.a,
                        b: meeting.b,
                        a_topics: topics(meeting.a, meeting.b, meeting.region),
                        b_topics: topics(meeting.b, meeting.a, meeting.region),
                    })
                    .collect();
                let opinions = language
                    .minds
                    .par_iter()
                    .any(|mind| mind.naming.is_some())
                    .then(|| self.opinions.as_ref().map(|owner| owner.index()))
                    .flatten();
                let affinity = |observer, target| {
                    opinions
                        .as_ref()
                        .map_or(0, |index| index.affinity(observer, target))
                };
                language
                    .prepare_mixed_with_affinity(
                        &rows,
                        mechanical_claims.as_deref(),
                        self.core.seed,
                        tick,
                        &affinity,
                    )
                    .map_err(str::to_owned)
            })
            .transpose()?;
        let written_explanations = match (
            social.and_then(|owner| owner.language.as_ref()),
            language.as_mut(),
        ) {
            (Some(opening), Some(next)) => {
                self.stage_written_explanations(opening, next, &meetings, tick)?
            }
            _ => Vec::new(),
        };
        let signals = social
            .and_then(|social| social.signals.as_ref())
            .map(|signals| {
                let contacts: Vec<_> = meetings
                    .par_iter()
                    .filter_map(|meeting| {
                        let a = person(&self.core.people, meeting.a)
                            .expect("known meeting participant");
                        let b = person(&self.core.people, meeting.b)
                            .expect("known meeting participant");
                        (!a.controlled && !b.controlled).then_some(game_signals::Contact {
                            a: meeting.a,
                            b: meeting.b,
                            a_hunger: a.hunger,
                            b_hunger: b.hunger,
                        })
                    })
                    .collect();
                let locations =
                    game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
                let directed: Vec<_> = signals
                    .directed_targets()
                    .into_par_iter()
                    .filter_map(|(actor, observer)| {
                        let a = person(&self.core.people, actor)?;
                        let b = person(&self.core.people, observer)?;
                        (!a.controlled
                            && a.health > 0
                            && b.health > 0
                            && locations
                                .region((a.id, a.region), (b.id, b.region))
                                .is_some())
                        .then_some(game_signals::Directed {
                            actor,
                            observer,
                            hunger: a.hunger,
                        })
                    })
                    .collect();
                signals
                    .prepare_with_directed(&contacts, &directed, self.core.seed, tick)
                    .map_err(str::to_owned)
            })
            .transpose()?;
        let mut disease_events: Vec<_> = next
            .disease
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.events.par_iter().cloned().collect());
        disease_events.par_extend(elapsed_disease_events);
        let mut opinions = next.social.as_ref().map(|_| self.opinions.clone());
        if let Some(opinions) = &mut opinions {
            game_economy_dynamics::learn_social(opinions, Some(&next), tick);
        }
        if let Some(signals) = &signals {
            let changes: Vec<_> = signals
                .feeling_changes()
                .into_par_iter()
                .map(|change| game_opinions::PersonalChange {
                    observer: change.observer,
                    target: change.target,
                    trust: change.reaction.trust,
                    affinity: change.reaction.affinity,
                    fear: change.reaction.fear,
                    obligation: change.reaction.obligation,
                })
                .collect();
            if !changes.is_empty() {
                let current = opinions.get_or_insert_with(|| self.opinions.clone());
                if let Some(opinions) = current {
                    opinions.apply_personal_changes(&changes, tick);
                }
            }
        }
        let knowledge = if let (Some(owner), Some(_)) = (&self.knowledge, &next.social) {
            let view = self.core.physical.view(tick)?;
            game_economy_knowledge::advance(
                Some(owner),
                &self.core.people,
                self.core.physical.goods(),
                &view,
                next.social.as_ref(),
                self.opinions.as_ref(),
                game_economy_knowledge::ObservationAccess {
                    now: tick,
                    not_at_stock: self
                        .block_away_from_stock(&busy)
                        .as_deref()
                        .unwrap_or(&busy),
                },
            )?
        } else {
            None
        };
        let taking = self
            .taking
            .as_ref()
            .filter(|owner| {
                (!owner.report_policies.is_empty() && !meetings.is_empty())
                    || !owner.complaints.is_empty()
            })
            .map(|owner| {
                let mut owner = owner.clone();
                let contacts: Vec<_> = meetings
                    .par_iter()
                    .map(|event| (event.a, event.b))
                    .collect();
                let changes = self.stage_taking_contacts(&mut owner, &contacts, tick);
                if !changes.is_empty() {
                    let current = opinions.get_or_insert_with(|| self.opinions.clone());
                    if let Some(opinions) = current {
                        opinions.apply_personal_changes(&changes, tick);
                    }
                }
                owner
            });
        let teaching = next
            .social
            .as_ref()
            .and_then(|social| self.stage_skill_teaching(social, tick));
        let recipe_learning = if let Some(knowledge) = self
            .skills
            .as_ref()
            .and_then(|skills| skills.owner.recipe_knowledge.as_ref())
        {
            let pairs: Vec<_> = meetings
                .par_iter()
                .filter(|event| {
                    [event.a, event.b].into_iter().all(|person| {
                        self.core
                            .people
                            .binary_search_by_key(&person, |row| row.id)
                            .ok()
                            .is_some_and(|index| {
                                !self.core.people[index].controlled
                                    && self.core.people[index].health > 0
                            })
                    })
                })
                .map(|event| (event.a, event.b))
                .collect();
            let pressured: std::collections::HashSet<_> = meetings
                .par_iter()
                .flat_map_iter(|event| {
                    event
                        .pressured_people()
                        .map(|person| (person, if person == event.a { event.b } else { event.a }))
                })
                .collect();
            let policies: std::collections::HashMap<_, _> = next
                .social
                .as_ref()
                .into_iter()
                .flat_map(|social| social.pressures.iter())
                .filter(|policy| policy.enabled)
                .map(|policy| (policy.person, policy))
                .collect();
            let opinions = self.opinions.as_ref().map(|owner| owner.index());
            let staged = knowledge
                .meet(&pairs, tick, |teacher, student| {
                    pressured.contains(&(teacher, student))
                        || policies.get(&teacher).is_some_and(|policy| {
                            opinions.as_ref().is_some_and(|opinions| {
                                let (fear, obligation) = opinions.feelings(teacher, student);
                                policy.responds(fear, obligation)
                            })
                        })
                })
                .map_err(str::to_owned)?;
            staged.or_else(|| {
                (!knowledge.events.is_empty()).then(|| {
                    let mut cleared = knowledge.parallel_copy();
                    cleared.events.clear();
                    cleared
                })
            })
        } else {
            None
        };
        let mechanical_testimony =
            language
                .as_ref()
                .zip(mechanical_claims.as_ref())
                .and_then(|(language, offers)| {
                    self.stage_mechanical_testimony(&language.events, &meetings, offers, tick)
                });
        let writing = self.writing.as_ref().and_then(|opening| {
            if opening.study.is_empty() || meetings.is_empty() {
                return None;
            }
            let mut owner = opening.clone();
            owner.advance_study(opening, &self.core.physical, &[], &meetings, tick);
            Some(owner)
        });
        if let Some(frame) = &teaching
            && self
                .opinions
                .as_ref()
                .is_some_and(|owner| owner.tracks_teaching_reactions())
        {
            let current = opinions.get_or_insert_with(|| self.opinions.clone());
            if let Some(opinions) = current {
                opinions.learn_teaching_pairs(&frame.lessons, tick);
            }
        }
        let route_reports = self
            .core
            .travel
            .as_ref()
            .map(|travel| {
                crate::route_sharing::prepare(
                    travel,
                    &self.core.people,
                    self.opinions.as_ref(),
                    &meetings,
                )
            })
            .unwrap_or_default();
        let mut travel = (!route_reports.is_empty())
            .then(|| self.core.travel.clone().expect("reports need travel"));
        if let Some(travel) = &mut travel {
            travel.receive_reports(&route_reports);
        }
        let changed = people
            .par_iter()
            .zip(self.core.people.par_iter())
            .any(|(a, b)| a != b);
        let generation = if changed {
            Some(
                self.core
                    .generation
                    .checked_add(1)
                    .ok_or("core revision overflow")?,
            )
        } else {
            None
        };
        if let Some(owner) = next.social {
            self.social = Some(CoreSocial {
                owner,
                last_tick: Some(tick),
                language,
                signals,
            });
        }
        if let Some(owner) = next.disease {
            let treatment = self
                .health
                .as_mut()
                .map(|health| std::mem::take(&mut health.treatment))
                .unwrap_or_default();
            self.health = Some(CoreHealth {
                owner,
                treatment,
                last_tick: Some(tick),
            });
        }
        if let Some(staged) = elapsed_health {
            self.health = Some(staged);
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        if let Some(taking) = taking {
            self.taking = Some(taking);
        }
        if let Some(frame) = teaching {
            if frame.rate_changed {
                self.skill_rates = Some(frame.skills.owner.index());
            }
            self.skills = Some(frame.skills);
        }
        if let Some(knowledge) = recipe_learning {
            self.skills
                .as_mut()
                .expect("recipe knowledge has a skill owner")
                .owner
                .recipe_knowledge = Some(knowledge);
        }
        if let Some(memories) = mechanical_testimony {
            self.publish_mechanical_testimony(memories);
        }
        if let Some(knowledge) = knowledge {
            self.knowledge = Some(knowledge);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        if let Some(travel) = travel {
            self.core.travel = Some(travel);
        }
        if let Some(generation) = generation {
            self.core.people = people;
            self.core.generation = generation;
            self.work_checked_revision = None;
        }
        Ok(CoreDynamicsResults {
            meetings,
            disease_events,
            written_explanations,
        })
    }
}
