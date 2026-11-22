use super::*;
use game_family_index::OrderedMemberIndex;
use game_genetics::Inheritance;
use game_matching::resolve;
use rayon::prelude::*;
use std::collections::BTreeSet;
mod relationships;

pub struct Draft {
    pub families: Families,
    pub births: Vec<Birth>,
    pub deaths: BTreeSet<u64>,
}
pub struct ElapsedFamilyDraft {
    pub families: Families,
    pub births: Vec<Birth>,
    pub events: Vec<FamilyEvent>,
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
    eligible_at_age(m, p, rules, m.age)
}
fn eligible_at_age(m: &Member, p: &LifeInput, rules: &FamilyRules, age: u64) -> bool {
    m.alive
        && m.reproduction_allowed
        && age >= rules.adult_age
        && age < rules.fertile_until
        && p.region.is_some()
        && p.health >= rules.minimum_health
        && p.hunger <= rules.maximum_hunger
}
fn eligible_for_partnership(m: &Member, p: &LifeInput, rules: &FamilyRules) -> bool {
    m.alive && m.age >= rules.adult_age && p.health > 0 && p.region.is_some()
}
fn seeks_mate(m: &Member, p: &LifeInput, dependents: u64, seed: u64, tick: u64, traits: Option<&TraitCatalog>) -> bool {
    let pref = m.mate_preference.unwrap_or_default();
    let expressed = traits.map_or_else(|| m.genome.expressed_mate_desire(), |catalog| catalog.mate_desire(&m.genome));
    let genetic = (i128::from(expressed) - 500)
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
    let feelings = physical.mate_feelings(observer.id, target.id);
    let sex_attraction = match target.genome.sex {
        Sex::Female => pref.female_attraction,
        Sex::Male => pref.male_attraction,
    };
    let childhood = if feelings.caregiver_known { i64::from(pref.childhood_weight) } else { 0 };
    if pref.trust_weight == 0 && pref.fear_weight == 0 && pref.obligation_weight == 0 {
        return i64::from(pref.affinity_weight) * i64::from(feelings.affinity)
            + i64::from(pref.attraction_weight) * attraction / 1000
            + i64::from(sex_attraction) + childhood;
    }
    let score = i128::from(pref.affinity_weight) * i128::from(feelings.affinity)
        + i128::from(pref.trust_weight) * i128::from(feelings.trust)
        + i128::from(pref.fear_weight) * i128::from(feelings.fear)
        + i128::from(pref.obligation_weight) * i128::from(feelings.obligation)
        + i128::from(pref.attraction_weight) * i128::from(attraction) / 1000
        + i128::from(sex_attraction) + i128::from(childhood);
    score.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
fn mutual_mate_score(
    observer: &Member,
    target: &Member,
    physical: &impl game_family_model::PhysicalView,
    seed: u64,
) -> Option<i64> {
    let score = mate_score(observer, target, physical, seed);
    (score >= i64::from(observer.mate_preference.as_ref().map_or(0, |p| p.minimum))
        && accepts_mate(target, observer, physical, seed))
    .then_some(score)
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

/// Each person ranks a bounded sample and each endpoint independently chooses
/// its favorite of the proposed edges. Only mutual choices become partnerships.
fn subjective_partnerships(
    group: &[(u64, game_family_model::ContactPartition, &Member)],
    index: &OrderedMemberIndex<'_>,
    physical: &impl game_family_model::PhysicalView,
    dependent_count: &(impl Fn(u64) -> u64 + Sync),
    seed: u64,
    tick: u64,
    traits: Option<&TraitCatalog>,
) -> Vec<(u64, u64, u64)> {
    if group.len() < 2 {
        return Vec::new();
    }
    let region = group[0].0;
    let mut candidates: Vec<_> = group.par_iter().map(|row| row.2).collect();
    candidates.par_sort_unstable_by_key(|member| (random(seed ^ 31, member.id, tick), member.id));
    let edges: Vec<_> = candidates
        .par_iter()
        .filter_map(|person| {
            if !seeks_mate(
                person,
                &physical.read(person.id),
                dependent_count(person.id),
                seed,
                tick,
                traits,
            ) {
                return None;
            }
            let offset = (random(seed ^ 71, person.id, tick) % candidates.len() as u64) as usize;
            let choice = (0..candidates.len().min(32))
                .map(|n| (offset + n) % candidates.len())
                .filter(|&i| candidates[i].id != person.id)
                .filter_map(|i| {
                    mutual_mate_score(person, candidates[i], physical, seed).map(|score| (i, score))
                })
                .max_by_key(|&(i, score)| (score, std::cmp::Reverse(candidates[i].id)))?
                .0;
            let other = candidates[choice].id;
            Some((person.id.min(other), person.id.max(other)))
        })
        .collect();
    mutually_preferred_edges(edges, index, physical, seed, tick)
        .into_par_iter()
        .map(|(a, b)| (a, b, region))
        .collect()
}

// The choices are one per person at this opportunity. Matches do not impose
// exclusivity or change the participants' later preferences.
fn mutually_preferred_edges(
    mut edges: Vec<(u64, u64)>,
    index: &OrderedMemberIndex<'_>,
    physical: &impl game_family_model::PhysicalView,
    seed: u64,
    time: u64,
) -> Vec<(u64, u64)> {
    edges.par_sort_unstable();
    edges.dedup();
    let mut rankings: Vec<_> = edges
        .par_iter()
        .flat_map_iter(|&(a, b)| {
            [
                (
                    a,
                    std::cmp::Reverse(mate_score(&index[&a], &index[&b], physical, seed)),
                    random(seed ^ time, a, b),
                    b,
                ),
                (
                    b,
                    std::cmp::Reverse(mate_score(&index[&b], &index[&a], physical, seed)),
                    random(seed ^ time, b, a),
                    a,
                ),
            ]
        })
        .collect();
    rankings.par_sort_unstable();
    let selected: Vec<_> = rankings
        .par_iter()
        .enumerate()
        .filter_map(|(i, row)| (i == 0 || rankings[i - 1].0 != row.0).then_some((row.0, row.3)))
        .collect();
    edges
        .into_par_iter()
        .filter_map(|(a, b)| {
            let accepts = |person, other| {
                selected
                    .binary_search_by_key(&person, |row| row.0)
                    .ok()
                    .is_some_and(|i| selected[i].1 == other)
            };
            (accepts(a, b) && accepts(b, a)).then_some((a, b))
        })
        .collect()
}
impl Families {
    fn parallel_copy(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            traits: self.traits.clone(),
            familiarity: self.familiarity.clone(),
            members: self.members.par_iter().cloned().collect(),
            next_id: self.next_id,
            last_tick: self.last_tick,
            elapsed_lifetimes: self.elapsed_lifetimes.clone(),
            totals: self.totals.clone(),
            events: self.events.par_iter().cloned().collect(),
        }
    }

    /// Each participant evaluates an encounter at this elapsed opportunity.
    pub fn choose_conceptions_elapsed(
        &self,
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        now: u128,
    ) -> Result<Option<ElapsedFamilyDraft>, &'static str> {
        let calendar = self
            .elapsed_lifetimes
            .as_ref()
            .ok_or("elapsed conception has no calendar")?;
        let period = calendar
            .conception_period_millis
            .ok_or("elapsed conception has no period")?;
        if period == 0 || now == 0 || !now.is_multiple_of(u128::from(period)) {
            return Err("conception is outside its elapsed opportunity");
        }
        let mut parents: Vec<_> = self
            .members
            .par_iter()
            .filter(|member| {
                member.alive
                    && self.age_at(member, now) < self.rules.adult_age
                    && physical.read(member.id).health > 0
            })
            .flat_map_iter(|member| member.parents.iter().copied())
            .collect();
        parents.par_sort_unstable();
        let dependents = |id| {
            (parents.partition_point(|parent| *parent <= id)
                - parents.partition_point(|parent| *parent < id)) as u64
        };
        let mut choices = prepare_conceptions(
            self,
            &self.members,
            physical,
            &dependents,
            seed,
            ConceptionTime {
                tick,
                now: Some(now),
                exact: true,
            },
        )?;
        let relationships = if calendar.review_partnerships {
            relationships::review(self, physical, &dependents, seed, tick, now)
        } else {
            relationships::Draft::default()
        };
        if choices.is_empty() && relationships.events.is_empty() {
            return Ok(None);
        }
        choices.par_sort_unstable_by_key(|choice| choice.0);
        let mut families = self.parallel_copy();
        families.members.par_iter_mut().for_each(|member| {
            if let Ok(choice) = choices.binary_search_by_key(&member.id, |choice| choice.0) {
                member.pregnancy = Some(choices[choice].1);
            }
            if let Ok(change) = relationships
                .changes
                .binary_search_by_key(&member.id, |row| row.0)
            {
                member.partner = relationships.changes[change].1;
            }
        });
        families.totals.conceptions = families
            .totals
            .conceptions
            .checked_add(choices.len() as u64)
            .ok_or("family counter overflow")?;
        families.totals.partnerships = families
            .totals
            .partnerships
            .checked_add(relationships.formed)
            .ok_or("family counter overflow")?;
        if !choices.is_empty() {
            families
                .elapsed_lifetimes
                .as_mut()
                .expect("checked conception clock")
                .refresh_deliveries(&families.members);
        }
        let mut events = relationships.events;
        events.par_extend(choices.par_iter().map(|&(mother, pregnancy)| {
            event(
                tick,
                "conception",
                vec![mother, pregnancy.father],
                physical.read(mother).region,
            )
        }));
        Ok(Some(ElapsedFamilyDraft {
            families,
            births: Vec::new(),
            events,
        }))
    }

    pub fn deliver_elapsed(
        &self,
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        now: u128,
    ) -> Result<ElapsedFamilyDraft, &'static str> {
        let calendar = self
            .elapsed_lifetimes
            .as_ref()
            .filter(|calendar| calendar.elapsed_gestation)
            .ok_or("elapsed delivery requires a gestation clock")?;
        let due = |member: &Member| {
            member
                .pregnancy
                .as_ref()
                .and_then(|p| p.due_at_millis)
                .is_some_and(|at| at <= now)
        };
        let healthy = |member: &Member| member.alive && physical.read(member.id).health > 0;
        let stage = stage_births(
            self.next_id,
            &self.members,
            &OrderedMemberIndex::new(&self.members),
            self,
            physical,
            seed,
            BirthTime {
                tick,
                millis: Some(now),
            },
        )?;
        let mut families = self.parallel_copy();
        let mut events: Vec<_> = families
            .members
            .par_iter_mut()
            .filter_map(|member| {
                if !due(member) {
                    return None;
                }
                let region = physical.read(member.id).region;
                let died = !healthy(member);
                let father = member.pregnancy.take().expect("due pregnancy").father;
                died.then(|| event(tick, "pregnancy_loss", vec![member.id, father], region))
            })
            .collect();
        let lost = events.len() as u64;
        let next = families
            .elapsed_lifetimes
            .as_mut()
            .expect("validated gestation clock");
        if !stage.births.is_empty() {
            let recovery = now
                .checked_add(
                    u128::from(calendar.unit_millis) * u128::from(self.rules.birth_spacing),
                )
                .ok_or("elapsed birth spacing overflow")?;
            std::sync::Arc::make_mut(&mut next.birth_recovery)
                .extend(stage.births.iter().map(|birth| (birth.mother, recovery)));
            next.enroll(&self.rules, &stage.newborns, now)?;
        }
        families.next_id = stage.next_id;
        families.totals.births = families
            .totals
            .births
            .checked_add(stage.births.len() as u64)
            .ok_or("family counter overflow")?;
        families.totals.pregnancy_losses = families
            .totals
            .pregnancy_losses
            .checked_add(lost)
            .ok_or("family counter overflow")?;
        families.members.extend(stage.newborns);
        next.refresh_deliveries(&families.members);
        events.extend(stage.events);
        events.par_sort_unstable_by(|a, b| (&a.kind, &a.people).cmp(&(&b.kind, &b.people)));
        Ok(ElapsedFamilyDraft {
            families,
            births: stage.births,
            events,
        })
    }

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
        self.advance_phase(physical, seed, tick, penalty, None)
    }

    /// Settle family records after the elapsed host has applied natural mortality.
    pub fn advance_validated_at(
        &self,
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        now: u128,
    ) -> Result<Draft, &'static str> {
        self.advance_phase(physical, seed, tick, None, Some(now))
    }

    fn advance_phase(
        &self,
        physical: &impl game_family_model::PhysicalView,
        seed: u64,
        tick: u64,
        penalty: Option<(u64, u64)>,
        now: Option<u128>,
    ) -> Result<Draft, &'static str> {
        #[cfg(feature = "profile")]
        let mut profile_phase = game_diagnostics::span("family.life");
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("family phase must advance one tick");
        }
        let calendar = self.elapsed_lifetimes.as_ref();
        if calendar.is_some() && now.is_none() {
            return Err("elapsed natural mortality requires an interval host");
        }
        let rules = &self.rules;
        let elapsed_partnerships = calendar.is_some_and(|clock| clock.review_partnerships);
        let (mut members, mut events) = stage_life(self, physical, tick, penalty, now)?;
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.reconsider");
        }
        if !elapsed_partnerships {
            game_family_index::reconsider(&mut members, physical, rules.fertile_until)?;
        }
        if rules.subjective_mating && !elapsed_partnerships {
            let index = OrderedMemberIndex::new(&members);
            let mut separations: Vec<_> = members
                .par_iter()
                .filter_map(|person| {
                    let partner = index.get(&person.partner?)?;
                    (!physical.controlled(person.id)
                        && !accepts_mate(person, partner, physical, seed))
                    .then_some((person.id.min(partner.id), person.id.max(partner.id)))
                })
                .collect();
            separations.par_sort_unstable();
            separations.dedup();
            let mut separated: Vec<_> = separations
                .par_iter()
                .flat_map_iter(|&(a, b)| [a, b])
                .collect();
            separated.par_sort_unstable();
            events.par_extend(
                separations
                    .par_iter()
                    .map(|&(a, b)| event(tick, "separation", vec![a, b], physical.read(a).region)),
            );
            members.par_iter_mut().for_each(|member| {
                if separated.binary_search(&member.id).is_ok() {
                    member.partner = None;
                }
            });
        }
        let index = OrderedMemberIndex::new(&members);
        let mut parents: Vec<_> = if rules.subjective_mating && !elapsed_partnerships {
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
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.matching");
        }
        let mut local: Vec<_> = if elapsed_partnerships {
            Vec::new()
        } else {
            members
                .par_iter()
                .filter_map(|m| {
                    let person = physical.read(m.id);
                    (m.partner.is_none()
                        && !physical.controlled(m.id)
                        && if rules.subjective_mating {
                            eligible_for_partnership(m, &person, rules)
                        } else {
                            eligible(m, &person, rules)
                        })
                    .then(|| (person.region.unwrap(), physical.contact_partition(m.id), m))
                })
                .collect()
        };
        local.par_sort_unstable_by_key(|(region, partition, member)| {
            (*region, *partition, member.id)
        });
        let matches: Vec<Vec<_>> = local
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| {
                if rules.subjective_mating {
                    return subjective_partnerships(
                        group,
                        &index,
                        physical,
                        &dependent_count,
                        seed,
                        tick,
                        self.traits.as_deref(),
                    );
                }
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
                        let candidates = (0..males.len().min(32))
                            .map(|n| (offset + n) % males.len())
                            .filter(|&candidate| ancestors.is_disjoint(&kin[candidate]));
                        let chosen = candidates.into_iter().next();
                        chosen.map(|candidate| (mother.id, males[candidate].id, area))
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
        let BirthStage {
            next_id,
            recovered,
            newborns,
            births,
            events: birth_events,
        } = stage_births(
            self.next_id,
            &members,
            &index,
            self,
            physical,
            seed,
            BirthTime { tick, millis: None },
        )?;
        events.par_extend(birth_events);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("family.conceptions");
        }
        let conceptions =
            if calendar.is_some_and(|calendar| calendar.conception_period_millis.is_some()) {
                Vec::new()
            } else {
                prepare_conceptions(
                    self,
                    &members,
                    physical,
                    &dependent_count,
                    seed,
                    ConceptionTime {
                        tick,
                        now,
                        exact: false,
                    },
                )?
            };
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
                    member.pregnancy = Some(conceived[conception_index].1);
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
        let mut elapsed_lifetimes = calendar
            .zip(now)
            .map(|(calendar, now)| calendar.advance(rules, &deaths, &newborns, now))
            .transpose()?;
        members.extend(newborns);
        if let Some(calendar) = &mut elapsed_lifetimes {
            calendar.refresh_deliveries(&members);
        }
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
                traits: self.traits.clone(),
                familiarity: self.familiarity.clone(),
                members,
                next_id,
                last_tick: tick,
                elapsed_lifetimes,
                totals,
                events,
            },
            births,
            deaths,
        })
    }
}

