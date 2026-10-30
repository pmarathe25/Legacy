//! Membership-scoped views of local civic decisions.
use crate::Location;
use game_parallel::{InstitutionAction, InstitutionBody, World};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BallotView {
    pub id: u64,
    pub action: InstitutionAction,
    pub closes_at: u64,
    pub votes: usize,
    pub own_vote: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstitutionView {
    pub body: InstitutionBody,
    pub member: bool,
    pub ballots: Vec<BallotView>,
    pub applications: Vec<ApplicationView>,
    pub admissions: Vec<game_parallel::InstitutionAdmissionEvent>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationView {
    pub applicant: u64,
    pub requested_at: u64,
    pub votes: usize,
    pub own_vote: Option<bool>,
}

pub fn institutions(world: &World, person: u64, location: &Location) -> Vec<InstitutionView> {
    let Location::Local { region, .. } = location else {
        return Vec::new();
    };
    world.institutions.as_ref().map_or_else(Vec::new, |owner| {
        owner
            .bodies
            .iter()
            .filter(|body| body.region == *region && !body.members.is_empty())
            .map(|body| InstitutionView {
                body: body.clone(),
                member: body.members.contains(&person),
                admissions: owner
                    .admissions
                    .iter()
                    .filter(|event| event.body == body.id && body.members.contains(&person))
                    .cloned()
                    .collect(),
                applications: owner
                    .requests
                    .iter()
                    .filter(|request| request.body == body.id && body.members.contains(&person))
                    .map(|request| ApplicationView {
                        applicant: request.applicant,
                        requested_at: request.requested_at,
                        votes: request.votes.len(),
                        own_vote: request
                            .votes
                            .iter()
                            .find(|vote| vote.voter == person)
                            .map(|vote| vote.yes),
                    })
                    .collect(),
                ballots: owner
                    .ballots
                    .iter()
                    .filter(|ballot| ballot.body == body.id && body.members.contains(&person))
                    .map(|ballot| BallotView {
                        id: ballot.id,
                        action: ballot.action.clone(),
                        closes_at: ballot.closes_at,
                        votes: ballot.votes.len(),
                        own_vote: ballot
                            .votes
                            .iter()
                            .find(|vote| vote.voter == person)
                            .map(|vote| vote.yes),
                    })
                    .collect(),
            })
            .collect()
    })
}
