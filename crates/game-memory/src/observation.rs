use super::*;
use game_memory_state::CueIndex;

struct Context {
    now: SimTime,
    end: SimTime,
    emotion: i16,
    values: Option<Table<Arc<str>, i32>>,
}
impl Context {
    fn new(tx: &Transaction<'_>, p: PersonId) -> Result<Self> {
        Ok(Self {
            now: tx.now(),
            end: tx.now().after(retention::duration(tx, p))?,
            emotion: game_vitality::condition(tx, p).map_or(0, |(stress, satisfaction)| {
                if stress > 500 {
                    -(stress as i16)
                } else {
                    satisfaction.clamp(-1000, 1000) as i16
                }
            }),
            values: tx
                .get::<game_relationships::Social>()
                .ok()
                .and_then(|s| s.values.get(&p))
                .cloned(),
        })
    }
}

pub(super) fn record<V: Content>(
    tx: &mut Transaction<'_>,
    p: PersonId,
    key: Arc<str>,
    same: bool,
) -> Result<()> {
    record_many::<V>(tx, p, vec![(key, same)])
}

pub(super) fn record_many<V: Content>(
    tx: &mut Transaction<'_>,
    p: PersonId,
    keys: Vec<(Arc<str>, bool)>,
) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let context = Context::new(tx, p)?;
    static INDEX: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let indexed = *INDEX.get_or_init(|| std::env::var("LEGACY_MEMORY_INDEX").as_deref() == Ok("1"));
    let mut index = (indexed && keys.len() >= 32).then(|| {
        tx.get::<Memories<V>>()
            .unwrap()
            .1
            .get(&p)
            .map(CueIndex::new)
            .unwrap_or_default()
    });
    let state = tx.edit::<Memories<V>>()?;
    let facts = &state.0[&p];
    let book = state.1.entry(p).or_default();
    for (key, same) in keys {
        note(facts, book, key, same, &context, index.as_mut());
    }
    retention::finish_batch::<V>(tx, p)
}

fn note<V: Content>(
    facts: &Facts<V>,
    book: &mut Book,
    key: Arc<str>,
    same: bool,
    context: &Context,
    index: Option<&mut CueIndex>,
) {
    if same && book.entries.get(&key).is_some_and(|t| t.expires.is_none()) {
        let t = book.entries.get_mut(&key).unwrap();
        t.strength = t.accessibility(context.now).saturating_add(20).min(1000);
        t.rehearsed = context.now;
        return;
    }
    let fact = facts.get(&key).unwrap();
    let mut cues = fact.value.cues(&key);
    cues.retain(|s| !s.is_empty() && s.len() <= 160);
    cues.sort();
    cues.dedup();
    cues.truncate(8);
    let interest = context.values.as_ref().map_or(100, |values| {
        cues.iter()
            .filter_map(|c| values.get(c.as_str()))
            .map(|n| (n.unsigned_abs() * 10).min(1000) as u16)
            .max()
            .unwrap_or(100)
    });
    let prior = book.entries.get(&key);
    let importance = prior.map_or(interest, |t| t.importance.max(interest));
    let related = if let Some(index) = index {
        let related = index.related(&key, &cues);
        index.replace(&key, prior.map_or(&[], |t| t.cues.as_slice()), &cues);
        related
    } else {
        book.entries
            .iter()
            .filter(|(k, t)| **k != key && t.cues.iter().any(|c| cues.contains(c)))
            .take(4)
            .map(|(k, _)| k.clone())
            .collect()
    };
    book.entries.insert(
        key,
        Trace {
            learned: context.now,
            rehearsed: context.now,
            expires: Some(context.end),
            importance,
            emotion: context.emotion,
            strength: 1000,
            recalls: 0,
            cues,
            related,
        },
    );
}
