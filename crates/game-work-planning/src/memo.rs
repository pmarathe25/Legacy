use super::*;
use game_storage::{Table, Version};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
const MAX_ENTRIES: usize = 512;
const MAX_BYTES: usize = 4 * 1024 * 1024;
type Profile = BTreeMap<String, i32>;
type Values = Table<Arc<str>, i32>;
struct Supply {
    id: InventoryId,
    capacity: u64,
    version: Version<GoodId, u64>,
}
struct Entry {
    seed: u64,
    intelligence: u64,
    goals: Vec<Goal>,
    patches: Weak<()>,
    recipes: Version<String, Arc<game_workshop::Recipe>>,
    objects: Weak<BTreeMap<GoodId, game_objects::Food>>,
    definitions: Option<Version<String, Arc<game_traits::Trait>>>,
    profile: Weak<Profile>,
    values: Option<Version<Arc<str>, i32>>,
    supplies: Vec<Supply>,
    result: Option<Plan>,
    used: u64,
    prepared: bool,
    bytes: usize,
}
struct Inputs<'a> {
    person: PersonId,
    seed: u64,
    area: AreaId,
    intelligence: u64,
    goals: &'a [Goal],
    patches: Weak<()>,
    workshop: &'a game_workshop::Workshop,
    objects: &'a game_objects::Objects,
    traits: Option<&'a game_traits::Traits>,
    profile: Weak<Profile>,
    values: Option<&'a Values>,
    supplies: Vec<(InventoryId, u64, game_knowledge::Stock)>,
}
impl Entry {
    fn matches(&self, i: &Inputs<'_>) -> bool {
        self.seed == i.seed
            && self.intelligence == i.intelligence
            && self.goals == i.goals
            && self.patches.ptr_eq(&i.patches)
            && i.workshop.recipes.matches(&self.recipes)
            && self.objects.as_ptr() == Arc::as_ptr(i.objects.food_rules())
            && match (&self.definitions, i.traits) {
                (None, None) => true,
                (Some(v), Some(t)) => t.definitions.matches(v),
                _ => false,
            }
            && self.profile.ptr_eq(&i.profile)
            && match (&self.values, i.values) {
                (None, None) => true,
                (Some(v), Some(t)) => v.matches(t),
                _ => false,
            }
            && self.supplies.len() == i.supplies.len()
            && self
                .supplies
                .iter()
                .zip(&i.supplies)
                .all(|(a, (id, cap, stock))| {
                    a.id == *id && a.capacity == *cap && a.version.matches(stock)
                })
    }
    fn new(i: Inputs<'_>, result: Option<Plan>, used: u64, prepared: bool) -> Self {
        let bytes = std::mem::size_of::<Self>()
            + i.goals
                .iter()
                .map(|g| {
                    std::mem::size_of::<Goal>()
                        + g.object.len()
                        + g.affinity.len()
                        + g.concept.len()
                })
                .sum::<usize>()
            + i.supplies.len() * std::mem::size_of::<Supply>()
            + result.as_ref().map_or(0, |p| {
                p.concept.len()
                    + p.steps
                        .iter()
                        .map(|s| {
                            std::mem::size_of::<Step>()
                                + match s {
                                    Step::Craft { recipe } => recipe.len(),
                                    _ => 0,
                                }
                        })
                        .sum::<usize>()
            });
        Self {
            seed: i.seed,
            intelligence: i.intelligence,
            goals: i.goals.to_vec(),
            patches: i.patches,
            recipes: i.workshop.recipes.version(),
            objects: Arc::downgrade(i.objects.food_rules()),
            definitions: i.traits.map(|t| t.definitions.version()),
            profile: i.profile,
            values: i.values.map(Table::version),
            supplies: i
                .supplies
                .into_iter()
                .map(|(id, capacity, stock)| Supply {
                    id,
                    capacity,
                    version: stock.version(),
                })
                .collect(),
            result,
            used,
            prepared,
            bytes,
        }
    }
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<(PersonId, AreaId), Entry>,
    clock: u64,
    bytes: usize,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> std::sync::MutexGuard<'static, Cache> {
    let slot = CACHE.get_or_init(Default::default);
    slot.lock().unwrap_or_else(|p| {
        let mut c = p.into_inner();
        *c = Cache::default();
        slot.clear_poison();
        c
    })
}
fn inputs<'a>(
    tx: &'a Transaction<'_>,
    p: PersonId,
    inventories: &[InventoryId],
    goals: &'a [Goal],
) -> Option<Inputs<'a>> {
    let facts = tx.get::<game_knowledge::Knowledge>().ok()?.0.get(&p)?;
    let area = game_transport::area(tx.get().ok()?, p).ok()?;
    let objects = tx.get::<game_objects::Objects>().ok()?;
    let supplies = game_food_memory::with_stores(p, facts, objects, |stores| {
        inventories
            .iter()
            .map(|id| {
                let s = stores.get(*id)?;
                Some((*id, s.location.as_ref()?.capacity, s.stock.clone()?))
            })
            .collect::<Option<Vec<_>>>()
    })?;
    let traits = tx.get::<game_traits::Traits>().ok();
    Some(Inputs {
        person: p,
        seed: tx.seed(),
        area,
        intelligence: game_vitality::ability(tx, p, "intelligence"),
        goals,
        patches: facts.namespace_version("patch"),
        workshop: tx.get().ok()?,
        objects,
        traits,
        profile: traits
            .and_then(|t| t.people.shared(&p))
            .map_or_else(Weak::new, |v| Arc::downgrade(&v)),
        values: tx
            .get::<game_relationships::Social>()
            .ok()
            .and_then(|s| s.values.get(&p)),
        supplies,
    })
}
pub fn choose(
    tx: &Transaction<'_>,
    p: PersonId,
    inventories: &[InventoryId],
    goals: &[Goal],
) -> Result<Option<Plan>> {
    let _measurement = game_diagnostics::span("work planning");
    let Some(input) = inputs(tx, p, inventories, goals) else {
        return uncached(tx, p, inventories, goals);
    };
    let key = (p, input.area);
    {
        let mut c = cache();
        c.clock = c.clock.saturating_add(1);
        let used = c.clock;
        if let Some(old) = c.entries.get_mut(&key)
            && old.matches(&input)
        {
            old.used = used;
            if old.prepared {
                old.prepared = false;
                let _reused = game_diagnostics::span("work preparation reused");
            }
            let _hit = game_diagnostics::span("work cache hit");
            return Ok(old.result.clone());
        }
        if c.entries.get(&key).is_some_and(|e| e.prepared) {
            let _stale = game_diagnostics::span("work preparation stale");
        }
    }
    let result = uncached(tx, p, inventories, goals)?;
    remember(input, result.clone(), false);
    Ok(result)
}
fn remember(input: Inputs<'_>, result: Option<Plan>, prepared: bool) {
    let key = (input.person, input.area);
    let mut c = cache();
    let entry = Entry::new(input, result, c.clock, prepared);
    if entry.bytes <= MAX_BYTES {
        if let Some(old) = c.entries.remove(&key) {
            c.bytes -= old.bytes;
        }
        while c.entries.len() >= MAX_ENTRIES || c.bytes + entry.bytes > MAX_BYTES {
            let oldest = c
                .entries
                .iter()
                .min_by_key(|(k, e)| (e.used, **k))
                .map(|(k, _)| *k)
                .unwrap();
            c.bytes -= c.entries.remove(&oldest).unwrap().bytes;
        }
        c.bytes += entry.bytes;
        c.entries.insert(key, entry);
    }
}

