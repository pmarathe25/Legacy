//! Individual purpose follows actual outcomes and shares the physical lifecycle.
use crate::Families;
use game_parallel_economy_person::Person;
use game_parallel_goals::Completion;
use game_parallel_lot_runtime::{Events, Work};
use game_parallel_purpose::{Event, Outcome, Purpose};
use rayon::prelude::*;

pub fn prepare(
    owner: Option<&Purpose>,
    work: &[Work],
    events: &Events,
    blocked: &[Completion],
    people: &mut [Person],
    tick: u64,
) -> Option<Purpose> {
    let owner = owner?;
    let outcomes: Vec<_> = events
        .finished
        .par_iter()
        .filter_map(|id| {
            work.binary_search_by_key(id, |row| row.job.id)
                .ok()
                .map(|i| Event {
                    person: work[i].job.worker,
                    outcome: Outcome::Progress,
                })
        })
        .chain(blocked.par_iter().map(|row| Event {
            person: row.person,
            outcome: Outcome::Blocked,
        }))
        .collect();
    let next = owner.prepare(&outcomes, tick, |id| {
        people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .is_some_and(|i| people[i].health > 0)
    });
    let mut harmful: Vec<_> = next
        .people
        .par_iter()
        .filter(|p| tick >= p.eligible_at && p.deprivation >= next.rules.damage_threshold)
        .map(|p| p.person)
        .collect();
    if harmful.par_windows(2).any(|pair| pair[0] > pair[1]) {
        harmful.par_sort_unstable();
    }
    people.par_iter_mut().for_each(|p| {
        if p.health > 0 && harmful.binary_search(&p.id).is_ok() {
            p.health = p.health.saturating_sub(next.rules.damage);
        }
    });
    Some(next)
}

/// Actual newborns become eligible at biological adulthood, when configured for births.
pub fn enroll(
    owner: Option<&mut Purpose>,
    family: Option<&Families>,
    tick: u64,
) -> Result<(), String> {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(_, family)| family.rules.enroll_births)
    else {
        return Ok(());
    };
    let newborn_ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.born == Some(tick))
        .map(|m| m.id)
        .collect();
    if newborn_ids.is_empty() {
        return Ok(());
    }
    let eligible_at = tick
        .checked_add(family.rules.adult_age)
        .ok_or("purpose eligibility overflow")?;
    let newborns: Vec<_> = newborn_ids
        .into_par_iter()
        .map(|id| game_parallel_purpose::Person {
            person: id,
            deprivation: 0,
            eligible_at,
        })
        .collect();
    owner.people.par_extend(newborns);
    Ok(())
}
