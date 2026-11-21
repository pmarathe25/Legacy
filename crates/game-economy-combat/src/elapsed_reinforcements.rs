//! Resolve personally chosen entries into existing tactical engagements.
use crate::{
    Point,
    elapsed::{Choice, Participant, Tactical},
};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
pub struct Request {
    pub person: u64,
    pub beneficiary: u64,
    pub opponent: u64,
    pub fight_on_entry: bool,
}

pub(crate) struct Admission {
    pub person: u64,
    pub opponent: u64,
    pub engagement: u64,
    pub placement: Option<Participant>,
}

pub(crate) fn prepare(
    owner: &Tactical,
    requests: &[Request],
    seed: u64,
    now: u128,
) -> Vec<Admission> {
    let mut requested: Vec<_> = requests
        .par_iter()
        .filter_map(|request| {
            let fight = owner.engagement(request.beneficiary)?;
            if request.person == request.opponent
                || request.beneficiary == request.opponent
                || owner.engagement(request.person).is_some()
                || fight
                    .people
                    .binary_search_by_key(&request.opponent, |row| row.person)
                    .is_err()
            {
                return None;
            }
            Some((fight.id, *request))
        })
        .collect();
    requested.par_sort_unstable_by_key(|row| (row.0, row.1.person));
    let owners: HashMap<_, _> = requested
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| {
            let index = owner
                .engagements
                .binary_search_by_key(&group[0].0, |row| row.id)
                .ok()?;
            let fight = &owner.engagements[index];
            let occupied: HashSet<_> = fight
                .people
                .par_iter()
                .map(|fighter| (fighter.position.x, fighter.position.y))
                .collect();
            Some((fight.id, (fight, occupied)))
        })
        .collect();
    let mut proposals: Vec<_> = requested
        .par_iter()
        .map(|&(engagement, request)| {
            let (fight, occupied) = &owners[&engagement];
            let ally = fight.people[fight
                .people
                .binary_search_by_key(&request.beneficiary, |row| row.person)
                .expect("validated ally")]
            .position;
            let map = owner.map(fight.region).expect("validated map");
            let radius = map.rules.movement.clamp(1, 8);
            let start_x = ally.x.saturating_sub(radius);
            let start_y = ally.y.saturating_sub(radius);
            let end_x = ally.x.saturating_add(radius).min(map.rules.width);
            let end_y = ally.y.saturating_add(radius).min(map.rules.height);
            let spot = (start_x..=end_x)
                .flat_map(|x| (start_y..=end_y).map(move |y| Point { x, y }))
                .filter(|point| {
                    !occupied.contains(&(point.x, point.y)) && map.visible(*point, ally)
                })
                .min_by_key(|point| {
                    (
                        u64::from(point.x.abs_diff(ally.x)) + u64::from(point.y.abs_diff(ally.y)),
                        point.x,
                        point.y,
                    )
                });
            let rank = mixer(
                seed ^ request.person.rotate_left(17)
                    ^ engagement.rotate_left(11)
                    ^ now as u64
                    ^ ((now >> 64) as u64),
            );
            (engagement, request, spot, rank)
        })
        .collect();
    proposals.par_sort_unstable_by_key(|&(fight, request, point, rank)| {
        (fight, point.map(|p| (p.x, p.y)), rank, request.person)
    });
    proposals
        .par_chunk_by(|a, b| a.0 == b.0 && a.2 == b.2)
        .flat_map_iter(|group| {
            group
                .iter()
                .enumerate()
                .map(|(index, &(engagement, request, spot, _))| {
                    let placement = spot.filter(|_| index == 0).map(|position| Participant {
                        person: request.person,
                        position,
                        choice: if request.fight_on_entry {
                            Choice::Attack {
                                target: request.opponent,
                                pursue: true,
                            }
                        } else {
                            Choice::Wait
                        },
                    });
                    Admission {
                        person: request.person,
                        opponent: request.opponent,
                        engagement,
                        placement,
                    }
                })
        })
        .collect()
}

fn mixer(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
