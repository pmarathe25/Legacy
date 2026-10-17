use game_core::*;
use game_knowledge::{Fact, FactValue, Facts, StoreLocation};
use game_physiology::{Body, Kind, Rules, advance_body};
use game_storage::Table;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
pub const HOT: usize = 4;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    pub last: u64,
    pub score: i64,
    pub failures: u64,
}
#[derive(Clone, Copy, Debug)]
struct Stamp {
    source: PersonId,
    observed: SimTime,
    revision: u64,
    confidence: u8,
}
impl Stamp {
    fn of(f: &Fact) -> Self {
        Self {
            source: f.source,
            observed: f.observed,
            revision: f.revision,
            confidence: f.confidence,
        }
    }
    fn apply(self, f: &mut Fact) {
        f.source = self.source;
        f.observed = self.observed;
        f.revision = self.revision;
        f.confidence = self.confidence;
    }
    fn fact(self, value: Arc<FactValue>) -> Fact {
        Fact {
            value,
            source: self.source,
            observed: self.observed,
            revision: self.revision,
            confidence: self.confidence,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: PersonId,
    pub body: Body,
    pub mind: Mind,
    pub facts: [Fact; HOT],
}
pub struct Source {
    pub keys: [Arc<str>; HOT],
    pub values: [Arc<FactValue>; HOT],
}
pub struct World {
    pub cold: Table<Arc<str>, Fact>,
    pub sources: Vec<Source>,
    pub stock: u64,
    pub rules: Rules,
}
impl World {
    pub fn new(n: usize, history: usize, bins: usize) -> Arc<Self> {
        let stock = (n.div_ceil(bins) as u64) * 10;
        let value = Arc::new(FactValue::Other(
            serde_json::json!({"history":"shared report", "extension":{"preserved":true}}),
        ));
        let cold = (0..history)
            .map(|i| {
                (
                    format!("memory:{i:08}").into(),
                    Fact {
                        value: value.clone(),
                        source: PersonId(3),
                        observed: SimTime(0),
                        revision: 0,
                        confidence: 90,
                    },
                )
            })
            .collect();
        let sources = (1..=bins)
            .map(|id| {
                let area = AreaId(id as u64);
                Source {
                    keys: [
                        format!("roads:{id}").into(),
                        format!("patch:{id}").into(),
                        format!("store:{id}").into(),
                        format!("store-location:{id}").into(),
                    ],
                    values: [
                        Arc::new(FactValue::Roads(Arc::new(game_land::Area {
                            name: format!("area {id}"),
                            links: Default::default(),
                        }))),
                        Arc::new(FactValue::Patch(game_ecology::Patch {
                            area,
                            good: game_cargo::FOOD,
                            stock: 1000,
                            capacity: 1000,
                            renew: 0,
                        })),
                        Arc::new(FactValue::Stock(Table::from([(game_cargo::FOOD, stock)]))),
                        Arc::new(FactValue::Location(StoreLocation {
                            area,
                            capacity: 1_000_000_000,
                            carrier: None,
                            mass: stock,
                        })),
                    ],
                }
            })
            .collect();
        Arc::new(Self {
            cold,
            sources,
            stock,
            rules: Rules::default(),
        })
    }
    pub fn bin(&self, id: PersonId) -> usize {
        id.0 as usize % self.sources.len()
    }
    pub fn source(&self, id: PersonId) -> &Source {
        &self.sources[self.bin(id)]
    }
    pub fn actor(&self, id: PersonId, at: SimTime) -> Actor {
        Actor {
            id,
            body: Body {
                kind: if id.0.is_multiple_of(19) {
                    Kind::Guardian
                } else {
                    Kind::Mortal
                },
                born: at,
                alive: true,
                hunger: id.0 % 25,
                fatigue: id.0 % 10,
                health: 100,
                resting: false,
                last: at,
                life_limit: None,
                maximum_health: None,
                fatigue_fraction: 0,
                hunger_fraction: 0,
                recovery_fraction: 0,
                metabolic_rates: Default::default(),
            },
            mind: Mind::default(),
            facts: std::array::from_fn(|i| Fact {
                value: self.source(id).values[i].clone(),
                source: id,
                observed: at,
                revision: 0,
                confidence: 100,
            }),
        }
    }
}
pub trait Store: Sized {
    type Mark;
    type Reader;
    fn new(world: Arc<World>, actors: Vec<Actor>, stock: Vec<u64>) -> Self;
    fn world(&self) -> &Arc<World>;
    fn ids(&self) -> Vec<PersonId>;
    fn body(&self, id: PersonId) -> Body;
    fn mind(&self, id: PersonId) -> Mind;
    fn fact(&self, id: PersonId, key: usize) -> Fact;
    fn set_body(&mut self, id: PersonId, body: Body);
    fn set_mind(&mut self, id: PersonId, mind: Mind);
    fn set_fact(&mut self, id: PersonId, key: usize, fact: Fact);
    fn stock(&self, bin: usize) -> u64;
    fn set_stock(&mut self, bin: usize, stock: u64);
    fn mark(&mut self) -> Self::Mark;
    fn rollback(&mut self, mark: Self::Mark);
    fn commit(&mut self, mark: Self::Mark);
    fn validate(&self, mark: &Self::Mark);
    fn advance(&mut self, pool: &rayon::ThreadPool, at: SimTime);
    fn remove(&mut self, id: PersonId);
    fn add(&mut self, actor: Actor);
    fn freeze(&self) -> Self::Reader;
    fn read(reader: &Self::Reader) -> Vec<Actor>;
    fn actor(&self, id: PersonId) -> Actor {
        Actor {
            id,
            body: self.body(id),
            mind: self.mind(id),
            facts: std::array::from_fn(|i| self.fact(id, i)),
        }
    }
    fn actors(&self) -> Vec<Actor> {
        self.ids().into_iter().map(|id| self.actor(id)).collect()
    }
}
#[derive(Clone)]
pub struct Persistent {
    world: Arc<World>,
    bodies: Table<PersonId, Body>,
    minds: Table<PersonId, Mind>,
    facts: Table<PersonId, Table<Arc<str>, Fact>>,
    stock: Table<usize, u64>,
}
impl Store for Persistent {
    type Mark = Self;
    type Reader = Self;
    fn new(world: Arc<World>, actors: Vec<Actor>, stock: Vec<u64>) -> Self {
        let mut s = Self {
            world,
            bodies: Table::new(),
            minds: Table::new(),
            facts: Table::new(),
            stock: stock.into_iter().enumerate().collect(),
        };
        for a in actors {
            s.add(a);
        }
        s
    }
    fn world(&self) -> &Arc<World> {
        &self.world
    }
    fn ids(&self) -> Vec<PersonId> {
        self.bodies.keys().copied().collect()
    }
    fn body(&self, id: PersonId) -> Body {
        self.bodies[&id]
    }
    fn mind(&self, id: PersonId) -> Mind {
        self.minds[&id]
    }
    fn fact(&self, id: PersonId, key: usize) -> Fact {
        self.facts[&id][self.world.source(id).keys[key].as_ref()].clone()
    }
    fn set_body(&mut self, id: PersonId, body: Body) {
        *self.bodies.get_mut(&id).unwrap() = body;
    }
    fn set_mind(&mut self, id: PersonId, mind: Mind) {
        *self.minds.get_mut(&id).unwrap() = mind;
    }
    fn set_fact(&mut self, id: PersonId, key: usize, fact: Fact) {
        let old = self
            .facts
            .get_mut(&id)
            .unwrap()
            .get_mut(self.world.source(id).keys[key].as_ref())
            .unwrap();
        Stamp::of(&fact).apply(old);
        if !Arc::ptr_eq(&old.value, &fact.value) {
            old.value = fact.value;
        }
    }
    fn stock(&self, bin: usize) -> u64 {
        self.stock[&bin]
    }
    fn set_stock(&mut self, bin: usize, stock: u64) {
        *self.stock.get_mut(&bin).unwrap() = stock;
    }
    fn mark(&mut self) -> Self::Mark {
        self.clone()
    }
    fn rollback(&mut self, mark: Self::Mark) {
        *self = mark;
    }
    fn commit(&mut self, _mark: Self::Mark) {}
    fn validate(&self, mark: &Self::Mark) {
        assert!(
            self.bodies
                .changed(&mark.bodies)
                .all(|(_, b)| b.health <= 100)
        );
        for (_, facts, old) in self.facts.changed_pairs(&mark.facts) {
            let empty = Table::new();
            assert!(
                facts
                    .changed(old.unwrap_or(&empty))
                    .all(|(_, f)| f.confidence <= 100)
            );
        }
    }
    fn advance(&mut self, pool: &rayon::ThreadPool, at: SimTime) {
        let mut next = self.bodies.clone();
        next.update(pool, |_, b| *b = advance_body(b, &self.world.rules, at));
        assert!(next.values().all(|b| b.health <= 100));
        self.bodies = next;
    }
    fn remove(&mut self, id: PersonId) {
        self.bodies.remove(&id).unwrap();
        self.minds.remove(&id).unwrap();
        self.facts.remove(&id).unwrap();
    }
    fn add(&mut self, a: Actor) {
        assert!(!self.bodies.contains_key(&a.id));
        let mut facts = self.world.cold.clone();
        for (key, fact) in self.world.source(a.id).keys.iter().cloned().zip(a.facts) {
            facts.insert(key, fact);
        }
        self.bodies.insert(a.id, a.body);
        self.minds.insert(a.id, a.mind);
        self.facts.insert(a.id, facts);
    }
    fn freeze(&self) -> Self::Reader {
        self.clone()
    }
    fn read(r: &Self::Reader) -> Vec<Actor> {
        r.actors()
    }
}
pub struct Packed {
    world: Arc<World>,
    ids: Vec<PersonId>,
    index: HashMap<PersonId, usize>,
    bodies: Vec<Body>,
    next: Vec<Body>,
    minds: Vec<Mind>,
    values: Vec<[Arc<FactValue>; HOT]>,
    stamps: Vec<[Stamp; HOT]>,
    stock: Vec<u64>,
    undo: Vec<Undo>,
    depth: usize,
}
enum Undo {
    Body(usize, Body),
    Mind(usize, Mind),
    Stamp(usize, usize, Stamp),
    Value(usize, usize, Arc<FactValue>),
    Stock(usize, u64),
    Added(PersonId),
    Removed(usize, Box<Actor>),
}
pub struct Mark {
    offset: usize,
    depth: usize,
}
impl Packed {
    fn slot(&self, id: PersonId) -> usize {
        self.index[&id]
    }
    fn add_raw(&mut self, a: Actor) {
        let slot = self.ids.len();
        assert!(self.index.insert(a.id, slot).is_none());
        self.ids.push(a.id);
        self.bodies.push(a.body);
        self.minds.push(a.mind);
        self.values
            .push(std::array::from_fn(|i| a.facts[i].value.clone()));
        self.stamps
            .push(std::array::from_fn(|i| Stamp::of(&a.facts[i])));
    }
    fn remove_raw(&mut self, id: PersonId) -> (usize, Actor) {
        let slot = self.slot(id);
        let a = self.actor(id);
        self.index.remove(&id);
        self.ids.swap_remove(slot);
        self.bodies.swap_remove(slot);
        self.minds.swap_remove(slot);
        self.values.swap_remove(slot);
        self.stamps.swap_remove(slot);
        if slot < self.ids.len() {
            self.index.insert(self.ids[slot], slot);
        }
        (slot, a)
    }
    fn restore(&mut self, slot: usize, a: Actor) {
        let last = self.ids.len();
        self.add_raw(a);
        self.ids.swap(slot, last);
        self.bodies.swap(slot, last);
        self.minds.swap(slot, last);
        self.values.swap(slot, last);
        self.stamps.swap(slot, last);
        self.index.insert(self.ids[slot], slot);
        self.index.insert(self.ids[last], last);
    }
}
impl Store for Packed {
    type Mark = Mark;
    type Reader = Vec<Actor>;
    fn new(world: Arc<World>, mut actors: Vec<Actor>, stock: Vec<u64>) -> Self {
        let n = actors.len();
        actors.reverse();
        let mut s = Self {
            world,
            ids: Vec::with_capacity(n),
            index: HashMap::with_capacity(n),
            bodies: Vec::with_capacity(n),
            next: Vec::with_capacity(n),
            minds: Vec::with_capacity(n),
            values: Vec::with_capacity(n),
            stamps: Vec::with_capacity(n),
            stock,
            undo: Vec::new(),
            depth: 0,
        };
        for a in actors {
            s.add_raw(a);
        }
        s
    }
    fn world(&self) -> &Arc<World> {
        &self.world
    }
    fn ids(&self) -> Vec<PersonId> {
        let mut ids = self.ids.clone();
        ids.sort_unstable();
        ids
    }
    fn body(&self, id: PersonId) -> Body {
        self.bodies[self.slot(id)]
    }
    fn mind(&self, id: PersonId) -> Mind {
        self.minds[self.slot(id)]
    }
    fn fact(&self, id: PersonId, key: usize) -> Fact {
        let i = self.slot(id);
        self.stamps[i][key].fact(self.values[i][key].clone())
    }
    fn set_body(&mut self, id: PersonId, body: Body) {
        let i = self.slot(id);
        self.undo.push(Undo::Body(i, self.bodies[i]));
        self.bodies[i] = body;
    }
    fn set_mind(&mut self, id: PersonId, mind: Mind) {
        let i = self.slot(id);
        self.undo.push(Undo::Mind(i, self.minds[i]));
        self.minds[i] = mind;
    }
    fn set_fact(&mut self, id: PersonId, key: usize, fact: Fact) {
        let i = self.slot(id);
        self.undo.push(Undo::Stamp(i, key, self.stamps[i][key]));
        self.stamps[i][key] = Stamp::of(&fact);
        if !Arc::ptr_eq(&self.values[i][key], &fact.value) {
            self.undo
                .push(Undo::Value(i, key, self.values[i][key].clone()));
            self.values[i][key] = fact.value;
        }
    }
    fn stock(&self, bin: usize) -> u64 {
        self.stock[bin]
    }
    fn set_stock(&mut self, bin: usize, stock: u64) {
        self.undo.push(Undo::Stock(bin, self.stock[bin]));
        self.stock[bin] = stock;
    }
    fn mark(&mut self) -> Self::Mark {
        self.depth += 1;
        Mark {
            offset: self.undo.len(),
            depth: self.depth,
        }
    }
    fn rollback(&mut self, m: Self::Mark) {
        assert_eq!(self.depth, m.depth);
        while self.undo.len() > m.offset {
            match self.undo.pop().unwrap() {
                Undo::Body(i, b) => self.bodies[i] = b,
                Undo::Mind(i, v) => self.minds[i] = v,
                Undo::Stamp(i, k, v) => self.stamps[i][k] = v,
                Undo::Value(i, k, v) => self.values[i][k] = v,
                Undo::Stock(i, v) => self.stock[i] = v,
                Undo::Added(id) => {
                    assert_eq!(self.slot(id), self.ids.len() - 1);
                    self.remove_raw(id);
                }
                Undo::Removed(i, a) => self.restore(i, *a),
            }
        }
        self.depth -= 1;
    }
    fn commit(&mut self, m: Self::Mark) {
        assert_eq!(self.depth, m.depth);
        self.depth -= 1;
        if self.depth == 0 {
            self.undo.clear();
        }
    }
    fn validate(&self, m: &Self::Mark) {
        for u in &self.undo[m.offset..] {
            match u {
                Undo::Body(i, _) => assert!(self.bodies[*i].health <= 100),
                Undo::Stamp(i, k, _) => assert!(self.stamps[*i][*k].confidence <= 100),
                _ => {}
            }
        }
    }
    fn advance(&mut self, pool: &rayon::ThreadPool, at: SimTime) {
        assert_eq!(self.depth, 0);
        self.next.clear();
        self.next.extend_from_slice(&self.bodies);
        if pool.current_num_threads() == 1 {
            for b in &mut self.next {
                *b = advance_body(b, &self.world.rules, at);
            }
        } else {
            pool.install(|| {
                self.next
                    .par_iter_mut()
                    .for_each(|b| *b = advance_body(b, &self.world.rules, at))
            });
        }
        assert!(self.next.iter().all(|b| b.health <= 100));
        std::mem::swap(&mut self.bodies, &mut self.next);
    }
    fn remove(&mut self, id: PersonId) {
        let (i, a) = self.remove_raw(id);
        self.undo.push(Undo::Removed(i, Box::new(a)));
    }
    fn add(&mut self, a: Actor) {
        let id = a.id;
        self.add_raw(a);
        self.undo.push(Undo::Added(id));
    }
    fn freeze(&self) -> Self::Reader {
        self.actors()
    }
    fn read(r: &Self::Reader) -> Vec<Actor> {
        r.clone()
    }
}
pub fn normalize(world: &World, actors: &mut [Actor]) {
    for a in actors {
        let facts: Facts = world
            .source(a.id)
            .keys
            .iter()
            .cloned()
            .zip(a.facts.clone())
            .collect();
        a.facts = std::array::from_fn(|i| {
            facts
                .get(world.source(a.id).keys[i].as_ref())
                .unwrap()
                .to_owned()
        });
    }
}
