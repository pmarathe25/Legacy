//! Independent choices to explain visible marks during actual meetings.
use super::*;
use game_language::writing::{Association, Explanation};

impl CoreHost {
    pub(super) fn stage_written_explanations(
        &self,
        opening: &game_language::Language,
        next: &mut game_language::Language,
        meetings: &[game_social::Encounter],
        tick: u64,
    ) -> Result<Vec<Explanation>, String> {
        let Some(forms) = opening.writing.as_ref() else {
            return Ok(Vec::new());
        };
        if forms.sharing.is_empty() && forms.explanations.is_empty() {
            return Ok(Vec::new());
        }
        let pages = self.writing.as_ref();
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let pressured = |observer, target, fear: Option<u64>, obligation: Option<u64>| {
            opinions.as_ref().is_some_and(|opinions| {
                let (felt_fear, felt_obligation) = opinions.feelings(observer, target);
                fear.is_some_and(|threshold| felt_fear >= threshold)
                    || obligation.is_some_and(|threshold| felt_obligation >= threshold)
            })
        };
        let policy = |person| {
            forms
                .sharing
                .binary_search_by_key(&person, |row| row.person)
                .ok()
                .map(|index| &forms.sharing[index])
        };
        let mind = |person| {
            opening
                .minds
                .binary_search_by_key(&person, |row| row.person)
                .ok()
                .map(|index| &opening.minds[index])
        };
        let actor = |person| {
            self.core
                .people
                .binary_search_by_key(&person, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index])
        };
        let mut events: Vec<_> = meetings
            .par_iter()
            .flat_map_iter(|meeting| {
                [(meeting.a, meeting.b), (meeting.b, meeting.a)]
                    .into_iter()
                    .filter_map(|(speaker, listener)| {
                        let offer = policy(speaker).filter(|policy| {
                            policy.show
                                || pressured(
                                    speaker,
                                    listener,
                                    policy.show_fear_at_least,
                                    policy.show_obligation_at_least,
                                )
                        })?;
                        let speaking = mind(speaker).filter(|mind| mind.speak)?;
                        if actor(speaker)
                            .is_none_or(|person| person.controlled || person.health == 0)
                        {
                            return None;
                        }
                        let holder = self
                            .core
                            .physical
                            .custody(game_lots::Place::Person(speaker))?;
                        let (page, spoken) = holder
                            .lots
                            .iter()
                            .filter_map(|lot| {
                                let page = pages?.page(lot.item?)?;
                                let marks = page.marks.as_ref()?;
                                let sound = offer.claimed_sounds.clone().or_else(|| {
                                    forms.interpretations(speaker, marks).into_iter().next()
                                })?;
                                if sound.iter().any(|sound| {
                                    !speaking.phonemes.is_empty()
                                        && speaking.phonemes.binary_search(sound).is_err()
                                }) {
                                    return None;
                                }
                                Some((page, sound))
                            })
                            .min_by_key(|(page, _)| page.item)?;
                        let marks = page.marks.as_ref()?.clone();
                        let heard = mind(listener).is_some_and(|mind| mind.hear)
                            && actor(listener).is_some_and(|person| person.health > 0);
                        let adoption = policy(listener)
                            .filter(|policy| {
                                (policy.adopt
                                    || pressured(
                                        listener,
                                        speaker,
                                        policy.adopt_fear_at_least,
                                        policy.adopt_obligation_at_least,
                                    ))
                                    && heard
                                    && actor(listener).is_some_and(|person| !person.controlled)
                            })
                            .map(|policy| Association {
                                sounds: spoken.clone(),
                                marks: policy
                                    .adopted_marks
                                    .clone()
                                    .unwrap_or_else(|| marks.clone()),
                            });
                        let adoption = adoption.filter(|form| {
                            forms
                                .minds
                                .binary_search_by_key(&listener, |mind| mind.person)
                                .ok()
                                .is_none_or(|index| {
                                    let existing = &forms.minds[index].associations;
                                    existing.len() < forms.capacity
                                        || existing
                                            .binary_search_by(|row| row.sounds.cmp(&form.sounds))
                                            .is_ok()
                                })
                        });
                        Some(Explanation {
                            tick,
                            speaker,
                            listener,
                            page: page.item,
                            marks,
                            spoken,
                            heard,
                            adopted: adoption,
                        })
                    })
            })
            .collect();
        events.par_sort_unstable_by_key(|event| (event.speaker, event.listener, event.page));
        if events.is_empty() && forms.explanations.is_empty() {
            return Ok(events);
        }
        let mut adoption: Vec<_> = events
            .par_iter()
            .filter_map(|event| event.adopted.clone().map(|form| (event.listener, form)))
            .collect();
        adoption.par_sort_unstable_by_key(|row| row.0);
        let target = std::sync::Arc::make_mut(
            next.writing
                .as_mut()
                .expect("opening writing carried forward"),
        );
        if !adoption.is_empty() {
            target
                .assign_batch(&adoption, opening.rules.sounds.len())
                .map_err(str::to_owned)?;
        }
        target.explanations = events.par_iter().cloned().collect();
        Ok(events)
    }
}
