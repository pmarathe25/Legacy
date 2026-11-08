//! Individual choices to share route beliefs during actual encounters.
use game_economy_person::{
    Person,
    travel::{RouteObservation, Travel},
};
use game_opinions::Opinions;
use game_social::Encounter;
use rayon::prelude::*;

pub fn prepare(
    travel: &Travel,
    people: &[Person],
    opinions: Option<&Opinions>,
    events: &[Encounter],
) -> Vec<RouteObservation> {
    if travel.sharing.is_empty() || travel.observations.is_empty() || events.is_empty() {
        return Vec::new();
    }
    let mut policies: Vec<_> = travel.sharing.par_iter().collect();
    policies.par_sort_unstable_by_key(|policy| policy.person);
    let mut reports: Vec<_> = travel.observations.par_iter().collect();
    reports.par_sort_unstable_by_key(|report| (report.person, report.source, report.destination));
    let needs_feelings = policies.par_iter().any(|policy| {
        !policy.willing && (policy.fear_at_least.is_some() || policy.obligation_at_least.is_some())
    });
    let feelings = opinions.filter(|_| needs_feelings).map(Opinions::index);
    let reception = opinions.and_then(Opinions::report_filter);
    let mut received: Vec<_> = events
        .par_iter()
        .flat_map_iter(|event| [(event.a, event.b), (event.b, event.a)])
        .filter_map(|(speaker, listener)| {
            let policy = policies
                .binary_search_by_key(&speaker, |policy| policy.person)
                .ok()
                .map(|i| policies[i])?;
            if policy.target.is_some_and(|target| target != listener)
                || people
                    .binary_search_by_key(&speaker, |person| person.id)
                    .ok()
                    .is_none_or(|i| people[i].controlled)
                || reception
                    .as_ref()
                    .is_some_and(|filter| !filter.accepts(listener, speaker))
            {
                return None;
            }
            let (fear, obligation) = feelings
                .as_ref()
                .map_or((0, 0), |index| index.feelings(speaker, listener));
            if !policy.willing
                && !policy
                    .fear_at_least
                    .is_some_and(|threshold| fear >= threshold)
                && !policy
                    .obligation_at_least
                    .is_some_and(|threshold| obligation >= threshold)
            {
                return None;
            }
            let key = (speaker, policy.source, policy.destination);
            let index = reports
                .binary_search_by_key(&key, |report| {
                    (report.person, report.source, report.destination)
                })
                .ok()?;
            let mut report = reports[index].clone();
            if policy
                .claimed_open
                .is_some_and(|value| value != report.open)
                || policy
                    .claimed_ticks
                    .is_some_and(|value| Some(value) != report.estimated_ticks)
            {
                report.witness = None;
            }
            if let Some(value) = policy.claimed_open {
                report.open = value;
            }
            if let Some(value) = policy.claimed_ticks {
                report.estimated_ticks = Some(value);
            }
            report.person = listener;
            report.learned_from = Some(speaker);
            Some(report)
        })
        .collect();
    // Equal-time competing statements resolve deterministically by source.
    received.par_sort_unstable_by_key(|report| {
        (
            report.person,
            report.source,
            report.destination,
            report.observed_at,
            report.learned_from,
        )
    });
    received
}
