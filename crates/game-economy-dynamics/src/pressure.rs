//! Directed personal pressure produces disjoint local encounters without institutions.
use game_economy_person::Person;
use game_missions::Missions;
use game_opinions::Opinions;
use game_personality::Personality;
use game_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub struct Access<'a> {
    pub speak: Option<&'a (dyn Fn(u64, u64) -> bool + Sync)>,
    pub opening: Option<&'a crate::OpeningLocator<'a>>,
    pub missions: Option<&'a Missions>,
    pub inquiry_meetings: &'a [(u64, u64)],
    pub people: &'a [Person],
}

pub fn prepare(
    social: &Social,
    contacts: &[Contact],
    opinions: Option<&Opinions>,
    personality: Option<&Personality>,
    clock: (u64, u64),
    requested: Option<&Encounter>,
    access: Access<'_>,
) -> Result<Social, &'static str> {
    #[cfg(feature = "profile")]
    let selection_phase = game_diagnostics::span("dynamics.meetings.select");
    #[cfg(feature = "profile")]
    let mut stage = game_diagnostics::span("dynamics.select.directed");
    let (seed, tick) = clock;
    let can_speak = access.speak;
    let mut reserved: Vec<_> = requested
        .into_iter()
        .flat_map(|event| [event.a, event.b])
        .collect();
    reserved.sort_unstable();
    let directed = crate::mission_meetings::Context {
        missions: access.missions,
        inquiry_meetings: access.inquiry_meetings,
        people: access.people,
        contacts,
        social,
        opinions,
        occupied: &reserved,
        speak: can_speak,
        seed,
        tick,
    }
    .select();
    reserved.par_extend(
        directed
            .par_iter()
            .flat_map_iter(|event| [event.a, event.b]),
    );
    reserved.par_sort_unstable();
    let personality = personality.map(Personality::index);
    let needs = social
        .needs
        .as_ref()
        .map(|needs| needs.index())
        .unwrap_or_default();
    let voluntary = |id, isolation| {
        personality
            .as_ref()
            .is_none_or(|index| index.social(id, isolation, seed, tick))
    };
    let partner_preferences = opinions.and_then(Opinions::meeting_filter);
    let accepts = |person, partner| partner_preferences.as_ref().is_none_or(|view| view.accepts(person, partner));
    let acceptable = |a, b| accepts(a, b) && accepts(b, a);
    let filter_meetings = |selection: &mut game_social::Selection| {
        if partner_preferences.is_some() {
            selection.events = std::mem::take(&mut selection.events)
                .into_par_iter()
                .filter(|event| acceptable(event.a, event.b))
                .collect();
        }
    };
    #[cfg(feature = "profile")]
    {
        drop(stage);
        stage = game_diagnostics::span("dynamics.select.pressure");
    }
    let mut pressured = if let Some(opinions) = opinions.filter(|o| {
        !o.records.is_empty()
            && !social.pressures.is_empty()
            && tick.is_multiple_of(social.rules.interval)
    }) {
        let mut participants: Vec<_> = social.participants.par_iter().collect();
        participants.par_sort_unstable_by_key(|p| p.person);
        let mut policies: Vec<_> = social.pressures.par_iter().collect();
        policies.par_sort_unstable_by_key(|p| p.person);
        let contact = |id| {
            contacts
                .binary_search_by_key(&id, |c| c.person)
                .ok()
                .map(|i| contacts[i])
        };
        let willing = |id| {
            participants
                .binary_search_by_key(&id, |p| p.person)
                .ok()
                .map(|i| {
                    let isolation = needs
                        .binary_search_by_key(&id, |row| row.0)
                        .ok()
                        .map_or(0, |i| needs[i].1);
                    participants[i].willing && voluntary(id, isolation)
                })
        };
        let eligible = |c: Contact| {
            !c.busy
                && reserved.binary_search(&c.person).is_err()
                && c.health > 0
                && c.health >= social.rules.min_health
                && c.hunger <= social.rules.max_hunger
        };
        // Each willing requester proposes at most one target; each target accepts at most one proposal.
        let mut proposals: Vec<_> = opinions
            .records
            .par_iter()
            .filter_map(|opinion| {
                let target = opinion.observer;
                let requester = opinion.target;
                let policy = policies
                    .binary_search_by_key(&target, |p| p.person)
                    .ok()
                    .map(|i| policies[i])?;
                if !policy.responds(opinion.fear, opinion.obligation)
                    || willing(target).map(|willing| willing && accepts(target, requester)) != Some(false)
                    || willing(requester) != Some(true)
                    || !accepts(requester, target)
                {
                    return None;
                }
                let a = contact(requester)?;
                let b = contact(target)?;
                let region = a.region?;
                if !eligible(a)
                    || !eligible(b)
                    || (b.region != Some(region)
                        && !can_speak.is_some_and(|access| access(requester, target)))
                {
                    return None;
                }
                Some((
                    requester,
                    target,
                    region,
                    game_genetics::random(seed ^ requester, target, tick),
                ))
            })
            .collect();
        proposals.par_sort_unstable_by_key(|row| (row.0, row.3, row.1));
        let mut chosen: Vec<_> = proposals
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|rows| rows[0])
            .collect();
        chosen.par_sort_unstable_by_key(|row| {
            (
                row.1,
                game_genetics::random(seed ^ row.1, row.0, tick),
                row.0,
            )
        });
        let pressured: Vec<_> = chosen
            .par_chunk_by(|a, b| a.1 == b.1)
            .map(|rows| {
                let (requester, target, region, _) = rows[0];
                Encounter {
                    tick,
                    region,
                    a: requester.min(target),
                    b: requester.max(target),
                    pressured: Some(target),
                    also_pressured: None,
                }
            })
            .collect();
        if partner_preferences.is_some() {
            resolve_pressure_conflicts(pressured, seed, tick)
        } else {
            pressured
        }
    } else {
        Vec::new()
    };
    pressured.par_extend(directed);
    if let Some(requested) = requested {
        pressured.push(requested.clone());
    }
    let mut already_occupied: Vec<_> = pressured.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
    already_occupied.par_sort_unstable();
    #[cfg(feature = "profile")]
    {
        drop(stage);
        stage = game_diagnostics::span("dynamics.select.friendship");
    }
    let friends = crate::friendship::Context {
        social,
        contacts,
        opinions,
        needs: &needs,
        occupied: &already_occupied,
        seed,
        tick,
        can_speak,
    }
    .prepare(&voluntary, &acceptable);
    pressured.par_extend(friends);
    #[cfg(feature = "profile")]
    {
        drop(stage);
        stage = game_diagnostics::span("dynamics.select.openings");
    }
    if let Some(location) = access.opening {
        let mut occupied: Vec<_> = pressured.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
        occupied.par_sort_unstable();
        let conversations = crate::openings::Context {
            social,
            contacts,
            needs: &needs,
            occupied: &occupied,
            seed,
            tick,
            location,
        }
        .prepare(&voluntary);
        pressured.par_extend(conversations.into_par_iter().filter(|event| acceptable(event.a, event.b)));
    }
    #[cfg(feature = "profile")]
    {
        drop(stage);
        stage = game_diagnostics::span("dynamics.select.general");
    }
    if pressured.is_empty() {
        let mut selection = social.select_validated(contacts, seed, tick, voluntary)?;
        let original_count = selection.events.len();
        filter_meetings(&mut selection);
        if selection.events.len() != original_count {
            selection.needs = social.needs.as_ref()
                .map(|needs| needs.advance(&social.participants, contacts, &selection.events));
        }
        #[cfg(feature = "profile")]
        drop(stage);
        #[cfg(feature = "profile")]
        drop(selection_phase);
        #[cfg(feature = "profile")]
        let _history_phase = game_diagnostics::span("dynamics.meetings.history");
        return social.prepare_selection_validated(selection, tick);
    }
    let mut occupied: Vec<_> = pressured.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
    occupied.par_sort_unstable();
    let available: Vec<_> = contacts
        .par_iter()
        .map(|c| Contact {
            busy: c.busy || occupied.binary_search(&c.person).is_ok(),
            ..*c
        })
        .collect();
    let mut selection = social.select_validated(&available, seed, tick, voluntary)?;
    filter_meetings(&mut selection);
    selection.events.append(&mut pressured);
    selection.events.par_sort_unstable_by_key(|e| (e.a, e.b));
    selection.needs = social
        .needs
        .as_ref()
        .map(|needs| needs.advance(&social.participants, contacts, &selection.events));
    #[cfg(feature = "profile")]
    drop(stage);
    #[cfg(feature = "profile")]
    drop(selection_phase);
    #[cfg(feature = "profile")]
    let _history_phase = game_diagnostics::span("dynamics.meetings.history");
    social.prepare_selection_validated(selection, tick)
}

