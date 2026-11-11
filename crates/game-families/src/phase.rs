use super::*;
use game_family_index::OrderedMemberIndex;
use game_genetics::inherit;
use game_matching::resolve;
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
fn seeks_mate(m: &Member, p: &LifeInput, dependents: u64, seed: u64, tick: u64) -> bool {
    let pref = m.mate_preference.unwrap_or_default();
    let genetic = (i128::from(m.genome.expressed_mate_desire()) - 500)
        * i128::from(pref.genetic_weight)
        / 500;
    let desire = i128::from(pref.desire)
        + genetic
        + i128::from(pref.hunger_weight) * i128::from(p.hunger)
        + i128::from(pref.dependent_weight) * i128::from(dependents);
    (random(seed ^ 0x4d415445, m.id, tick) % 1000) < desire.clamp(0, 1000) as u64
}
fn mate_score(
    observer: &Member,
    target: &Member,
    physical: &impl game_family_model::PhysicalView,
    seed: u64,
) -> i64 {
    let pref = observer.mate_preference.unwrap_or_default();
    let attraction = (random(seed ^ observer.id ^ 0x41545452, target.id, 0) % 2001) as i64 - 1000;
    i64::from(pref.affinity_weight) * i64::from(physical.affinity(observer.id, target.id))
        + i64::from(pref.attraction_weight) * attraction / 1000
}
fn accepts_mate(
    observer: &Member,
    target: &Member,
    physical: &impl game_family_model::PhysicalView,
    seed: u64,
) -> bool {
    mate_score(observer, target, physical, seed)
        >= i64::from(observer.mate_preference.as_ref().map_or(0, |p| p.minimum))
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
fn ancestors(id: u64, members: &OrderedMemberIndex<'_>) -> Ancestors {
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
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        self.advance_validated_with_penalty(physical, seed, tick, None)
    }
    /// Advance a validated private generation with an optional lifespan cost.
    pub fn advance_validated_with_penalty(
        &self,
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        penalty: Option<(u64, u64)>,
    ) -> Result<Draft, &'static str> {
        #[cfg(feature = "profile")]
        let mut profile_phase = game_diagnostics::span("family.life");
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("family phase must advance one tick");
        }
        let rules = &self.rules;
        // Clone directly into the staged generation, avoiding per-chunk member
        // buffers and a second population-sized flattening pass.
        let mut members: Vec<_> = self.members.par_iter().cloned().collect();
        let chunks: Vec<Result<_, &'static str>> = members
            .par_chunks_mut(1024)
            .map(|chunk| {
                let mut events = Vec::new();
                for m in chunk {
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
                }
                Ok(events)
            })
            .collect();
        let event_chunks = chunks.into_iter().collect::<Result<Vec<_>, _>>()?;
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
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.reconsider");
        }
        game_family_index::reconsider(&mut members, physical, rules.fertile_until)?;
        let index = OrderedMemberIndex::new(&members);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.matching");
        }
        let mut local: Vec<_> = members
            .par_iter()
            .filter_map(|m| {
                let person = physical.read(m.id);
                (!rules.subjective_mating
                    && m.partner.is_none()
                    && !physical.controlled(m.id)
                    && eligible(m, &person, rules))
                .then(|| (person.region.unwrap(), physical.contact_partition(m.id), m))
            })
            .collect();
        local.par_sort_unstable_by_key(|(region, partition, member)| {
            (*region, *partition, member.id)
        });
        let matches: Vec<Vec<_>> = local
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| {
                let area = group[0].0;
                let females: Vec<_> = group
                    .par_iter()
                    .map(|(_, _, member)| *member)
                    .filter(|m| m.genome.sex == Sex::Female)
                    .collect();
                let mut males: Vec<_> = group
                    .par_iter()
                    .map(|(_, _, member)| *member)
                    .filter(|m| m.genome.sex == Sex::Male)
                    .collect();
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
        let index = OrderedMemberIndex::new(&members);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.births");
        }
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
                    mate_preference: None,
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
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.conceptions");
        }
        let mut parents: Vec<_> = if rules.subjective_mating {
            members
                .par_iter()
                .filter(|child| child.alive && child.age < rules.adult_age)
                .flat_map_iter(|child| child.parents.iter().copied())
                .collect()
        } else {
            Vec::new()
        };
        parents.par_sort_unstable();
        let dependents: Vec<_> = parents
            .par_chunk_by(|a, b| a == b)
            .map(|group| (group[0], group.len() as u64))
            .collect();
        let dependent_count = |id| {
            dependents
                .binary_search_by_key(&id, |row| row.0)
                .ok()
                .map_or(0, |index| dependents[index].1)
        };
        let mut prospective_fathers: Vec<_> = if rules.subjective_mating {
            members
                .par_iter()
                .filter_map(|m| {
                    let person = physical.read(m.id);
                    (m.genome.sex == Sex::Male
                        && !physical.controlled(m.id)
                        && eligible(m, &person, rules)
                        && seeks_mate(m, &person, dependent_count(m.id), seed, tick))
                    .then(|| (person.region.unwrap(), physical.contact_partition(m.id), m))
                })
                .collect()
        } else {
            Vec::new()
        };
        prospective_fathers.par_sort_unstable_by_key(|(region, partition, father)| {
            (
                *region,
                *partition,
                random(seed ^ 0x46415448, father.id, tick),
                father.id,
            )
        });
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
                let mother_person = physical.read(mother.id);
                let father = if rules.subjective_mating {
                    if !seeks_mate(
                        mother,
                        &mother_person,
                        dependent_count(mother.id),
                        seed,
                        tick,
                    ) {
                        return None;
                    }
                    let area = (mother_person.region?, physical.contact_partition(mother.id));
                    let first = prospective_fathers.partition_point(|row| (row.0, row.1) < area);
                    let end = prospective_fathers.partition_point(|row| (row.0, row.1) <= area);
                    if first == end {
                        return None;
                    }
                    let offset = (random(seed ^ 0x5345454b, mother.id, tick) % (end - first) as u64)
                        as usize;
                    let mother_ancestors = ancestors(mother.id, &index);
                    (0..(end - first).min(32))
                        .map(|n| prospective_fathers[first + (offset + n) % (end - first)].2)
                        .filter(|father| {
                            mother_ancestors.is_disjoint(&ancestors(father.id, &index))
                                && accepts_mate(mother, father, physical, seed)
                                && accepts_mate(father, mother, physical, seed)
                        })
                        .max_by_key(|father| {
                            (
                                mate_score(mother, father, physical, seed),
                                std::cmp::Reverse(father.id),
                            )
                        })?
                } else {
                    mother.partner.and_then(|p| index.get(&p))?
                };
                let chance = 500 + u64::from(mother.genome.expressed(0));
                if father.genome.sex != Sex::Male
                    || physical.controlled(father.id)
                    || !ancestors(mother.id, &index).is_disjoint(&ancestors(father.id, &index))
                    || !eligible(father, &physical.read(father.id), rules)
                    || physical.read(father.id).region != physical.read(mother.id).region
                    || physical.contact_partition(father.id)
                        != physical.contact_partition(mother.id)
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
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.finish");
        }
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
        #[cfg(feature = "profile")]
        drop(profile_phase);
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