struct ConceptionTime {
    tick: u64,
    now: Option<u128>,
    exact: bool,
}

fn elapsed_entropy(now: u128) -> u64 {
    now as u64 ^ ((now >> 64) as u64).rotate_left(29)
}

fn prepare_conceptions(
    family: &Families,
    members: &[Member],
    physical: &impl game_family_model::PhysicalView,
    dependent_count: &(impl Fn(u64) -> u64 + Sync),
    seed: u64,
    time: ConceptionTime,
) -> Result<Vec<(u64, Pregnancy)>, &'static str> {
    let rules = &family.rules;
    let calendar = family.elapsed_lifetimes.as_ref();
    let tick = time.tick;
    let now = time.now;
    let entropy = if time.exact {
        elapsed_entropy(now.expect("elapsed conception instant"))
    } else {
        tick
    };
    let is_eligible = |member: &Member, person: &LifeInput| {
        let age = if time.exact {
            family.age_at(member, now.expect("elapsed conception instant"))
        } else {
            member.age
        };
        eligible_at_age(member, person, rules, age)
    };
    let index = OrderedMemberIndex::new(members);
    let can_carry = |mother: &Member| {
        mother.pregnancy.is_none()
            && match calendar
                .filter(|calendar| calendar.elapsed_gestation)
                .zip(now)
            {
                Some((calendar, now)) => calendar
                    .birth_recovery
                    .get(&mother.id)
                    .is_none_or(|due| *due <= now),
                None => mother.next_conception <= tick,
            }
    };
    let mut selected: Vec<_> = if rules.subjective_mating {
        let (mut mothers, mut fathers): (Vec<_>, Vec<_>) = members
            .par_iter()
            .filter_map(|member| {
                let person = physical.read(member.id);
                (!physical.controlled(member.id)
                    && is_eligible(member, &person)
                    && (member.genome.sex == Sex::Male || can_carry(member)))
                .then(|| {
                    (
                        (
                            person.region.expect("eligible local person"),
                            physical.contact_partition(member.id),
                        ),
                        member,
                        person,
                    )
                })
            })
            .partition(|(_, member, _)| member.genome.sex == Sex::Female);
        let key = |row: &MateCandidate<'_>| {
            (
                row.0,
                random(seed ^ 0x46415448, row.1.id, entropy),
                row.1.id,
            )
        };
        rayon::join(
            || mothers.par_sort_unstable_by_key(key),
            || fathers.par_sort_unstable_by_key(key),
        );
        let propose = |sources: &[MateCandidate<'_>], targets: &[MateCandidate<'_>]| {
            sources
                .par_iter()
                .filter_map(|(area, source, person)| {
                    if !seeks_mate(
                        source,
                        person,
                        dependent_count(source.id),
                        seed,
                        entropy,
                        family.traits.as_deref(),
                    ) {
                        return None;
                    }
                    let first = targets.partition_point(|row| row.0 < *area);
                    let end = targets.partition_point(|row| row.0 <= *area);
                    if first == end {
                        return None;
                    }
                    let offset = (random(seed ^ 0x5345454b, source.id, entropy)
                        % (end - first) as u64) as usize;
                    let target = (0..(end - first).min(32))
                        .map(|n| targets[first + (offset + n) % (end - first)].1)
                        .filter_map(|target| {
                            mutual_mate_score(source, target, physical, seed)
                                .map(|score| (target.id, score))
                        })
                        .max_by_key(|&(target, score)| (score, std::cmp::Reverse(target)))?
                        .0;
                    Some((source.id.min(target), source.id.max(target)))
                })
                .collect::<Vec<_>>()
        };
        let (mut proposals, responded) = rayon::join(
            || propose(&mothers, &fathers),
            || propose(&fathers, &mothers),
        );
        proposals.par_extend(responded);
        mutually_preferred_edges(proposals, &index, physical, seed, entropy)
            .into_par_iter()
            .map(|(a, b)| {
                if index[&a].genome.sex == Sex::Female {
                    (a, b)
                } else {
                    (b, a)
                }
            })
            .collect()
    } else {
        members
            .par_iter()
            .filter_map(|mother| {
                if mother.genome.sex != Sex::Female
                    || physical.controlled(mother.id)
                    || !can_carry(mother)
                    || !is_eligible(mother, &physical.read(mother.id))
                {
                    return None;
                }
                let father = index.get(&mother.partner?)?;
                let father_person = physical.read(father.id);
                (father.genome.sex == Sex::Male
                    && !physical.controlled(father.id)
                    && is_eligible(father, &father_person)
                    && father_person.region == physical.read(mother.id).region
                    && physical.contact_partition(father.id)
                        == physical.contact_partition(mother.id)
                    && ancestors(mother.id, &index).is_disjoint(&ancestors(father.id, &index)))
                .then_some((mother.id, father.id))
            })
            .collect()
    };
    selected.par_sort_unstable();
    selected
        .into_par_iter()
        .filter_map(|(mother_id, father_id)| {
            let mother = &index[&mother_id];
            let chance = 500 + u64::from(mother.genome.expressed(0));
            if !random(seed ^ 97, mother.id, entropy).is_multiple_of(rules.conception_one_in)
                || random(seed ^ 107, mother.id, entropy) % 1500 >= chance
            {
                return None;
            }
            Some((|| {
                let (due, due_at_millis) = match calendar
                    .filter(|calendar| calendar.elapsed_gestation)
                    .zip(now)
                {
                    Some((calendar, now)) => (
                        entropy,
                        Some(
                            now.checked_add(
                                u128::from(calendar.unit_millis) * u128::from(rules.gestation),
                            )
                            .ok_or("elapsed gestation overflow")?,
                        ),
                    ),
                    None => (
                        tick.checked_add(rules.gestation)
                            .ok_or("gestation overflow")?,
                        None,
                    ),
                };
                Ok((
                    mother.id,
                    Pregnancy {
                        father: father_id,
                        due,
                        due_at_millis,
                    },
                ))
            })())
        })
        .collect()
}

