//! Directed personal pressure produces disjoint local encounters without institutions.
use game_parallel_opinions::Opinions;
use game_parallel_personality::Personality;
use game_parallel_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub fn prepare(
    social: &Social,
    contacts: &[Contact],
    opinions: Option<&Opinions>,
    personality: Option<&Personality>,
    seed: u64,
    tick: u64,
    requested: Option<&Encounter>,
) -> Result<Social, &'static str> {
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
                    || willing(target) != Some(false)
                    || willing(requester) != Some(true)
                {
                    return None;
                }
                let a = contact(requester)?;
                let b = contact(target)?;
                let region = a.region?;
                if !eligible(a) || !eligible(b) || b.region != Some(region) {
                    return None;
                }
                Some((
                    requester,
                    target,
                    region,
                    game_parallel_genetics::random(seed ^ requester, target, tick),
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
                game_parallel_genetics::random(seed ^ row.1, row.0, tick),
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
                }
            })
            .collect();
        pressured
    } else {
        Vec::new()
    };
    if let Some(requested) = requested {
        pressured.push(requested.clone());
    }
    let mut already_occupied: Vec<_> = pressured.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
    already_occupied.par_sort_unstable();
    let friends = crate::friendship::Context {
        social,
        contacts,
        opinions,
        needs: &needs,
        occupied: &already_occupied,
        seed,
        tick,
    }
    .prepare(&voluntary);
    pressured.par_extend(friends);
    if pressured.is_empty() {
        let selection = social.select_validated(contacts, seed, tick, voluntary)?;
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
    selection.events.append(&mut pressured);
    selection.events.par_sort_unstable_by_key(|e| (e.a, e.b));
    selection.needs = social
        .needs
        .as_ref()
        .map(|needs| needs.advance(&social.participants, contacts, &selection.events));
    social.prepare_selection_validated(selection, tick)
}
