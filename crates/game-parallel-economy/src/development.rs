//! Individual, configurable development from actual care and unattended periods.
use game_parallel_economy_family::Families;
use game_parallel_economy_work::care::DependentCare;
use game_parallel_opinions::Opinions;
use rayon::prelude::*;

/// Configure only actual newborns, with no inherited relationship records or feelings.
pub fn enroll(opinions: &mut Option<Opinions>, family: Option<&Families>, tick: u64) {
    let Some((opinions, family)) = opinions.as_mut().zip(family).filter(|(opinions, family)| {
        family.rules.enroll_births && opinions.rules.newborn_obligation_development.is_some()
    }) else {
        return;
    };
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.born == Some(tick))
        .map(|m| m.id)
        .collect();
    opinions.enroll_newborn_development(&newborns);
}

pub fn advance(
    opinions: &mut Option<Opinions>,
    care: Option<&DependentCare>,
    family: Option<&Families>,
    tick: u64,
) {
    let Some((opinions, care, family)) = opinions
        .as_mut()
        .zip(care)
        .zip(family)
        .map(|((opinions, care), family)| (opinions, care, family))
    else {
        return;
    };
    if care.events.is_empty() || !opinions.tracks_obligation_development() {
        return;
    }
    let mut ages: Vec<_> = family.members.par_iter().map(|m| (m.id, m.age)).collect();
    if ages.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
        ages.par_sort_unstable();
    }
    let dense = ages.first().zip(ages.last()).and_then(|(first, last)| {
        (last.0.checked_sub(first.0) == Some((ages.len() - 1) as u64)).then_some(first.0)
    });
    let experiences: Vec<_> = care
        .events
        .par_iter()
        .filter(|event| event.tick == tick)
        .filter_map(|event| {
            let index = dense
                .and_then(|first| event.child.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < ages.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| ages.binary_search_by_key(&event.child, |row| row.0).ok())
                        .flatten()
                })?;
            let age = ages[index].1;
            Some((event.child, age, event.caregiver.is_some()))
        })
        .collect();
    opinions.develop_obligation(&experiences);
}
