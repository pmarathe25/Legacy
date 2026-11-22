//! Local tactical engagements with independent elapsed round deadlines.
use crate::elapsed_structures::{SiteStrike, Structures};
use crate::{Combat, Map, Point, Protection, Reaction, Response, Weapon};
use crate::{elapsed_reinforcements, intervention};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Choice {
    #[default]
    Wait,
    Defend,
    Attack {
        target: u64,
        pursue: bool,
    },
    AttackSite {
        site: u64,
    },
    Move {
        destination: Point,
    },
    Retreat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub person: u64,
    pub position: Point,
    #[serde(default)]
    pub choice: Choice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engagement {
    pub id: u64,
    pub region: u64,
    pub sequence: u64,
    pub next_millis: u128,
    pub people: Vec<Participant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Started,
    Entered,
    EntryBlocked,
    Moved,
    MovementBlocked,
    Defended,
    Attacked,
    Missed,
    Withdrew,
    Incapacitated,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at_millis: u128,
    pub engagement: u64,
    pub sequence: u64,
    pub person: Option<u64>,
    pub target: Option<u64>,
    pub position: Option<Point>,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tactical {
    pub round_millis: u64,
    pub base_force: u32,
    pub next_id: u64,
    pub maps: Vec<Map>,
    #[serde(default, skip_serializing_if = "Structures::is_empty")]
    pub structures: Structures,
    pub engagements: Vec<Engagement>,
    pub reactions: Vec<Reaction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interventions: Vec<intervention::Preference>,
    pub protection: Vec<Protection>,
    pub weapons: Vec<Weapon>,
    pub membership: Vec<(u64, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_millis: Option<u128>,
}

/// Information projected once from real people, equipment, and personal beliefs.
#[derive(Clone, Copy)]
pub struct Actor {
    pub id: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub can_act: bool,
    pub controlled: bool,
    pub force: u32,
    pub armor: u32,
}

#[derive(Clone, Copy)]
pub struct Hit {
    pub actor: u64,
    pub target: u64,
    pub force: u32,
}

#[derive(Clone, Copy)]
pub struct Start {
    pub actor: u64,
    pub target: u64,
    pub region: u64,
}

pub struct Admissions {
    opening: u64,
    next_id: u64,
    fights: Vec<Engagement>,
    events: Vec<Event>,
}

impl Admissions {
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

pub struct Round {
    pub at_millis: u128,
    pub changes: Vec<Engagement>,
    pub events: Vec<Event>,
    pub hits: Vec<Hit>,
    pub site_strikes: Vec<SiteStrike>,
    pub protection: Vec<(u64, u32)>,
}

impl Engagement {
    fn event(&self, now: u128, person: Option<u64>, kind: EventKind) -> Event {
        Event {
            at_millis: now,
            engagement: self.id,
            sequence: self.sequence,
            person,
            target: None,
            position: None,
            kind,
        }
    }
}

impl Tactical {
    /// Import generated map and configuration data without invoking the tick driver.
    pub fn from_fresh(combat: Combat, round_millis: u64) -> Result<Self, String> {
        if !combat.conflict.events.is_empty()
            || !combat.conflict.encounters.is_empty()
            || !combat.tactics.battles.is_empty()
        {
            return Err("elapsed tactical import requires a fresh map".into());
        }
        let mut maps = combat.tactics.maps;
        maps.par_sort_unstable_by_key(|row| row.region);
        let mut reactions = combat.reactions;
        reactions.par_sort_unstable_by_key(|row| row.person);
        let mut interventions = combat.interventions;
        interventions.par_sort_unstable_by_key(|row| row.person);
        Ok(Self {
            round_millis,
            base_force: combat.conflict.damage,
            next_id: combat.conflict.next_id,
            maps,
            structures: Structures::new(combat.structures, combat.structure_preferences),
            engagements: Vec::new(),
            reactions,
            interventions,
            weapons: combat.weapons,
            protection: combat.protection,
            membership: Vec::new(),
            next_millis: None,
        })
    }

    pub fn validate(
        &self,
        now: u128,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        goods: &HashSet<u32>,
    ) -> Result<(), String> {
        let membership = self.index_membership();
        if self.round_millis == 0 || self.base_force == 0 || self.next_id == 0
            || self.maps.par_windows(2).any(|pair| pair[0].region >= pair[1].region)
            || self.maps.par_iter().any(|map| !regions.contains(&map.region) || map.validate().is_err())
            || self.engagements.par_windows(2).any(|pair| pair[0].id >= pair[1].id)
            || membership.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
            || self.membership != membership
            || self.engagements.par_iter().map(|fight| fight.next_millis).min() != self.next_millis
            || self.reactions.par_windows(2).any(|pair| pair[0].person >= pair[1].person)
            || self.reactions.par_iter().any(|row| !people.contains(&row.person) || row.retreat_fear_at_least == Some(0))
            || self.weapons.par_iter().any(|row| !goods.contains(&row.good) || row.bonus == 0)
            || self.protection.par_iter().any(|row| !goods.contains(&row.good) || row.reduction == 0)
            || self.engagements.par_iter().any(|fight| {
                let Some(map) = self.map(fight.region) else { return true; };
                let positions: HashSet<_> = fight.people.iter().map(|row| (row.position.x, row.position.y)).collect();
                fight.id == 0 || fight.id >= self.next_id || fight.people.len() < 2
                    || positions.len() != fight.people.len()
                    || fight.next_millis < now
                    || fight.next_millis - now > u128::from(self.round_millis)
                    || fight.people.windows(2).any(|pair| pair[0].person >= pair[1].person)
                    || fight.people.iter().any(|row| {
                        !people.contains(&row.person) || !map.open(row.position)
                            || matches!(row.choice, Choice::Attack { target, .. } if target == row.person || fight.people.binary_search_by_key(&target, |person| person.person).is_err())
                            || matches!(row.choice, Choice::Move { destination } if !map.open(destination))
                            || matches!(row.choice, Choice::AttackSite { site } if self.structures.target(site, fight.region).is_none())
                    })
            })
        {
            return Err("invalid elapsed tactical state".into());
        }
        self.structures.validate(&self.maps, people)?;
        intervention::validate(&self.interventions, people)
    }

    pub fn map(&self, region: u64) -> Option<&Map> {
        self.maps
            .binary_search_by_key(&region, |map| map.region)
            .ok()
            .map(|index| &self.maps[index])
    }

    pub fn actors(&self) -> Vec<u64> {
        self.membership.par_iter().map(|row| row.0).collect()
    }

    fn index_membership(&self) -> Vec<(u64, u64)> {
        let mut people: Vec<_> = self
            .engagements
            .par_iter()
            .flat_map_iter(|fight| {
                fight
                    .people
                    .iter()
                    .map(|participant| (participant.person, fight.id))
            })
            .collect();
        people.par_sort_unstable();
        people
    }

    pub fn next_round(&self) -> Option<u128> {
        self.next_millis
    }

    pub fn prepare_starts(&self, starts: &[Start], now: u128) -> Result<Admissions, String> {
        let mut people: Vec<_> = starts
            .par_iter()
            .flat_map_iter(|row| [row.actor, row.target])
            .collect();
        people.par_sort_unstable();
        if people.par_windows(2).any(|pair| pair[0] == pair[1])
            || people.par_iter().any(|person| {
                self.membership
                    .binary_search_by_key(person, |row| row.0)
                    .is_ok()
            })
        {
            return Err("a person can join only one physical engagement".into());
        }
        let count = u64::try_from(starts.len()).map_err(|_| "tactical identity overflow")?;
        let next_id = self
            .next_id
            .checked_add(count)
            .ok_or("tactical identity overflow")?;
        let deadline = now
            .checked_add(u128::from(self.round_millis))
            .ok_or("tactical time overflow")?;
        let (fights, events) = starts
            .par_iter()
            .enumerate()
            .filter_map(|(index, request)| {
                self.prepare_start(*request, self.next_id + index as u64, now, deadline)
            })
            .unzip();
        Ok(Admissions {
            opening: self.next_id,
            next_id,
            fights,
            events,
        })
    }

    fn prepare_start(
        &self,
        request: Start,
        id: u64,
        now: u128,
        deadline: u128,
    ) -> Option<(Engagement, Event)> {
        let Start {
            actor,
            target,
            region,
        } = request;
        let map = self.map(region)?;
        let radius = map.rules.movement.min(map.rules.attack_range).clamp(1, 16);
        let center = Point {
            x: map.rules.width / 2,
            y: map.rules.height / 2,
        };
        let mut open = (0..32)
            .flat_map(|x| {
                (0..32).map(move |y| Point {
                    x: center.x.saturating_sub(radius).saturating_add(x),
                    y: center.y.saturating_sub(radius).saturating_add(y),
                })
            })
            .filter(|point| map.open(*point));
        let first = open.next()?;
        let second = open.find(|point| map.visible(first, *point))?;
        let mut people = vec![
            Participant {
                person: actor,
                position: first,
                choice: Choice::Attack {
                    target,
                    pursue: true,
                },
            },
            Participant {
                person: target,
                position: second,
                choice: Choice::Wait,
            },
        ];
        people.sort_unstable_by_key(|row| row.person);
        let fight = Engagement {
            id,
            region,
            sequence: 0,
            next_millis: deadline,
            people,
        };
        let mut event = fight.event(now, Some(actor), EventKind::Started);
        event.target = Some(target);
        Some((fight, event))
    }

    pub fn publish_starts(&mut self, mut admissions: Admissions) -> Result<Vec<Event>, String> {
        if self.next_id != admissions.opening {
            return Err("stale tactical admissions".into());
        }
        if !admissions.events.is_empty() {
            self.engagements.append(&mut admissions.fights);
            self.membership = self.index_membership();
            self.next_millis = self
                .engagements
                .par_iter()
                .map(|fight| fight.next_millis)
                .min();
            self.next_id = admissions.next_id;
        }
        Ok(admissions.events)
    }

    pub fn engagement(&self, person: u64) -> Option<&Engagement> {
        let member = self
            .membership
            .binary_search_by_key(&person, |row| row.0)
            .ok()?;
        let engagement = self.membership[member].1;
        self.engagements
            .binary_search_by_key(&engagement, |row| row.id)
            .ok()
            .map(|index| &self.engagements[index])
    }

    pub fn choose(&mut self, person: u64, choice: Choice) -> Result<(), String> {
        let fight = self
            .engagements
            .iter_mut()
            .find(|fight| {
                fight
                    .people
                    .binary_search_by_key(&person, |row| row.person)
                    .is_ok()
            })
            .ok_or("person has no active tactical engagement")?;
        if let Choice::Attack { target, .. } = choice
            && (target == person
                || fight
                    .people
                    .binary_search_by_key(&target, |row| row.person)
                    .is_err())
        {
            return Err("target is outside this engagement".into());
        }
        if let Choice::Move { destination } = choice {
            let map = &self.maps[self
                .maps
                .binary_search_by_key(&fight.region, |row| row.region)
                .expect("engagement map")];
            if !map.open(destination) {
                return Err("destination is outside open tactical space".into());
            }
        }
        let position = fight
            .people
            .binary_search_by_key(&person, |row| row.person)
            .expect("engaged person");
        if let Choice::AttackSite { site } = choice {
            let target = self
                .structures
                .target(site, fight.region)
                .ok_or("unknown local tactical site")?;
            let map = &self.maps[self
                .maps
                .binary_search_by_key(&fight.region, |row| row.region)
                .expect("engagement map")];
            if !map.visible(fight.people[position].position, target.position) {
                return Err("the site is not visible".into());
            }
        }
        fight.people[position].choice = choice;
        Ok(())
    }

    pub fn prepare_rounds(
        &self,
        now: u128,
        actors: &[Actor],
        seed: u64,
        feelings: &(dyn Fn(u64, u64) -> (u64, u64) + Sync),
    ) -> Result<Round, String> {
        let rows: Vec<_> = self
            .engagements
            .par_iter()
            .filter(|day| day.next_millis == now)
            .map(|fight| prepare_one(fight, self, actors, now, seed, feelings))
            .collect::<Result<_, _>>()?;
        let mut round = Round {
            at_millis: now,
            changes: Vec::new(),
            events: Vec::new(),
            hits: Vec::new(),
            site_strikes: Vec::new(),
            protection: Vec::new(),
        };
        for (fight, mut results) in rows {
            round.changes.push(fight);
            round.events.append(&mut results.events);
            round.hits.append(&mut results.hits);
            round.site_strikes.append(&mut results.site_strikes);
            round.protection.append(&mut results.protection);
        }
        round.protection.par_sort_unstable_by_key(|row| row.0);
        Ok(round)
    }

    pub fn publish_rounds(&mut self, mut round: Round, unable: &[u64]) -> Vec<Event> {
        let at = round.at_millis;
        let changed_membership = !unable.is_empty()
            || round
                .events
                .par_iter()
                .any(|event| matches!(event.kind, EventKind::Incapacitated | EventKind::Withdrew));
        for fight in &mut round.changes {
            let fallen: Vec<_> = fight
                .people
                .iter()
                .filter(|row| unable.binary_search(&row.person).is_ok())
                .map(|row| fight.event(at, Some(row.person), EventKind::Incapacitated))
                .collect();
            round.events.extend(fallen);
            fight
                .people
                .retain(|row| unable.binary_search(&row.person).is_err());
            let mut remaining: Vec<_> = fight.people.iter().map(|row| row.person).collect();
            remaining.sort_unstable();
            for row in &mut fight.people {
                if matches!(row.choice, Choice::Attack { target, .. } if remaining.binary_search(&target).is_err())
                {
                    row.choice = Choice::Wait;
                }
            }
            if fight.people.len() < 2 {
                round.events.push(fight.event(at, None, EventKind::Ended));
            }
        }
        for next in round.changes {
            let index = self
                .engagements
                .binary_search_by_key(&next.id, |row| row.id)
                .expect("staged engagement");
            self.engagements[index] = next;
        }
        self.engagements.retain(|fight| fight.people.len() >= 2);
        if changed_membership {
            self.membership = self.index_membership();
        }
        self.next_millis = self
            .engagements
            .par_iter()
            .map(|fight| fight.next_millis)
            .min();
        round.events
    }

    pub fn remove_structures(&mut self, destroyed: &HashSet<u64>) {
        self.structures.remove(destroyed, &mut self.maps);
        self.engagements.par_iter_mut().for_each(|fight| {
            for row in &mut fight.people {
                if matches!(row.choice, Choice::AttackSite { site } if destroyed.contains(&site)) {
                    row.choice = Choice::Wait;
                }
            }
        });
    }

    /// Arbitrate real map positions within each active engagement, then publish
    /// only successful entries. Reinforcements first act at its next round.
    pub fn reinforce(
        &mut self,
        requests: &[elapsed_reinforcements::Request],
        now: u128,
        seed: u64,
    ) -> Result<Vec<Event>, String> {
        let mut actors: Vec<_> = requests.par_iter().map(|request| request.person).collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("a person can attempt one tactical entry at a time".into());
        }
        let mut admissions = elapsed_reinforcements::prepare(self, requests, seed, now);
        admissions.par_sort_unstable_by_key(|row| (row.engagement, row.person));
        let events = admissions
            .par_iter()
            .map(|admission| {
                let index = self
                    .engagements
                    .binary_search_by_key(&admission.engagement, |row| row.id)
                    .expect("staged engagement");
                let fight = &self.engagements[index];
                let mut event = fight.event(now, Some(admission.person), EventKind::EntryBlocked);
                event.target = Some(admission.opponent);
                if let Some(participant) = admission.placement {
                    event.kind = EventKind::Entered;
                    event.position = Some(participant.position);
                }
                event
            })
            .collect();
        if admissions.par_iter().any(|row| row.placement.is_some()) {
            self.engagements.par_iter_mut().for_each(|fight| {
                let start = admissions.partition_point(|row| row.engagement < fight.id);
                let end = admissions.partition_point(|row| row.engagement <= fight.id);
                if start != end {
                    fight.people.extend(
                        admissions[start..end]
                            .iter()
                            .filter_map(|row| row.placement),
                    );
                    fight.people.par_sort_unstable_by_key(|row| row.person);
                }
            });
            self.membership = self.index_membership();
        }
        Ok(events)
    }
}

#[derive(Default)]
struct Results {
    events: Vec<Event>,
    hits: Vec<Hit>,
    site_strikes: Vec<SiteStrike>,
    protection: Vec<(u64, u32)>,
}

fn distance(from: Point, to: Point) -> u128 {
    u128::from(from.x.abs_diff(to.x)).pow(2) + u128::from(from.y.abs_diff(to.y)).pow(2)
}

fn step_toward(map: &Map, from: Point, target: Point) -> Option<Point> {
    let m = map.rules.movement;
    let candidates = [m, m.div_ceil(2), 1].into_iter().flat_map(|step| {
        let left = from.x.saturating_sub(step);
        let right = from.x.saturating_add(step).min(map.rules.width);
        let down = from.y.saturating_sub(step);
        let up = from.y.saturating_add(step).min(map.rules.height);
        [left, from.x, right]
            .into_iter()
            .flat_map(move |x| [down, from.y, up].into_iter().map(move |y| Point { x, y }))
    });
    candidates
        .filter(|point| {
            *point != from
                && distance(*point, target) < distance(from, target)
                && map.can_move(from, *point)
        })
        .min_by_key(|point| (distance(*point, target), point.x, point.y))
}

fn exit(map: &Map, point: Point) -> Point {
    [
        Point { x: 0, y: point.y },
        Point {
            x: map.rules.width,
            y: point.y,
        },
        Point { x: point.x, y: 0 },
        Point {
            x: point.x,
            y: map.rules.height,
        },
    ]
    .into_iter()
    .min_by_key(|&edge| distance(edge, point))
    .expect("map edges")
}

fn movement_priority(seed: u64, id: u64, sequence: u64, actor: u64) -> u64 {
    let mut value = seed ^ id.rotate_left(13) ^ sequence.rotate_left(29) ^ actor;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn prepare_one(
    fight: &Engagement,
    owner: &Tactical,
    actors: &[Actor],
    now: u128,
    seed: u64,
    feelings: &(dyn Fn(u64, u64) -> (u64, u64) + Sync),
) -> Result<(Engagement, Results), String> {
    let map = owner.map(fight.region).ok_or("no tactical map here")?;
    let mut results = Results::default();
    let mut next = fight.clone();
    next.next_millis = fight
        .next_millis
        .checked_add(u128::from(owner.round_millis))
        .ok_or("tactical time overflow")?;
    let find = |id| {
        actors
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|index| &actors[index])
    };
    next.people.retain(|row| {
        let present = find(row.person)
            .is_some_and(|person| person.health > 0 && person.region == Some(fight.region));
        if !present {
            results
                .events
                .push(fight.event(now, Some(row.person), EventKind::Incapacitated));
        }
        present
    });
    let opening: HashSet<_> = next
        .people
        .iter()
        .map(|row| (row.position.x, row.position.y))
        .collect();
    let mut incoming: Vec<_> = next
        .people
        .iter()
        .filter_map(|person| {
            if let Choice::Attack { target, .. } = person.choice {
                Some((target, person.person))
            } else {
                None
            }
        })
        .collect();
    incoming.sort_unstable();
    let mut decisions = Vec::with_capacity(next.people.len());
    for row in &next.people {
        let actor = find(row.person).expect("present fighter");
        let target = match row.choice {
            Choice::Attack { target, .. } => Some(target),
            _ => {
                let first = incoming.partition_point(|&(target, _)| target < row.person);
                incoming
                    .get(first)
                    .and_then(|&(target, attacker)| (target == row.person).then_some(attacker))
            }
        };
        let opponent = target.and_then(|target| {
            let index = next
                .people
                .binary_search_by_key(&target, |person| person.person)
                .ok()?;
            next.people.get(index)
        });
        let preference = owner
            .reactions
            .binary_search_by_key(&row.person, |row| row.person)
            .ok()
            .map(|i| &owner.reactions[i]);
        let choice = if !actor.can_act {
            Choice::Wait
        } else if !actor.controlled
            && let Some(preference) = preference
        {
            if preference
                .retreat_at_or_below
                .is_some_and(|health| actor.health <= health)
                || opponent.is_some_and(|other| {
                    preference
                        .retreat_fear_at_least
                        .is_some_and(|threshold| feelings(row.person, other.person).0 >= threshold)
                })
            {
                Choice::Retreat
            } else {
                match preference.response {
                    Response::Wait => Choice::Wait,
                    Response::Fight => opponent.map_or(row.choice, |other| Choice::Attack {
                        target: other.person,
                        pursue: preference.pursue,
                    }),
                    Response::Defend => Choice::Defend,
                    Response::Retreat => Choice::Retreat,
                }
            }
        } else {
            row.choice
        };
        let choice = if actor.can_act
            && !actor.controlled
            && choice != Choice::Retreat
            && let Some(site) = owner
                .structures
                .chosen(row.person, row.position, map, feelings)
        {
            Choice::AttackSite { site }
        } else {
            choice
        };
        decisions.push((choice, opponent.map(|row| row.position)));
    }
    let mut moves: Vec<_> = next
        .people
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let (choice, opponent) = decisions[index];
            let destination = match choice {
                Choice::Move { destination } => Some(destination),
                Choice::Retreat => step_toward(map, row.position, exit(map, row.position)),
                Choice::Attack { pursue: true, .. } => opponent
                    .filter(|&other| !map.can_attack(row.position, other))
                    .and_then(|target| step_toward(map, row.position, target)),
                _ => None,
            }?;
            Some((
                (destination.x, destination.y),
                movement_priority(seed, fight.id, fight.sequence, row.person),
                row.person,
                index,
            ))
        })
        .collect();
    moves.sort_unstable();
    let mut last = None;
    let mut moved = HashSet::new();
    for (point, _, id, index) in moves {
        let destination = Point {
            x: point.0,
            y: point.1,
        };
        let accepted = last != Some(point)
            && !opening.contains(&point)
            && map.can_move(next.people[index].position, destination);
        let mut event = fight.event(
            now,
            Some(id),
            if accepted {
                EventKind::Moved
            } else {
                EventKind::MovementBlocked
            },
        );
        event.position = Some(destination);
        results.events.push(event);
        if accepted {
            last = Some(point);
            next.people[index].position = destination;
            moved.insert(id);
        }
    }
    let mut withdrawn = HashSet::new();
    for (row, &(choice, _)) in next.people.iter().zip(&decisions) {
        let actor = find(row.person).expect("present fighter");
        let guarding = choice == Choice::Defend;
        if guarding || actor.armor != 0 {
            let reduction = if guarding {
                owner.base_force / 2 + owner.base_force % 2
            } else {
                0
            };
            results
                .protection
                .push((row.person, actor.armor.saturating_add(reduction)));
        }
        if guarding {
            results
                .events
                .push(fight.event(now, Some(row.person), EventKind::Defended));
        }
        if choice == Choice::Retreat && exit(map, row.position) == row.position {
            withdrawn.insert(row.person);
            results
                .events
                .push(fight.event(now, Some(row.person), EventKind::Withdrew));
        }
    }
    for (row, &(choice, _)) in next.people.iter_mut().zip(&decisions) {
        if matches!(row.choice, Choice::Move { .. }) {
            row.choice = Choice::Wait;
        }
        if let Choice::Attack { target, .. } = choice
            && !moved.contains(&row.person)
            && !withdrawn.contains(&target)
        {
            let mut event = fight.event(now, Some(row.person), EventKind::Missed);
            event.target = Some(target);
            results.events.push(event);
        }
        if let Choice::AttackSite { site } = choice
            && let Some(target) = owner.structures.target(site, fight.region)
        {
            let harm = find(row.person).expect("present fighter").force;
            if harm > 0 && map.can_attack(row.position, target.position) {
                results.site_strikes.push(SiteStrike {
                    at_millis: now,
                    engagement: fight.id,
                    sequence: fight.sequence,
                    actor: row.person,
                    site,
                    harm,
                    collapsed: false,
                });
            }
        }
    }
    let attack_index: Vec<_> = results
        .events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            (event.kind == EventKind::Missed).then_some((i, event.person?, event.target?))
        })
        .collect();
    for (index, actor, target) in attack_index {
        let force = find(actor).expect("present fighter").force;
        let from = next
            .people
            .binary_search_by_key(&actor, |row| row.person)
            .ok()
            .map(|i| next.people[i].position);
        let to = next
            .people
            .binary_search_by_key(&target, |row| row.person)
            .ok()
            .map(|i| next.people[i].position);
        if force > 0
            && from
                .zip(to)
                .is_some_and(|(from, to)| map.can_attack(from, to))
        {
            results.events[index].kind = EventKind::Attacked;
            results.hits.push(Hit {
                actor,
                target,
                force,
            });
        }
    }
    next.people.retain(|row| !withdrawn.contains(&row.person));
    next.sequence = fight
        .sequence
        .checked_add(1)
        .ok_or("tactical round overflow")?;
    Ok((next, results))
}