type MateCandidate<'a> = (
    (u64, game_family_model::ContactPartition),
    &'a Member,
    LifeInput,
);

/// Physical age and mortality use only the opening family and physical views.
fn stage_life(
    opening: &Families,
    physical: &impl game_family_model::PhysicalView,
    tick: u64,
    penalty: Option<(u64, u64)>,
    now: Option<u128>,
) -> Result<(Vec<Member>, Vec<FamilyEvent>), &'static str> {
    // Clone directly into the staged generation, avoiding per-chunk member
    // buffers and a second population-sized flattening pass.
    let rules = &opening.rules;
    let mut members: Vec<_> = opening.members.par_iter().cloned().collect();
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
                    let old_age = m.age;
                    m.age = if let Some((calendar, now)) =
                        opening.elapsed_lifetimes.as_ref().zip(now)
                    {
                        calendar.age_at(rules, m, now)
                    } else {
                        m.age.checked_add(1).ok_or("age overflow")?
                    };
                    let person = physical.read(m.id);
                    if person.health == 0
                        || (opening.elapsed_lifetimes.is_none()
                            && m.age >= lifetimes::lifespan(rules, m))
                    {
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
                    } else if old_age < rules.adult_age && m.age >= rules.adult_age {
                        events.push(event(tick, "adulthood", vec![m.id], person.region));
                    }
                }
            }
            Ok(events)
        })
        .collect();
    let event_chunks = chunks.into_iter().collect::<Result<Vec<_>, _>>()?;
    let events: Vec<_> = event_chunks.into_par_iter().flatten().collect();
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
            (last.checked_sub(*first) == Some((alive.len() - 1) as u64)).then_some((*first, *last))
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
    Ok((members, events))
}

