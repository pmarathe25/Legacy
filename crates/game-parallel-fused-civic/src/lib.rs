//! Reserve civic actors and prepare their votes with ordinary institution rules.
mod application;
mod commands;
mod founding;
mod resignation;
use game_parallel::{Institutions, World};
use game_parallel_fused_data::{Block, NONE};
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
/// Publish member deaths after physical and family outcomes have committed.
pub fn remove_dead(
    institutions: &mut Option<Institutions>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    known_deaths: Option<&[u64]>,
    tick: u64,
) {
    let Some(owner) = institutions.as_ref() else {
        return;
    };
    let scanned;
    let deaths = if let Some(deaths) = known_deaths {
        deaths
    } else {
        scanned = owner
            .bodies
            .par_iter()
            .flat_map_iter(|body| {
                body.members.iter().filter_map(|&person| {
                    let index = positions.binary_search_by_key(&person, |row| row.0).ok()?;
                    let (_, block, slot) = positions[index];
                    (blocks[block].health[slot] == 0).then_some(person)
                })
            })
            .collect::<Vec<_>>();
        &scanned
    };
    game_parallel_block_institutions::turnover::remove_dead(institutions, deaths, tick);
}
pub struct Draft {
    pub actors: Vec<u64>,
    pub institutions: Institutions,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    people: &[PersonSnapshot],
    reserved: &[u64],
    next: u64,
) -> Result<Option<Draft>, &'static str> {
    let Some(institutions) = world.institutions.as_ref() else {
        return if world.civic_auto.is_some() {
            Err("civic preferences require institutions")
        } else {
            Ok(None)
        };
    };
    let fallback;
    let policy = if let Some(policy) = &world.civic_auto {
        policy
    } else if institutions
        .requests
        .par_iter()
        .any(|request| !request.votes.is_empty())
    {
        fallback = game_parallel::CivicAuto {
            policies: Vec::new(),
            founding: None,
        };
        &fallback
    } else {
        return Ok(None);
    };
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &handle)| (handle != NONE).then_some(id))
        })
        .collect();
    busy.extend_from_slice(reserved);
    let departures = resignation::prepare(world, people, &busy);
    busy.extend(departures.par_iter().map(|row| row.1).collect::<Vec<_>>());
    let votes = policy.prepare(institutions, world.opinions.as_ref(), people, &busy);
    if let Some(votes) = &votes {
        busy.extend_from_slice(&votes.actors);
    }
    let admissions = policy.admissions(institutions, world.opinions.as_ref(), people, &busy, next);
    if let Some(admissions) = &admissions {
        busy.extend_from_slice(&admissions.actors);
    }
    let applications = application::prepare(world, people, &busy, next);
    busy.extend(
        applications
            .par_iter()
            .map(|row| row.applicant)
            .collect::<Vec<_>>(),
    );
    let founded = founding::prepare(policy, institutions, world.opinions.as_ref(), people, &busy)?;
    if votes.is_none()
        && admissions.is_none()
        && founded.is_empty()
        && departures.is_empty()
        && applications.is_empty()
    {
        return Ok(None);
    }
    let mut actors: Vec<_> = founded
        .par_iter()
        .flat_map_iter(|body| body.members.iter().copied())
        .collect();
    actors.extend(departures.par_iter().map(|row| row.1).collect::<Vec<_>>());
    actors.extend(
        applications
            .par_iter()
            .map(|row| row.applicant)
            .collect::<Vec<_>>(),
    );
    if let Some(votes) = &votes {
        actors.extend_from_slice(&votes.actors);
    }
    if let Some(admissions) = &admissions {
        actors.extend_from_slice(&admissions.actors);
    }
    let mut staged = institutions.clone();
    if let Some(votes) = &votes
        && !votes.votes.is_empty()
    {
        staged
            .vote(people, &votes.votes)
            .map_err(|_| "invalid automatic vote")?;
    }
    staged.prepare(people, next).commit(&mut staged);
    let proposals: Vec<_> = votes
        .as_ref()
        .map_or(&[][..], |votes| votes.proposals.as_slice())
        .par_iter()
        .map(|proposal| game_parallel::InstitutionOpen {
            body: proposal.body,
            proposer: proposal.proposer,
            action: game_parallel::InstitutionAction::Trade {
                open: proposal.open,
            },
            duration: proposal.duration,
        })
        .collect();
    staged
        .open_batch(people, &proposals, next)
        .map_err(|_| "invalid automatic proposal")?;
    if let Some(admissions) = admissions {
        game_parallel_block_institutions::membership::publish(
            &mut staged,
            admissions.into_decisions(),
        );
    }
    game_parallel_block_institutions::turnover::remove_departures(&mut staged, &departures, next);
    application::commit(&mut staged, applications);
    staged.bodies.par_extend(founded);
    Ok(Some(Draft {
        actors,
        institutions: staged,
    }))
}

#[cfg(test)]
#[path = "../tests/support/admissions.rs"]
mod admission_tests;
