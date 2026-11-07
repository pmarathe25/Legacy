//! Independent choices to open a physical cell for a particular person.
use super::{Confinement, Event, EventKind, ordered_index};
use game_economy_person::Person;
use game_opinions::Opinions;
use game_work_sites::Facilities;
use rayon::prelude::*;

pub struct Context<'a> {
    pub owner: &'a mut Confinement,
    pub people: &'a [Person],
    pub sites: &'a Facilities,
    pub busy: &'a [u64],
    /// Targets reserved by an explicit player action.
    pub reserved_targets: &'a [u64],
    /// Sorted remaining effort of guards who chose to resist outside openings.
    pub opening_guards: &'a [(u64, u128)],
    pub opinions: Option<&'a Opinions>,
    pub seed: u64,
    pub tick: u64,
}

#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    target: u64,
    site: u64,
    priority: u64,
    effort: u64,
}

/// Compute per-site exclusive prefixes using parallel blocks and a small block scan.
fn opening_results(mut selected: Vec<Choice>, guards: &[(u64, u128)]) -> Vec<(Choice, bool)> {
    if guards.is_empty() {
        return selected
            .into_par_iter()
            .map(|choice| (choice, true))
            .collect();
    }
    selected.par_sort_unstable_by_key(|c| (c.site, c.priority, c.actor, c.target));
    let mut rows: Vec<_> = selected
        .into_par_iter()
        .map(|choice| (choice, 0u128))
        .collect();
    const BLOCK: usize = 1024;
    rows.par_chunks_mut(BLOCK).for_each(|chunk| {
        let mut previous = None;
        let mut total = 0u128;
        for (choice, prefix) in chunk {
            if previous != Some(choice.site) {
                total = 0;
            }
            *prefix = total;
            total = total.saturating_add(u128::from(choice.effort));
            previous = Some(choice.site);
        }
    });
    let mut previous = None;
    let mut carry = 0u128;
    let offsets: Vec<_> = rows
        .chunks(BLOCK)
        .map(|chunk| {
            let first = chunk[0].0.site;
            let last = chunk[chunk.len() - 1];
            let offset = if previous == Some(first) { carry } else { 0 };
            carry = last.1.saturating_add(u128::from(last.0.effort));
            if first == last.0.site {
                carry = carry.saturating_add(offset);
            }
            previous = Some(last.0.site);
            offset
        })
        .collect();
    rows.par_chunks_mut(BLOCK)
        .zip(offsets.into_par_iter())
        .for_each(|(chunk, offset)| {
            let first = chunk[0].0.site;
            for (choice, prefix) in chunk {
                if choice.site != first {
                    break;
                }
                *prefix = prefix.saturating_add(offset);
            }
        });
    rows.into_par_iter()
        .map(|(choice, prefix)| {
            let opposition = guards
                .binary_search_by_key(&choice.site, |row| row.0)
                .ok()
                .map_or(0, |i| guards[i].1)
                .saturating_sub(prefix);
            (
                choice,
                opposition == 0 || u128::from(choice.effort) > opposition,
            )
        })
        .collect()
}