pub struct Query<'a> {
    pub person: PersonId,
    pub inventories: Vec<InventoryId>,
    pub goals: &'a [Goal],
}
/// Capture private inputs before workers search; retain only bounded results and weak versions.
pub fn prepare(tx: &Transaction<'_>, queries: &[Query<'_>]) {
    use rayon::prelude::*;
    let _span = game_diagnostics::span("work preparation batch");
    let drafts: Vec<_> = queries
        .iter()
        .filter_map(|q| {
            let versions = inputs(tx, q.person, &q.inventories, q.goals)?;
            if cache()
                .entries
                .get(&(q.person, versions.area))
                .is_some_and(|e| e.matches(&versions))
            {
                let _already = game_diagnostics::span("work preparation already cached");
                return None;
            }
            match super::search::capture(tx, q.person, &q.inventories, q.goals) {
                Ok(Some(input)) => Some((versions, input)),
                _ => {
                    let _failed = game_diagnostics::span("work preparation failed");
                    None
                }
            }
        })
        .collect();
    let run = |(versions, input): (Inputs<'_>, super::search::Inputs<'_>)| {
        let _search = game_diagnostics::span("work prepared search");
        if let Ok(result) = input.run() {
            remember(versions, result, true);
        } else {
            let _failed = game_diagnostics::span("work preparation failed");
        }
    };
    if drafts.len() < 2 || tx.compute().current_num_threads() == 1 {
        drafts.into_iter().for_each(run);
    } else {
        let _parallel = game_diagnostics::span("work preparation parallel batch");
        tx.compute()
            .install(|| drafts.into_par_iter().for_each(run));
    }
}