struct BirthStage {
    next_id: u64,
    recovered: Vec<(u64, u64)>,
    newborns: Vec<Member>,
    births: Vec<Birth>,
    events: Vec<FamilyEvent>,
}

struct BirthTime {
    tick: u64,
    millis: Option<u128>,
}

/// Stage the births that are physically due, independently of any new encounter.
fn stage_births(
    first_id: u64,
    members: &[Member],
    index: &OrderedMemberIndex<'_>,
    family: &Families,
    physical: &impl game_family_model::PhysicalView,
    seed: u64,
    time: BirthTime,
) -> Result<BirthStage, &'static str> {
    let rules = &family.rules;
    let tick = time.tick;
    let entropy = time.millis.map_or(tick, |now| {
        now as u64 ^ ((now >> 64) as u64).rotate_left(29)
    });
    let due: Vec<_> = members
        .par_iter()
        .filter(|m| {
            m.alive
                && m.pregnancy.as_ref().is_some_and(|p| match time.millis {
                    Some(now) => {
                        p.due_at_millis.is_some_and(|due| due <= now)
                            && physical.read(m.id).health > 0
                    }
                    None => p.due_at_millis.is_none() && p.due <= tick,
                })
        })
        .collect();
    let next_id = first_id
        .checked_add(due.len() as u64)
        .ok_or("birth identity overflow")?;
    let children: Vec<Result<_, &'static str>> = due
        .par_iter()
        .enumerate()
        .map(|(offset, mother)| {
            let father = &index[&mother.pregnancy.as_ref().unwrap().father];
            let id = first_id + offset as u64;
            let genome = Inheritance {
                seed,
                mother: mother.id,
                time: entropy,
                mutation_per_million: rules.mutation_per_million,
                mutation_step: rules.mutation_step,
            }.child(&mother.genome, &father.genome, family.traits.as_deref());
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
            let recover = if time.millis.is_some() {
                tick
            } else {
                tick.checked_add(rules.birth_spacing)
                    .ok_or("birth spacing overflow")?
            };
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
    let events = children
        .par_iter()
        .map(|(child, birth, _)| {
            event(
                tick,
                "birth",
                vec![child.id, child.parents[0], child.parents[1]],
                birth.region,
            )
        })
        .collect();
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
    Ok(BirthStage {
        next_id,
        recovered,
        newborns,
        births,
        events,
    })
}