/// Directed refusals can put the same actor in both proposal roles.
fn resolve_pressure_conflicts(candidates: Vec<Encounter>, seed: u64, tick: u64) -> Vec<Encounter> {
    let mut choices: Vec<_> = candidates
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, event)| {
            let priority = game_genetics::random(seed ^ event.a, event.b, tick);
            [(event.a, priority, index), (event.b, priority, index)]
        })
        .collect();
    choices.par_sort_unstable();
    let winners: Vec<_> = choices
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|rows| (rows[0].0, rows[0].2))
        .collect();
    let selected = |person, index| winners
        .binary_search_by_key(&person, |row| row.0)
        .is_ok_and(|at| winners[at].1 == index);
    candidates.into_par_iter().enumerate().filter_map(|(index, row)| {
        (selected(row.a, index) && selected(row.b, index)).then_some(row)
    }).collect()
}

#[cfg(test)]
#[test]
fn a_person_can_take_only_one_part_when_pressured_in_both_roles() {
    let candidate = |a, b| Encounter {
        tick: 1, region: 1, a, b, pressured: Some(b), also_pressured: None,
    };
    for workers in [1, 6] {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let admitted = resolve_pressure_conflicts(vec![candidate(1, 2), candidate(2, 3)], 7, 1);
            assert_eq!(admitted.len(), 1);
        });
    }
}
