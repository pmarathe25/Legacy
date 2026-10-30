//! Regional institutions with explicit ballots, offices and trade law.
mod requests;
use game_parallel_index::membership::Membership;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
pub use requests::{AdmissionEvent, MembershipRequest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Institutions {
    pub next_id: u64,
    pub bodies: Vec<Body>,
    #[serde(default)]
    pub ballots: Vec<Ballot>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub requests: Vec<MembershipRequest>,
    #[serde(default)]
    pub admissions: Vec<AdmissionEvent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    pub id: u64,
    pub region: u64,
    pub members: Vec<u64>,
    pub quorum: u32,
    pub chair: Option<u64>,
    pub trade_open: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ballot {
    pub id: u64,
    pub body: u64,
    pub proposer: u64,
    pub action: Action,
    pub closes_at: u64,
    #[serde(default)]
    pub votes: Vec<Vote>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Elect { candidate: u64 },
    Trade { open: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vote {
    pub voter: u64,
    pub yes: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub body: u64,
    pub proposer: u64,
    pub action: Action,
    pub duration: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cast {
    pub ballot: u64,
    pub voter: u64,
    pub yes: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub ballot: u64,
    pub body: u64,
    pub adopted: bool,
}
pub struct Draft {
    next: u64,
    changes: Vec<(Option<u64>, bool)>,
    events: Vec<Event>,
}
impl Draft {
    pub fn commit(self, institutions: &mut Institutions) {
        institutions
            .bodies
            .par_iter_mut()
            .zip(self.changes.into_par_iter())
            .for_each(|(body, (chair, trade_open))| {
                body.chair = chair;
                body.trade_open = trade_open;
            });
        institutions
            .ballots
            .retain(|ballot| ballot.closes_at > self.next);
        institutions.events = self.events;
    }
}
impl Institutions {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        now: u64,
    ) -> Result<(), String> {
        let mut ids: Vec<_> = self.bodies.par_iter().map(|body| body.id).collect();
        let mut locations: Vec<_> = self
            .bodies
            .par_iter()
            .filter(|body| !body.members.is_empty())
            .map(|body| body.region)
            .collect();
        ids.par_sort_unstable();
        locations.par_sort_unstable();
        let members: Vec<_> = self
            .bodies
            .par_iter()
            .map(|body| Membership::new(&body.members))
            .collect();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || locations.par_windows(2).any(|pair| pair[0] == pair[1])
            || self
                .bodies
                .par_iter()
                .zip(members.par_iter())
                .any(|(body, members)| {
                    !regions.contains(&body.region)
                        || body.quorum == 0
                        || members.duplicates()
                        || body
                            .members
                            .par_iter()
                            .any(|person| !people.contains(person))
                        || body.chair.is_some_and(|chair| !members.contains(chair))
                })
        {
            return Err("invalid institution".into());
        }
        let bodies: HashMap<_, _> = self
            .bodies
            .iter()
            .zip(&members)
            .map(|(body, members)| (body.id, (body, members)))
            .collect();
        let mut ballot_ids: Vec<_> = self.ballots.par_iter().map(|ballot| ballot.id).collect();
        let mut active: Vec<_> = self.ballots.par_iter().map(|ballot| ballot.body).collect();
        ballot_ids.par_sort_unstable();
        active.par_sort_unstable();
        if ballot_ids.par_windows(2).any(|pair| pair[0]==pair[1]) || active.par_windows(2).any(|pair| pair[0]==pair[1])
            || self.ballots.par_iter().any(|ballot| {
                let Some((body,members))=bodies.get(&ballot.body) else { return true; };
                let mut voters:Vec<_>=ballot.votes.par_iter().map(|vote| vote.voter).collect();
                if voters.par_windows(2).any(|pair| pair[0]>pair[1]) { voters.par_sort_unstable(); }
                ballot.id>=self.next_id || ballot.closes_at<=now || !members.contains(ballot.proposer)
                    || matches!(ballot.action,Action::Elect{candidate} if !members.contains(candidate))
                    || voters.par_windows(2).any(|pair| pair[0]==pair[1])
                    || ballot.votes.par_iter().any(|vote| !members.contains(vote.voter))
                    || body.quorum==0
            }) || self.events.par_iter().any(|event| event.tick>now || event.ballot>=self.next_id || ids.binary_search(&event.body).is_err()) {
            return Err("invalid institutional ballot".into());
        }
        self.validate_requests(people, now)?;
        Ok(())
    }
    fn local(people: &[PersonSnapshot], actor: u64, region: u64) -> bool {
        person_index(people, actor).is_some_and(|i| people[i].1 == Some(region) && people[i].2 > 0)
    }
    pub fn open(
        &mut self,
        people: &[PersonSnapshot],
        body: u64,
        proposer: u64,
        action: Action,
        duration: u64,
        now: u64,
    ) -> Result<u64, String> {
        let body = self
            .bodies
            .iter()
            .find(|candidate| candidate.id == body)
            .ok_or("unknown institution")?;
        if body.members.len() < body.quorum as usize
            || duration == 0
            || !Membership::new(&body.members).contains(proposer)
            || !Self::local(people, proposer, body.region)
        {
            return Err("proposer must be a living local member and duration positive".into());
        }
        if self.ballots.iter().any(|ballot| ballot.body == body.id) {
            return Err("institution already has an open ballot".into());
        }
        if let Action::Elect { candidate } = action
            && (!Membership::new(&body.members).contains(candidate)
                || !Self::local(people, candidate, body.region))
        {
            return Err("candidate must be a living local member".into());
        }
        let closes_at = now
            .checked_add(duration)
            .ok_or("ballot deadline overflow")?;
        let next = self.next_id.checked_add(1).ok_or("ballot ID overflow")?;
        let id = self.next_id;
        self.ballots.push(Ballot {
            id,
            body: body.id,
            proposer,
            action,
            closes_at,
            votes: Vec::new(),
        });
        self.next_id = next;
        Ok(id)
    }
    /// Open independent ballots against one committed snapshot, or reject the whole batch.
    pub fn open_batch(
        &mut self,
        people: &[PersonSnapshot],
        requests: &[Open],
        now: u64,
    ) -> Result<Vec<u64>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let count = u64::try_from(requests.len()).map_err(|_| "ballot count overflow")?;
        let next = self
            .next_id
            .checked_add(count)
            .ok_or("ballot ID overflow")?;
        let memberships: Vec<_> = self
            .bodies
            .par_iter()
            .map(|body| Membership::new(&body.members))
            .collect();
        let bodies: HashMap<_, _> = self
            .bodies
            .iter()
            .zip(&memberships)
            .map(|(body, members)| (body.id, (body, members)))
            .collect();
        let active: HashSet<_> = self.ballots.iter().map(|ballot| ballot.body).collect();
        let mut requested: Vec<_> = requests.par_iter().map(|request| request.body).collect();
        requested.par_sort_unstable();
        if requested.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate institution ballot".into());
        }
        let rows: Vec<_> = requests
            .par_iter()
            .enumerate()
            .map(|(index, request)| -> Result<Ballot, String> {
                let (body, members) = bodies.get(&request.body).ok_or("unknown institution")?;
                if body.members.len() < body.quorum as usize
                    || request.duration == 0
                    || active.contains(&request.body)
                    || !members.contains(request.proposer)
                    || !Self::local(people, request.proposer, body.region)
                {
                    return Err("invalid ballot proposer or active ballot".into());
                }
                if let Action::Elect { candidate } = request.action
                    && (!members.contains(candidate)
                        || !Self::local(people, candidate, body.region))
                {
                    return Err("candidate must be a living local member".into());
                }
                let closes_at = now
                    .checked_add(request.duration)
                    .ok_or("ballot deadline overflow")?;
                Ok(Ballot {
                    id: self.next_id + index as u64,
                    body: request.body,
                    proposer: request.proposer,
                    action: request.action.clone(),
                    closes_at,
                    votes: Vec::new(),
                })
            })
            .collect::<Result<_, _>>()?;
        let ids = rows.par_iter().map(|ballot| ballot.id).collect();
        self.ballots.extend(rows);
        self.next_id = next;
        Ok(ids)
    }
    /// Every ballot in the batch uses one committed snapshot and the batch is atomic.
    pub fn vote(&mut self, people: &[PersonSnapshot], requests: &[Cast]) -> Result<(), String> {
        let memberships: Vec<_> = self
            .bodies
            .par_iter()
            .map(|body| Membership::new(&body.members))
            .collect();
        let bodies: HashMap<_, _> = self
            .bodies
            .iter()
            .zip(&memberships)
            .map(|(body, members)| (body.id, (body, members)))
            .collect();
        let ballots: HashMap<_, _> = self
            .ballots
            .iter()
            .map(|ballot| (ballot.id, ballot))
            .collect();
        let existing: HashMap<u64, Vec<u64>> = self
            .ballots
            .par_iter()
            .map(|ballot| {
                let mut voters: Vec<_> = ballot.votes.par_iter().map(|vote| vote.voter).collect();
                if voters.par_windows(2).any(|pair| pair[0] > pair[1]) {
                    voters.par_sort_unstable();
                }
                (ballot.id, voters)
            })
            .collect();
        let mut sorted = None;
        if requests
            .par_windows(2)
            .any(|pair| (pair[0].ballot, pair[0].voter) > (pair[1].ballot, pair[1].voter))
        {
            let mut rows: Vec<_> = requests.par_iter().copied().collect();
            rows.par_sort_unstable_by_key(|vote| (vote.ballot, vote.voter));
            sorted = Some(rows);
        }
        let ordered = sorted.as_deref().unwrap_or(requests);
        if ordered
            .par_windows(2)
            .any(|pair| (pair[0].ballot, pair[0].voter) == (pair[1].ballot, pair[1].voter))
            || ordered.par_iter().any(|vote| {
                let Some(ballot) = ballots.get(&vote.ballot) else {
                    return true;
                };
                let (body, members) = &bodies[&ballot.body];
                body.members.len() < body.quorum as usize
                    || !members.contains(vote.voter)
                    || !Self::local(people, vote.voter, body.region)
                    || existing[&vote.ballot].binary_search(&vote.voter).is_ok()
            })
        {
            return Err("invalid or duplicate vote".into());
        }
        self.ballots.par_iter_mut().for_each(|ballot| {
            let start = ordered.partition_point(|vote| vote.ballot < ballot.id);
            let end = ordered.partition_point(|vote| vote.ballot <= ballot.id);
            if start == end {
                return;
            }
            let new_votes = ordered[start..end].par_iter().map(|request| Vote {
                voter: request.voter,
                yes: request.yes,
            });
            let mut votes: Vec<_> = ballot.votes.par_iter().cloned().chain(new_votes).collect();
            if votes
                .par_windows(2)
                .any(|pair| pair[0].voter > pair[1].voter)
            {
                votes.par_sort_unstable_by_key(|vote| vote.voter);
            }
            ballot.votes = votes;
        });
        Ok(())
    }
    /// Ballots resolve from the start-of-tick snapshot, before new laws take effect.
    pub fn prepare(&self, people: &[PersonSnapshot], next: u64) -> Draft {
        let bodies: HashMap<_, _> = self.bodies.iter().map(|body| (body.id, body)).collect();
        let decisions: Vec<_> = self
            .ballots
            .par_iter()
            .filter(|ballot| ballot.closes_at <= next)
            .map(|ballot| {
                let body = bodies[&ballot.body];
                let yes = ballot.votes.par_iter().filter(|vote| vote.yes).count();
                let no = ballot.votes.len() - yes;
                let eligible = match ballot.action {
                    Action::Elect { candidate } => Self::local(people, candidate, body.region),
                    Action::Trade { .. } => true,
                };
                (
                    ballot.body,
                    ballot.id,
                    ballot.action.clone(),
                    eligible && yes >= body.quorum as usize && yes > no,
                )
            })
            .collect();
        let by_body: HashMap<_, _> = decisions.iter().map(|row| (row.0, row)).collect();
        let changes = self
            .bodies
            .par_iter()
            .map(|body| {
                let mut chair = body
                    .chair
                    .filter(|chair| person_index(people, *chair).is_some_and(|i| people[i].2 > 0));
                let mut trade_open = body.trade_open;
                if let Some((_, _, action, true)) = by_body.get(&body.id).copied() {
                    match action {
                        Action::Elect { candidate } => chair = Some(*candidate),
                        Action::Trade { open } => trade_open = *open,
                    }
                }
                (chair, trade_open)
            })
            .collect();
        let mut events: Vec<_> = decisions
            .into_par_iter()
            .map(|(body, ballot, _, adopted)| Event {
                tick: next,
                ballot,
                body,
                adopted,
            })
            .collect();
        events.par_sort_unstable_by_key(|event| event.ballot);
        Draft {
            next,
            changes,
            events,
        }
    }
    pub fn closed_regions(&self) -> HashSet<u64> {
        self.bodies
            .par_iter()
            .filter(|body| !body.trade_open && body.members.len() >= body.quorum as usize)
            .map(|body| body.region)
            .collect()
    }
}

pub fn prepare_world(
    institutions: Option<&Institutions>,
    regions: &[game_parallel_people::Region],
    travelers: &[game_parallel_transport::Traveler],
    next: u64,
) -> Option<Draft> {
    institutions.map(|institutions| {
        institutions.prepare(
            &game_parallel_index::snapshot_people(regions, travelers),
            next,
        )
    })
}

pub fn commit_optional(draft: Option<Draft>, institutions: &mut Option<Institutions>) {
    if let Some(draft) = draft {
        draft.commit(institutions.as_mut().expect("prepared institutions"));
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod integration_tests;
