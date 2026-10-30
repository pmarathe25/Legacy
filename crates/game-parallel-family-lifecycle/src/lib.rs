//! Transactional lifecycle staging over reusable columns; editable Families is export-only.
use game_parallel_families::{Families, FamilyEvent, FamilyRules, FamilyTotals};
use game_parallel_family_columns::{BLOCK, Columns, Store};
use game_parallel_family_model::{Birth, Member, PhysicalView};
use game_parallel_family_receipts::{Frame, Kind, Row};
use game_parallel_genetics::inherit;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
static INSTANCE: AtomicU64 = AtomicU64::new(1);
pub struct Runtime {
    pub store: Store,
    pub rules: FamilyRules,
    pub totals: FamilyTotals,
    pub receipts: Option<Frame>,
    imported_events: Vec<FamilyEvent>,
    pub next_id: u64,
    pub last_tick: u64,
    unpaired: Vec<usize>,
    epoch: u64,
    instance: u64,
}
pub struct Draft {
    pub births: Vec<Birth>,
    pub deaths: Vec<u64>,
    pub receipts: Frame,
    newborns: Vec<Member>,
    totals: FamilyTotals,
    next_id: u64,
    tick: u64,
    unpaired: Vec<usize>,
    epoch: u64,
    instance: u64,
}
fn event(_tick: u64, kind: Kind, people: &[u64], region: Option<u64>) -> Row {
    let mut ids = [0; 3];
    ids[..people.len()].copy_from_slice(people);
    Row::new(kind, ids, region)
}
fn apply<T: Sync>(
    store: &mut Store,
    edits: &[(usize, T)],
    write: impl Fn(&mut Columns, usize, &T) + Sync,
) {
    if edits.is_empty() {
        return;
    }
    store
        .blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(block, data)| {
            let base = block * BLOCK;
            let start = edits.partition_point(|entry| entry.0 < base);
            let end = edits.partition_point(|entry| entry.0 < base + data.next.age.len());
            for (slot, value) in &edits[start..end] {
                write(&mut data.next, slot - base, value);
            }
        });
}
impl Runtime {
    /// Import after the host validates physical/family identities and commitments.
    pub fn compile_validated(families: &Families) -> Result<Self, &'static str> {
        families.rules.validate()?;
        let store = Store::compile(&families.members)?;
        let unpaired = store
            .metadata
            .par_iter()
            .enumerate()
            .filter_map(|(slot, _)| {
                let state = store.state(slot, false);
                (state.alive
                    && state.reproduction_allowed
                    && state.partner.is_none()
                    && state.age >= families.rules.adult_age
                    && state.age < families.rules.fertile_until)
                    .then_some(slot)
            })
            .collect();
        Ok(Self {
            store,
            unpaired,
            rules: families.rules.clone(),
            totals: families.totals.clone(),
            receipts: None,
            imported_events: families.events.clone(),
            next_id: families.next_id,
            last_tick: families.last_tick,
            epoch: 0,
            instance: INSTANCE.fetch_add(1, Ordering::Relaxed),
        })
    }
    pub fn export(&self) -> Families {
        Families {
            rules: self.rules.clone(),
            members: self.store.export(),
            next_id: self.next_id,
            last_tick: self.last_tick,
            totals: self.totals.clone(),
            events: self.events(),
        }
    }
    pub fn events(&self) -> Vec<FamilyEvent> {
        self.receipts
            .as_ref()
            .map_or_else(|| self.imported_events.clone(), Frame::export)
    }
    pub fn births(&self) -> Vec<u64> {
        self.receipts.as_ref().map_or_else(Vec::new, |frame| {
            frame
                .rows()
                .par_iter()
                .filter(|row| row.kind() == Kind::Birth)
                .map(|row| row.people()[0])
                .collect()
        })
    }
    /// Deaths in the just-committed lifecycle generation, in canonical order.
    pub fn deaths(&self) -> Vec<u64> {
        self.receipts.as_ref().map_or_else(Vec::new, |frame| {
            frame
                .rows()
                .par_iter()
                .filter(|row| row.kind() == Kind::Death)
                .map(|row| row.people()[0])
                .collect()
        })
    }
    /// The partner held before the just-committed death; valid until the next preparation.
    pub fn previous_partner(&self, person: u64) -> Option<u64> {
        self.store
            .locate(person)
            .and_then(|slot| self.store.state(slot, true).partner)
    }
    pub fn newborn(&self) -> Option<u64> {
        self.receipts
            .as_ref()
            .and_then(|frame| frame.people(Kind::Birth).next())
    }
    pub fn alive(&self, id: u64) -> Option<bool> {
        self.store
            .locate(id)
            .map(|slot| self.store.state(slot, false).alive)
    }
    pub fn prepare(
        &mut self,
        physical: &impl PhysicalView,
        seed: u64,
        tick: u64,
        penalty: Option<(u64, u64)>,
    ) -> Result<Draft, &'static str> {
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("family phase must advance one tick");
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("family draft generation overflow")?;
        let rules = &self.rules;
        let metadata = &self.store.metadata;
        let chunks: Vec<_> = self
            .store
            .blocks
            .par_iter_mut()
            .enumerate()
            .map(|(block, data)| {
                let mut events = Vec::new();
                for offset in 0..data.current.age.len() {
                    let row = &metadata[block * BLOCK + offset];
                    let mut state = data.current.read(offset);
                    if state.alive {
                        if let Some((person, amount)) = penalty
                            && row.id == person
                        {
                            state.lifespan_loss = state
                                .lifespan_loss
                                .checked_add(amount)
                                .ok_or("lifespan penalty overflow")?;
                        }
                        state.age = state.age.checked_add(1).ok_or("age overflow")?;
                        let lifespan = rules.lifespan_min
                            + ((u128::from(rules.lifespan_max - rules.lifespan_min)
                                * u128::from(row.genome.expressed(3)))
                                / 1000) as u64;
                        let person = physical.read(row.id);
                        if person.health == 0
                            || state.age >= lifespan.saturating_sub(state.lifespan_loss)
                        {
                            state.alive = false;
                            events.push(event(tick, Kind::Death, &[row.id], person.region));
                            if let Some((father, _)) = state.pregnancy.take() {
                                events.push(event(
                                    tick,
                                    Kind::PregnancyLoss,
                                    &[row.id, father],
                                    person.region,
                                ));
                            }
                        } else if state.age == rules.adult_age {
                            events.push(event(tick, Kind::Adulthood, &[row.id], person.region));
                        }
                    }
                    data.next.write(offset, state);
                }
                Ok(events)
            })
            .collect::<Result<_, &'static str>>()?;
        let mut events: Vec<_> = chunks.into_par_iter().flatten().collect();
        let cleared: Vec<_> = self
            .store
            .metadata
            .par_iter()
            .enumerate()
            .filter_map(|(slot, row)| {
                let state = self.store.state(slot, true);
                let partner = state.partner?;
                let other_slot = self.store.locate(partner).expect("validated partner");
                let other = self.store.state(other_slot, true);
                let other_row = &self.store.metadata[other_slot];
                let wants_to_leave =
                    |id: u64,
                     preference: bool,
                     own: game_parallel_family_columns::State,
                     mate: game_parallel_family_columns::State| {
                        !physical.controlled(id)
                            && preference
                            && own.alive
                            && own.reproduction_allowed
                            && own.age < rules.fertile_until
                            && (!mate.reproduction_allowed || mate.age >= rules.fertile_until)
                    };
                (!state.alive
                    || !other.alive
                    || wants_to_leave(row.id, row.reconsider_partnership, state, other)
                    || wants_to_leave(other_row.id, other_row.reconsider_partnership, other, state))
                .then_some((slot, ()))
            })
            .collect();
        apply(&mut self.store, &cleared, |next, offset, _| {
            next.partner[offset] = None
        });
        let mut unpaired: Vec<_> = self.unpaired.par_iter().copied().collect();
        unpaired.par_extend(cleared.par_iter().map(|entry| entry.0));
        unpaired.par_extend(
            events
                .par_iter()
                .filter(|event| event.kind() == Kind::Adulthood)
                .map(|event| self.store.locate(event.people()[0]).unwrap()),
        );
        if unpaired.len() != self.unpaired.len() {
            unpaired.par_sort_unstable();
            unpaired.dedup();
        }
        unpaired = unpaired
            .into_par_iter()
            .filter(|&slot| {
                let state = self.store.state(slot, true);
                state.alive
                    && state.reproduction_allowed
                    && state.partner.is_none()
                    && state.age >= rules.adult_age
                    && state.age < rules.fertile_until
            })
            .collect();
        let matches = game_parallel_family_pairing::partnerships(
            &self.store,
            &unpaired,
            physical,
            rules,
            seed,
            tick,
        );
        let mut partners: Vec<_> = matches
            .par_iter()
            .flat_map_iter(|&(a, b, _)| {
                [
                    (self.store.locate(a).unwrap(), b),
                    (self.store.locate(b).unwrap(), a),
                ]
            })
            .collect();
        partners.par_sort_unstable_by_key(|entry| entry.0);
        apply(&mut self.store, &partners, |next, offset, &partner| {
            next.partner[offset] = Some(partner)
        });
        events.par_extend(
            matches
                .par_iter()
                .map(|&(a, b, region)| event(tick, Kind::Partnership, &[a, b], Some(region))),
        );
        let due: Vec<_> = self
            .store
            .metadata
            .par_iter()
            .enumerate()
            .filter_map(|(slot, _)| {
                let state = self.store.state(slot, true);
                (state.alive && state.pregnancy.is_some_and(|(_, due)| due <= tick)).then_some(slot)
            })
            .collect();
        let next_id = self
            .next_id
            .checked_add(due.len() as u64)
            .ok_or("birth identity overflow")?;
        let children: Vec<_> = due
            .par_iter()
            .enumerate()
            .map(|(offset, &slot)| {
                let mother = &self.store.metadata[slot];
                let father_id = self.store.state(slot, true).pregnancy.unwrap().0;
                let father =
                    &self.store.metadata[self.store.locate(father_id).expect("validated father")];
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
                    parents: [mother.id, father.id].into_iter().collect(),
                    partner: None,
                    pregnancy: None,
                    next_conception: 0,
                    alive: true,
                    reproduction_allowed: true,
                    reconsider_partnership: mother.reconsider_partnership,
                };
                let recovery = tick
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
                    recovery,
                ))
            })
            .collect::<Result<_, &'static str>>()?;
        events.par_extend(children.par_iter().map(|(child, birth, _)| {
            event(
                tick,
                Kind::Birth,
                &[child.id, child.parents[0], child.parents[1]],
                birth.region,
            )
        }));
        let recovered: Vec<_> = children
            .par_iter()
            .map(|(_, birth, recovery)| (self.store.locate(birth.mother).unwrap(), *recovery))
            .collect();
        let (newborns, births): (Vec<_>, Vec<_>) = children
            .into_par_iter()
            .map(|(child, birth, _)| (child, birth))
            .unzip();
        // Selection sees the due pregnancy; recovery applies after conception selection.
        let conceived =
            game_parallel_family_pairing::conceptions(&self.store, physical, rules, seed, tick)?;
        events.par_extend(conceived.par_iter().map(|&(mother, (father, _))| {
            event(
                tick,
                Kind::Conception,
                &[mother, father],
                physical.read(mother).region,
            )
        }));
        let conceived: Vec<_> = conceived
            .into_par_iter()
            .map(|(id, pregnancy)| (self.store.locate(id).unwrap(), pregnancy))
            .collect();
        apply(&mut self.store, &recovered, |next, offset, &recovery| {
            next.pregnancy[offset] = None;
            next.next_conception[offset] = recovery;
        });
        apply(&mut self.store, &conceived, |next, offset, &pregnancy| {
            next.pregnancy[offset] = Some(pregnancy)
        });
        let receipts = Frame::canonical(tick, events);
        let deaths = receipts.people(Kind::Death).collect();
        let raw = receipts.counts();
        let counts = [raw[1], raw[3], raw[2], raw[5], raw[0], raw[4]];
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
        if !partners.is_empty() {
            unpaired = unpaired
                .into_par_iter()
                .filter(|&slot| self.store.state(slot, true).partner.is_none())
                .collect();
        }
        Ok(Draft {
            births,
            deaths,
            receipts,
            newborns,
            totals,
            next_id,
            tick,
            unpaired,
            epoch: self.epoch,
            instance: self.instance,
        })
    }
    pub fn check(&self, draft: &Draft) -> Result<(), &'static str> {
        if draft.instance != self.instance
            || draft.epoch != self.epoch
            || self.last_tick.checked_add(1) != Some(draft.tick)
        {
            return Err("stale family draft");
        }
        Ok(())
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), &'static str> {
        self.check(&draft)?;
        self.store.publish(draft.newborns);
        self.unpaired = draft.unpaired;
        self.receipts = Some(draft.receipts);
        self.imported_events.clear();
        self.totals = draft.totals;
        self.next_id = draft.next_id;
        self.last_tick = draft.tick;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/differential.rs"]
mod differential;
