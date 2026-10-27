use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

static OUTPUT: OnceLock<Option<Mutex<BufWriter<File>>>> = OnceLock::new();
static FAILED: AtomicBool = AtomicBool::new(false);
static IDS: AtomicU64 = AtomicU64::new(1);
fn output() -> Option<&'static Mutex<BufWriter<File>>> {
    OUTPUT
        .get_or_init(|| {
            let path = std::env::var_os("LEGACY_REVIEW_TRACE")?;
            match File::options().create_new(true).write(true).open(path) {
                Ok(file) => Some(Mutex::new(BufWriter::new(file))),
                Err(e) => {
                    eprintln!("review trace disabled: {e}");
                    None
                }
            }
        })
        .as_ref()
}
fn write(row: &Value) {
    if FAILED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(output) = output() {
        let result = (|| {
            let mut writer = output.lock().unwrap_or_else(|p| p.into_inner());
            serde_json::to_writer(&mut *writer, row)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            Ok::<_, Box<dyn std::error::Error>>(())
        })();
        if let Err(error) = result
            && !FAILED.swap(true, Ordering::Relaxed)
        {
            eprintln!("review trace stopped: {error}");
        }
    }
}
#[derive(Default)]
struct Context {
    stack: Vec<(&'static str, String)>,
    reviews: Vec<Value>,
    wakes: BTreeMap<u64, Vec<Value>>,
}
thread_local! { static CONTEXT: RefCell<Context> = RefCell::new(Context::default()); }
pub struct Scope(bool);
pub fn scope(kind: &'static str, key: &str) -> Scope {
    let active = output().is_some();
    if active {
        CONTEXT.with(|c| c.borrow_mut().stack.push((kind, key.to_owned())));
    }
    Scope(active)
}
impl Drop for Scope {
    fn drop(&mut self) {
        if self.0 {
            CONTEXT.with(|c| {
                c.borrow_mut().stack.pop();
            });
        }
    }
}
pub struct Review(Option<u64>);
impl Review {
    pub fn begin(person: u64, at: u64, revision: u64, before: impl FnOnce() -> Value) -> Self {
        if output().is_none() {
            return Self(None);
        }
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        let before = before();
        CONTEXT.with(|c| {
            let mut c = c.borrow_mut();
            let wakes = c.wakes.remove(&person).unwrap_or_default();
            let row = json!({"type":"review", "id":id, "person":person, "at":at,
                "attempt_revision":revision, "stack":c.stack, "causes":wakes, "before":before});
            c.reviews.push(row);
        });
        Self(Some(id))
    }
    pub fn annotate(&self, key: &str, value: impl FnOnce() -> Value) {
        if let Some(id) = self.0 {
            let value = value();
            CONTEXT.with(|c| {
                if let Some(row) = c
                    .borrow_mut()
                    .reviews
                    .iter_mut()
                    .rev()
                    .find(|r| r["id"] == id)
                {
                    row[key] = value;
                }
            });
        }
    }
}
impl Drop for Review {
    fn drop(&mut self) {
        if let Some(id) = self.0 {
            let row = CONTEXT.with(|c| {
                let mut c = c.borrow_mut();
                c.reviews
                    .iter()
                    .rposition(|r| r["id"] == id)
                    .map(|i| c.reviews.remove(i))
            });
            if let Some(row) = row {
                write(&row);
            }
        }
    }
}
pub fn wake(person: u64, at: u64, suppressed: bool, hunger_only: bool, immediate: bool) {
    if output().is_none() {
        return;
    }
    let row = CONTEXT.with(|c| {
        let mut c = c.borrow_mut();
        let row = json!({"type":"wake", "person":person, "at":at, "suppressed":suppressed,
            "hunger_only":hunger_only, "immediate":immediate, "stack":c.stack,
            "during_review":c.reviews.last().map(|r| &r["id"])});
        if !suppressed {
            if c.wakes.len() >= 4096 && !c.wakes.contains_key(&person) {
                c.wakes.pop_first();
            }
            let causes = c.wakes.entry(person).or_default();
            if causes.len() < 8 {
                causes.push(row.clone());
            }
        }
        row
    });
    write(&row);
}
/// Record an attempted operation and its enclosing request/listener stack.
pub fn point(label: &str, data: impl FnOnce() -> Value) {
    if output().is_none() {
        return;
    }
    let data = data();
    let row = CONTEXT.with(|c| {
        let c = c.borrow();
        json!({"type":"point", "label":label, "data":data, "stack":c.stack,
            "during_review":c.reviews.last().map(|r| &r["id"])})
    });
    write(&row);
}
pub fn candidates(person: u64, rows: impl Iterator<Item = (u64, [i64; 5], Option<u64>)>) {
    if output().is_none() {
        return;
    }
    let mut hash = 0xcbf29ce484222325u64;
    let mut count = 0;
    for (id, terms, continuation) in rows {
        for value in std::iter::once(id)
            .chain(terms.map(|n| n as u64))
            .chain([u64::from(continuation.is_some()), continuation.unwrap_or(0)])
        {
            for b in value.to_le_bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        }
        count += 1;
    }
    CONTEXT.with(|c| {
        if let Some(row) = c
            .borrow_mut()
            .reviews
            .iter_mut()
            .rev()
            .find(|r| r["person"] == person)
        {
            row["candidates"] = json!({"count":count, "numeric_hash":hash});
        }
    });
}
