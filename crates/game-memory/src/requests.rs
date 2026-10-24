use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "V: Content")]
pub struct Receive<V: Content> {
    pub person: PersonId,
    pub key: String,
    pub fact: Fact<V>,
}
impl<V: Content> Request for Receive<V> {
    const KEY: &'static str = "knowledge.receive";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "V: Content")]
pub struct Initialize<V: Content> {
    pub person: PersonId,
    pub facts: Facts<V>,
}
impl<V: Content> Request for Initialize<V> {
    const KEY: &'static str = "knowledge.initialize";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "V: Content")]
pub struct ObserveScene<V: Content> {
    pub person: PersonId,
    pub scene: Arc<game_facts::Scene<V>>,
}
impl<V: Content> Request for ObserveScene<V> {
    const KEY: &'static str = "memory.observe_scene";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Recall {
    pub person: PersonId,
    pub key: String,
}
impl Request for Recall {
    const KEY: &'static str = "memory.recall";
    type Response = Option<serde_json::Value>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Significance {
    pub person: PersonId,
    pub key: String,
    pub importance: u16,
    pub emotion: i16,
}
impl Request for Significance {
    const KEY: &'static str = "memory.significance";
    type Response = ();
}
pub(super) fn register<V: Content>(b: &mut Builder) -> Result<()> {
    b.request::<Receive<V>>(|tx, r| {
        require(
            !r.key.is_empty()
                && r.key.len() <= 160
                && r.fact.confidence <= 100
                && r.fact.observed <= tx.now()
                && r.fact.revision <= tx.revision_after(),
            "invalid learned claim",
        )?;
        let old = tx
            .get::<Memories<V>>()?
            .0
            .get(&r.person)
            .and_then(|f| f.get(&r.key));
        if old
            .as_ref()
            .is_some_and(|f| (f.observed, f.confidence) > (r.fact.observed, r.fact.confidence))
        {
            return Ok(());
        }
        let same =
            old.is_some_and(|f| game_facts::same_value(f.value.as_ref(), r.fact.value.as_ref()));
        let key: Arc<str> = r.key.into();
        tx.edit::<Memories<V>>()?
            .0
            .entry(r.person)
            .or_default()
            .insert(key.clone(), r.fact);
        observation::record::<V>(tx, r.person, key, same)
    })?;
    b.request::<Initialize<V>>(|tx, r| {
        require(
            !tx.get::<Memories<V>>()?.0.contains_key(&r.person),
            "knowledge already initialized",
        )?;
        let keys: Vec<_> = r.facts.keys().map(|k| (k.clone(), false)).collect();
        tx.edit::<Memories<V>>()?.0.insert(r.person, r.facts);
        observation::record_many::<V>(tx, r.person, keys)
    })?;
    b.request::<ObserveScene<V>>(|tx, r| {
        let keys: Vec<_> = r
            .scene
            .entries()
            .map(|(k, v)| {
                let same = tx
                    .get::<Memories<V>>()
                    .ok()
                    .and_then(|s| s.0.get(&r.person)?.get(k))
                    .is_some_and(|f| game_facts::same_value(f.value.as_ref(), v.as_ref()));
                (k.clone(), same)
            })
            .collect();
        let stamp = game_facts::Stamp {
            source: r.person,
            observed: tx.now(),
            revision: tx.revision_after(),
            confidence: 100,
        };
        tx.edit::<Memories<V>>()?
            .0
            .entry(r.person)
            .or_default()
            .observe(r.scene, stamp);
        observation::record_many::<V>(tx, r.person, keys)
    })?;
    b.request::<Significance>(|tx, r| {
        require(
            r.importance <= 1000 && r.emotion.unsigned_abs() <= 1000,
            "invalid memory significance",
        )?;
        let trace = tx
            .edit::<Memories<V>>()?
            .1
            .get_mut(&r.person)
            .and_then(|b| b.entries.get_mut(r.key.as_str()));
        if let Some(t) = trace {
            t.importance = t.importance.max(r.importance);
            if r.emotion.unsigned_abs() > t.emotion.unsigned_abs() {
                t.emotion = r.emotion;
            }
        }
        Ok(())
    })?;
    b.request::<Recall>(|tx, r| {
        retention::prune::<V>(tx, r.person)?;
        let Some(mut fact) = tx
            .get::<Memories<V>>()?
            .0
            .get(&r.person)
            .and_then(|f| f.get(&r.key))
            .map(|f| f.to_owned())
        else {
            return Ok(None);
        };
        let reliability = game_vitality::ability(tx, r.person, "memory_reliability").min(4000);
        let now = tx.now();
        let seed = tx.seed();
        if let Some(t) = tx
            .edit::<Memories<V>>()?
            .1
            .get_mut(&r.person)
            .and_then(|b| b.entries.get_mut(r.key.as_str()))
        {
            let salt = r.key.bytes().fold(r.person.0, |s, b| {
                s.wrapping_mul(131).wrapping_add(u64::from(b))
            });
            let draw = random(seed, salt, t.recalls);
            if t.expires.is_some()
                && (t.recalls == 0 || now.0.saturating_sub(t.rehearsed.0) >= HOUR.0)
            {
                t.importance = t.importance.saturating_add(200).min(1000);
                t.rehearsed = now;
            }
            t.recalls = t.recalls.saturating_add(1);
            if t.expires.is_none() {
                t.strength = t.accessibility(now).saturating_add(20).min(1000);
                t.rehearsed = now;
                if draw % 10000 < 100 - reliability * 90 / 4000
                    && let Some(value) = fact.value.reconstruct(draw / 10000)
                {
                    fact.value = Arc::new(value);
                    fact.confidence = fact.confidence.saturating_sub(1);
                }
            }
        }
        tx.edit::<Memories<V>>()?
            .0
            .get_mut(&r.person)
            .unwrap()
            .insert(r.key.into(), fact.clone());
        // Reconstruction changes remembered content, never its observation date or source.
        retention::schedule::<V>(tx, r.person)?;
        Ok(Some(
            serde_json::to_value(fact).map_err(|e| Error::InvalidSave(e.to_string()))?,
        ))
    })?;
    Ok(())
}