impl Context<'_> {
    /// Select one available actor per captive and remove only those actual holds.
    pub fn prepare(self) -> Vec<u64> {
        let need_opinions = self
            .owner
            .release_policies
            .par_iter()
            .any(|p| p.pressure_threshold.is_some());
        let opinions = need_opinions
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        let mut choices: Vec<_> = self
            .owner
            .release_policies
            .par_iter()
            .filter_map(|policy| {
                let actor = &self.people[ordered_index(self.people, policy.person, |p| p.id)?];
                let hold = &self.owner.holds
                    [ordered_index(&self.owner.holds, policy.target, |h| h.person)?];
                let site = &self.sites.sites[self
                    .sites
                    .sites
                    .binary_search_by_key(&hold.site, |s| s.id)
                    .ok()?];
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(subject, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor.id, subject);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (!actor.controlled
                    && actor.health > 0
                    && actor.region == Some(site.region)
                    && !self.owner.held(actor.id)
                    && self.busy.binary_search(&actor.id).is_err()
                    && self.reserved_targets.binary_search(&policy.target).is_err()
                    && (policy.willing || pressured))
                    .then_some(Choice {
                        actor: actor.id,
                        target: policy.target,
                        site: hold.site,
                        priority: game_genetics::random(
                            self.seed ^ 0x72656c65617365,
                            actor.id,
                            self.tick,
                        ),
                        effort: self.owner.person_effort(actor.id),
                    })
            })
            .collect();
        if choices.is_empty() {
            return Vec::new();
        }
        let selected: Vec<_> = if choices.par_iter().all(|c| c.target == choices[0].target) {
            choices
                .par_iter()
                .min_by_key(|c| (c.priority, c.actor))
                .copied()
                .into_iter()
                .collect()
        } else {
            if choices.par_windows(2).any(|pair| {
                (pair[0].target, pair[0].priority, pair[0].actor)
                    > (pair[1].target, pair[1].priority, pair[1].actor)
            }) {
                choices.par_sort_unstable_by_key(|c| (c.target, c.priority, c.actor));
            }
            choices
                .par_chunk_by(|a, b| a.target == b.target)
                .map(|group| group[0])
                .collect()
        };
        let results = opening_results(selected, self.opening_guards);
        let mut targets: Vec<_> = results
            .par_iter()
            .filter_map(|(c, opened)| opened.then_some(c.target))
            .collect();
        if targets.par_windows(2).any(|p| p[0] > p[1]) {
            targets.par_sort_unstable();
        }
        let dense = targets
            .first()
            .zip(targets.last())
            .is_some_and(|(first, last)| {
                last.checked_sub(*first) == Some((targets.len() - 1) as u64)
            });
        self.owner.holds = self
            .owner
            .holds
            .par_iter()
            .filter(|hold| {
                if dense {
                    hold.person < targets[0] || hold.person > targets[targets.len() - 1]
                } else {
                    targets.binary_search(&hold.person).is_err()
                }
            })
            .cloned()
            .collect();
        self.owner
            .events
            .par_extend(results.par_iter().map(|(choice, opened)| Event {
                tick: self.tick,
                person: choice.target,
                actor: Some(choice.actor),
                site: choice.site,
                kind: if *opened {
                    EventKind::Released
                } else {
                    EventKind::ReleaseResisted
                },
            }));
        if self
            .owner
            .events
            .par_windows(2)
            .any(|p| (p[0].person, p[0].actor, p[0].kind) > (p[1].person, p[1].actor, p[1].kind))
        {
            self.owner
                .events
                .par_sort_unstable_by_key(|event| (event.person, event.actor, event.kind));
        }
        let mut actors: Vec<_> = results.par_iter().map(|(c, _)| c.actor).collect();
        if actors.par_windows(2).any(|p| p[0] > p[1]) {
            actors.par_sort_unstable();
        }
        actors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_effort_is_spent_once_across_parallel_block_boundaries() {
        let choices: Vec<_> = (0..4100)
            .map(|i| Choice {
                actor: i + 1,
                target: i + 5000,
                site: if i < 2200 { 1 } else { 2 },
                priority: i,
                effort: if i % 5 == 0 { 2 } else { 1 },
            })
            .collect();
        let guards = [(1, 1422), (2, 1250)];
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| opening_results(choices.clone(), &guards))
        };
        let rows = run(1);
        assert_eq!(
            rows.iter()
                .map(|(c, ok)| (c.actor, *ok))
                .collect::<Vec<_>>(),
            run(4)
                .iter()
                .map(|(c, ok)| (c.actor, *ok))
                .collect::<Vec<_>>()
        );
        let mut site = 0;
        let mut spent = 0u128;
        for (choice, opened) in rows {
            if site != choice.site {
                site = choice.site;
                spent = 0;
            }
            let remaining = guards[(site - 1) as usize].1.saturating_sub(spent);
            assert_eq!(
                opened,
                remaining == 0 || u128::from(choice.effort) > remaining
            );
            spent = spent.saturating_add(u128::from(choice.effort));
        }
    }

    #[test]
    fn opening_effort_does_not_overflow_when_people_are_extremely_strong() {
        let choices = (1..=3)
            .map(|id| Choice {
                actor: id,
                target: id + 3,
                site: 1,
                priority: id,
                effort: u64::MAX,
            })
            .collect();
        let results = opening_results(choices, &[(1, 2 * u128::from(u64::MAX))]);
        assert_eq!(
            results
                .into_iter()
                .map(|(_, opened)| opened)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
    }
}
