//! Independent choices to open a physical cell for a particular person.
use super::{Confinement, Event, EventKind, ordered_index};
use game_parallel_economy_person::Person;
use game_parallel_opinions::Opinions;
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;

pub struct Context<'a> {
    pub owner: &'a mut Confinement,
    pub people: &'a [Person],
    pub sites: &'a Facilities,
    pub busy: &'a [u64],
    /// Targets reserved by an explicit player action.
    pub reserved_targets: &'a [u64],
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
                        priority: game_parallel_genetics::random(
                            self.seed ^ 0x72656c65617365,
                            actor.id,
                            self.tick,
                        ),
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
        let mut targets: Vec<_> = selected.par_iter().map(|c| c.target).collect();
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
            .par_extend(selected.par_iter().map(|choice| Event {
                tick: self.tick,
                person: choice.target,
                actor: Some(choice.actor),
                site: choice.site,
                kind: EventKind::Released,
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
        let mut actors: Vec<_> = selected.par_iter().map(|c| c.actor).collect();
        if actors.par_windows(2).any(|p| p[0] > p[1]) {
            actors.par_sort_unstable();
        }
        actors
    }
}
