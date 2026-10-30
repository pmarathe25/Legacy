use super::*;
use game_parallel_genetics::inherit;
use game_parallel_matching::resolve;
use rayon::prelude::*;
use std::collections::BTreeSet;

pub struct Draft {
    pub families: Families,
    pub births: Vec<Birth>,
    pub deaths: BTreeSet<u64>,
}
fn event(tick: u64, kind: &str, people: Vec<u64>, region: Option<u64>) -> FamilyEvent {
    FamilyEvent {
        tick,
        kind: kind.into(),
        people,
        region,
    }
}
fn eligible(m: &Member, p: &LifeInput, rules: &FamilyRules) -> bool {
    m.alive
        && m.reproduction_allowed
        && m.age >= rules.adult_age
        && m.age < rules.fertile_until
        && p.region.is_some()
        && p.health >= rules.minimum_health
        && p.hunger <= rules.maximum_hunger
}
// A valid pedigree has at most two parents and four grandparents. Repeated
// ancestors need not be deduplicated for the intersection test.
struct Ancestors {
    ids: [u64; 7],
    len: usize,
}
impl Ancestors {
    fn is_disjoint(&self, other: &Self) -> bool {
        self.ids[..self.len]
            .iter()
            .all(|id| !other.ids[..other.len].contains(id))
    }
}
fn ancestors(id: u64, members: &MemberIndex<'_>) -> Ancestors {
    let mut all = Ancestors {
        ids: [0; 7],
        len: 1,
    };
    all.ids[0] = id;
    for parent in &members[&id].parents {
        all.ids[all.len] = *parent;
        all.len += 1;
        for grandparent in &members[parent].parents {
            all.ids[all.len] = *grandparent;
            all.len += 1;
        }
    }
    all
}
impl Families {
    /// One immutable read phase, local matching, canonical IDs, then disjoint inheritance.
    pub fn advance(
        &self,
        inputs: &[LifeInput],
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("family phase must advance one tick");
        }
        let physical = self.validate_with_index(inputs, self.last_tick)?;
        self.advance_validated(&physical, seed, tick)
    }
    /// For a private host with validated family and physical identities.
    pub fn advance_validated(
        &self,
        physical: &impl game_parallel_family_model::PhysicalView,
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        self.advance_validated_with_penalty(physical, seed, tick, None)
    }
    /// Advance a validated private generation with an optional lifespan cost.
    pub fn advance_validated_with_penalty(
        &self,
        physical: &impl game_parallel_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        penalty: Option<(u64, u64)>,
    ) -> Result<Draft, &'static str> {
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("family phase must advance one tick");
        }
        let rules = &self.rules;
        let chunks: Vec<Result<_, &'static str>> = self
            .members
            .par_chunks(1024)
            .map(|chunk| {
                let mut members = Vec::with_capacity(chunk.len());
                let mut events = Vec::new();
                for old in chunk {
                    let mut m = old.clone();
                    if m.alive {
                        if let Some((person, amount)) = penalty
                            && m.id == person
                        {
                            m.lifespan_loss = m
                                .lifespan_loss
                                .checked_add(amount)
                                .ok_or("lifespan penalty overflow")?;
                        }
                        m.age = m.age.checked_add(1).ok_or("age overflow")?;
                        let lifespan = rules.lifespan_min
                            + ((u128::from(rules.lifespan_max - rules.lifespan_min)
                                * u128::from(m.genome.expressed(3)))
                                / 1000) as u64;
                        let lifespan = lifespan.saturating_sub(m.lifespan_loss);
                        let person = physical.read(m.id);
                        if person.health == 0 || m.age >= lifespan {
                            m.alive = false;
                            events.push(event(tick, "death", vec![m.id], person.region));
                            if let Some(p) = m.pregnancy.take() {
                                events.push(event(
                                    tick,
                                    "pregnancy_loss",
                                    vec![m.id, p.father],
                                    person.region,
                                ));
                            }
                        } else if m.age == rules.adult_age {
                            events.push(event(tick, "adulthood", vec![m.id], person.region));
                        }
                    }
                    members.push(m);
                }
                Ok((members, events))
            })
            .collect();
        let (member_chunks, event_chunks): (Vec<_>, Vec<_>) = chunks
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .unzip();
        let mut members: Vec<_> = member_chunks.into_par_iter().flatten().collect();
        let mut events: Vec<_> = event_chunks.into_par_iter().flatten().collect();
        if members.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            members.par_sort_unstable_by_key(|m| m.id);
        }
        let alive: Vec<_> = members
            .par_iter()
            .filter(|m| m.alive)
            .map(|m| m.id)
            .collect();
        let dense_alive = alive.first().and_then(|first| {
            alive.last().and_then(|last| {
                (last.checked_sub(*first) == Some((alive.len() - 1) as u64))
                    .then_some((*first, *last))
            })
        });
        members.par_iter_mut().for_each(|m| {
            if !m.alive
                || m.partner.is_some_and(|p| {
                    dense_alive.map_or_else(
                        || alive.binary_search(&p).is_err(),
                        |(first, last)| p < first || p > last,
                    )
                })
            {
                m.partner = None;
            }
        });
        game_parallel_family_index::reconsider(&mut members, physical, rules.fertile_until)?;
        let index = MemberIndex::new(&members)?;
        let mut local: Vec<_> = members
            .par_iter()
            .filter_map(|m| {
                let person = physical.read(m.id);
                (m.partner.is_none() && !physical.controlled(m.id) && eligible(m, &person, rules))
                    .then(|| (person.region.unwrap(), m))
            })
            .collect();
        local.par_sort_unstable_by_key(|(region, member)| (*region, member.id));
        let matches: Vec<Vec<_>> = local
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let area = group[0].0;
                let mut females: Vec<_> = group
                    .par_iter()
                    .map(|(_, member)| *member)
                    .filter(|m| m.genome.sex == Sex::Female)
                    .collect();
                let mut males: Vec<_> = group
                    .par_iter()
                    .map(|(_, member)| *member)
                    .filter(|m| m.genome.sex == Sex::Male)
                    .collect();
                females.par_sort_unstable_by_key(|m| (random(seed, m.id, tick), m.id));
                males.par_sort_unstable_by_key(|m| (random(seed ^ 31, m.id, tick), m.id));
                let kin: Vec<_> = males.par_iter().map(|m| ancestors(m.id, &index)).collect();
                let proposed: Vec<_> = females
                    .par_iter()
                    .filter_map(|mother| {
                        if males.is_empty() {
                            return None;
                        }
                        let ancestors = ancestors(mother.id, &index);
                        let offset =
                            (random(seed ^ 71, mother.id, tick) % males.len() as u64) as usize;
                        (0..males.len().min(32))
                            .map(|n| (offset + n) % males.len())
                            .find(|&candidate| ancestors.is_disjoint(&kin[candidate]))
                            .map(|candidate| (mother.id, males[candidate].id, area))
                    })
                    .collect();
                resolve(proposed, seed, tick)
            })
            .collect();
        let winners: Vec<_> = matches.into_par_iter().flatten().collect();
        let mut partners: Vec<_> = winners
            .par_iter()
            .flat_map_iter(|&(a, b, _)| [(a, b), (b, a)])
            .collect();
        partners.par_sort_unstable_by_key(|row| row.0);
        events.par_extend(
            winners
                .par_iter()
                .map(|&(a, b, area)| event(tick, "partnership", vec![a, b], Some(area))),
        );
        if !partners.is_empty() {
            members.par_chunks_mut(1024).for_each(|chunk| {
                let mut index = partners.partition_point(|row| row.0 < chunk[0].id);
                for member in chunk {
                    while index < partners.len() && partners[index].0 < member.id {
                        index += 1;
                    }
                    if partners.get(index).is_some_and(|row| row.0 == member.id) {
                        member.partner = Some(partners[index].1);
                    }
                }
            });
        }
        let index = MemberIndex::new(&members)?;
        let due: Vec<_> = members
            .par_iter()
            .filter(|m| m.alive && m.pregnancy.as_ref().is_some_and(|p| p.due <= tick))
            .collect();
        let next_id = self
            .next_id
            .checked_add(due.len() as u64)
            .ok_or("birth identity overflow")?;
        let children: Vec<Result<_, &'static str>> = due
            .par_iter()
            .enumerate()
            .map(|(offset, mother)| {
                let father = &index[&mother.pregnancy.as_ref().unwrap().father];
                let id = self.next_id + offset as u64;
                let genome = inherit(
                    &mother.genome,
                    &father.genome,
                    seed,
                    mother.id,
                    tick,
                    rules.mutation_per_million,
                    rules.mutation_step,
                );
                let child = Member {
                    id,
                    born: Some(tick),
                    age: 0,
                    lifespan_loss: 0,
                    genome,
                    parents: vec![mother.id, father.id].into(),
                    partner: None,
                    pregnancy: None,
                    next_conception: 0,
                    alive: true,
                    reproduction_allowed: true,
                    reconsider_partnership: mother.reconsider_partnership,
                };
                let recover = tick
                    .checked_add(rules.birth_spacing)
                    .ok_or("birth spacing overflow")?;
                Ok((
                    child,
                    Birth {
                        id,
                        mother: mother.id,
                        region: physical.read(mother.id).region,
                        health: rules.newborn_health,
                    },
                    recover,
                ))
            })
            .collect();
        let children: Vec<_> = children.into_par_iter().collect::<Result<_, _>>()?;
        events.par_extend(children.par_iter().map(|(child, birth, _)| {
            event(
                tick,
                "birth",
                vec![child.id, child.parents[0], child.parents[1]],
                birth.region,
            )
        }));
        let mut recovered: Vec<_> = children
            .par_iter()
            .map(|(_, birth, recover)| (birth.mother, *recover))
            .collect();
        if recovered.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            recovered.par_sort_unstable_by_key(|row| row.0);
        }
        let (newborns, births): (Vec<_>, Vec<_>) = children
            .into_par_iter()
            .map(|(child, birth, _)| (child, birth))
            .unzip();
        let conceptions: Vec<_> = members
            .par_iter()
            .filter_map(|mother| {
                if mother.genome.sex != Sex::Female
                    || physical.controlled(mother.id)
                    || mother.pregnancy.is_some()
                    || mother.next_conception > tick
                    || !eligible(mother, &physical.read(mother.id), rules)
                {
                    return None;
                }
                let father = mother.partner.and_then(|p| index.get(&p))?;
                let chance = 500 + u64::from(mother.genome.expressed(0));
                if father.genome.sex != Sex::Male
                    || physical.controlled(father.id)
                    || !ancestors(mother.id, &index).is_disjoint(&ancestors(father.id, &index))
                    || !eligible(father, &physical.read(father.id), rules)
                    || physical.read(father.id).region != physical.read(mother.id).region
                    || !random(seed ^ 97, mother.id, tick).is_multiple_of(rules.conception_one_in)
                    || random(seed ^ 107, mother.id, tick) % 1500 >= chance
                {
                    return None;
                }
                Some(
                    tick.checked_add(rules.gestation)
                        .ok_or("gestation overflow")
                        .map(|due| {
                            (
                                mother.id,
                                Pregnancy {
                                    father: father.id,
                                    due,
                                },
                            )
                        }),
                )
            })
            .collect::<Result<_, _>>()?;
        events.par_extend(conceptions.par_iter().map(|(mother, pregnancy)| {
            event(
                tick,
                "conception",
                vec![*mother, pregnancy.father],
                physical.read(*mother).region,
            )
        }));
        let mut conceived = conceptions;
        if conceived.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            conceived.par_sort_unstable_by_key(|row| row.0);
        }
        members.par_chunks_mut(1024).for_each(|chunk| {
            let mut recovery_index = recovered.partition_point(|row| row.0 < chunk[0].id);
            let mut conception_index = conceived.partition_point(|row| row.0 < chunk[0].id);
            for member in chunk {
                while recovery_index < recovered.len() && recovered[recovery_index].0 < member.id {
                    recovery_index += 1;
                }
                while conception_index < conceived.len()
                    && conceived[conception_index].0 < member.id
                {
                    conception_index += 1;
                }
                if recovered
                    .get(recovery_index)
                    .is_some_and(|row| row.0 == member.id)
                {
                    member.pregnancy = None;
                    member.next_conception = recovered[recovery_index].1;
                }
                if conceived
                    .get(conception_index)
                    .is_some_and(|row| row.0 == member.id)
                {
                    member.pregnancy = Some(conceived[conception_index].1.clone());
                }
            }
        });
        let deaths = events
            .iter()
            .filter(|e| e.kind == "death")
            .map(|e| e.people[0])
            .collect();
        members.extend(newborns);
        events.par_sort_unstable_by(|a, b| (&a.kind, &a.people).cmp(&(&b.kind, &b.people)));
        let counts = events
            .par_iter()
            .fold(
                || [0u64; 6],
                |mut counts, event| {
                    let i = match event.kind.as_str() {
                        "birth" => 0,
                        "death" => 1,
                        "conception" => 2,
                        "pregnancy_loss" => 3,
                        "adulthood" => 4,
                        _ => 5,
                    };
                    counts[i] += 1;
                    counts
                },
            )
            .reduce(
                || [0u64; 6],
                |mut left, right| {
                    for (a, b) in left.iter_mut().zip(right) {
                        *a += b;
                    }
                    left
                },
            );
        let mut totals = self.totals.clone();
        for (total, count) in [
            &mut totals.births,
            &mut totals.deaths,
            &mut totals.conceptions,
            &mut totals.pregnancy_losses,
            &mut totals.adulthoods,
            &mut totals.partnerships,
        ]
        .into_iter()
        .zip(counts)
        {
            *total = total.checked_add(count).ok_or("family counter overflow")?;
        }
        Ok(Draft {
            families: Families {
                rules: rules.clone(),
                members,
                next_id,
                last_tick: tick,
                totals,
                events,
            },
            births,
            deaths,
        })
    }
}
