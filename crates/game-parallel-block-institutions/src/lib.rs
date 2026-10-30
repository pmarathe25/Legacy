//! Checked institutional commands for compiled worlds.
pub mod membership;
pub mod turnover;
use game_parallel::{InstitutionAction, InstitutionVote, World};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
pub struct Context<'a> {
    world: &'a mut World,
    people: &'a [PersonSnapshot],
}
impl Context<'_> {
    pub fn new<'a>(world: &'a mut World, people: &'a [PersonSnapshot]) -> Context<'a> {
        Context { world, people }
    }
    fn suspended(&self) -> Option<u64> {
        self.world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
    }
    pub fn open(
        &mut self,
        body: u64,
        proposer: u64,
        action: InstitutionAction,
        duration: u64,
    ) -> Result<u64, String> {
        if self.suspended() == Some(proposer) {
            return Err("player control is suspended".into());
        }
        self.world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?
            .open(
                self.people,
                body,
                proposer,
                action,
                duration,
                self.world.tick,
            )
    }
    pub fn vote(&mut self, requests: &[InstitutionVote]) -> Result<(), String> {
        if self
            .suspended()
            .is_some_and(|person| requests.par_iter().any(|vote| vote.voter == person))
        {
            return Err("player control is suspended".into());
        }
        self.world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?
            .vote(self.people, requests)
    }
    pub fn apply(&mut self, body: u64, applicant: u64) -> Result<(), String> {
        if self.suspended() == Some(applicant) {
            return Err("player control is suspended".into());
        }
        let institutions = self
            .world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?;
        let institution = institutions
            .bodies
            .iter()
            .find(|row| row.id == body)
            .ok_or("unknown institution")?;
        let local = person_index(self.people, applicant).is_some_and(|index| {
            self.people[index].1 == Some(institution.region) && self.people[index].2 > 0
        });
        if !local
            || institution.members.is_empty()
            || institution.members.contains(&applicant)
            || institutions
                .requests
                .iter()
                .any(|row| row.body == body && row.applicant == applicant)
        {
            return Err(
                "applicant must be living, local, and not already a member or applicant".into(),
            );
        }
        institutions
            .requests
            .push(game_parallel::InstitutionMembershipRequest {
                body,
                applicant,
                requested_at: self.world.tick,
                votes: vec![],
            });
        Ok(())
    }
    pub fn leave(&mut self, body: u64, person: u64) -> Result<(), String> {
        if self.suspended() == Some(person) {
            return Err("player control is suspended".into());
        }
        let institutions = self
            .world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?;
        let institution = institutions
            .bodies
            .iter_mut()
            .find(|row| row.id == body)
            .ok_or("unknown institution")?;
        let local = person_index(self.people, person).is_some_and(|index| {
            self.people[index].1 == Some(institution.region) && self.people[index].2 > 0
        });
        let member = institution
            .members
            .iter()
            .position(|&member| member == person)
            .ok_or("person is not a member")?;
        if !local {
            return Err("member must be living and local".into());
        }
        institution.members.remove(member);
        if institution.chair == Some(person) {
            institution.chair = None;
        }
        let vacant = institution.members.is_empty();
        let tick = self.world.tick;
        let mut cancelled = Vec::new();
        institutions.ballots.retain_mut(|ballot| {
            if ballot.body != body { return true; }
            let cancel = ballot.proposer == person
                || matches!(ballot.action, InstitutionAction::Elect { candidate } if candidate == person);
            if cancel {
                cancelled.push(game_parallel::InstitutionEvent { tick, ballot: ballot.id, body, adopted: false });
            } else {
                ballot.votes.retain(|vote| vote.voter != person);
            }
            !cancel
        });
        institutions.events.extend(cancelled);
        institutions
            .requests
            .par_iter_mut()
            .filter(|request| request.body == body)
            .for_each(|request| request.votes.retain(|vote| vote.voter != person));
        if vacant {
            institutions.requests.retain(|request| {
                if request.body != body {
                    return true;
                }
                institutions
                    .admissions
                    .push(game_parallel::InstitutionAdmissionEvent {
                        tick,
                        body,
                        applicant: request.applicant,
                        accepted: false,
                        yes: 0,
                        no: 0,
                    });
                false
            });
        }
        Ok(())
    }
    pub fn vote_application(
        &mut self,
        body: u64,
        applicant: u64,
        voter: u64,
        yes: bool,
    ) -> Result<(), String> {
        if self.suspended() == Some(voter) {
            return Err("player control is suspended".into());
        }
        let institutions = self
            .world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?;
        let institution = institutions
            .bodies
            .iter()
            .find(|row| row.id == body)
            .ok_or("unknown institution")?;
        let local = person_index(self.people, voter).is_some_and(|index| {
            self.people[index].1 == Some(institution.region) && self.people[index].2 > 0
        });
        if !local || !institution.members.contains(&voter) {
            return Err("voter must be a living local member".into());
        }
        let request = institutions
            .requests
            .iter_mut()
            .find(|row| row.body == body && row.applicant == applicant)
            .ok_or("unknown membership application")?;
        if request.votes.iter().any(|vote| vote.voter == voter) {
            return Err("member has already voted".into());
        }
        request
            .votes
            .push(game_parallel::InstitutionRecordedVote { voter, yes });
        request.votes.sort_unstable_by_key(|vote| vote.voter);
        Ok(())
    }
    pub fn withdraw_application(&mut self, body: u64, applicant: u64) -> Result<(), String> {
        if self.suspended() == Some(applicant) {
            return Err("player control is suspended".into());
        }
        if person_index(self.people, applicant).is_none_or(|index| self.people[index].2 == 0) {
            return Err("applicant must be living".into());
        }
        let institutions = self
            .world
            .institutions
            .as_mut()
            .ok_or("institutions are disabled")?;
        let index = institutions
            .requests
            .iter()
            .position(|row| row.body == body && row.applicant == applicant)
            .ok_or("unknown membership application")?;
        institutions.requests.remove(index);
        Ok(())
    }
}
