use super::*;
use std::borrow::Cow;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct StoreLocation {
    pub area: AreaId,
    pub capacity: u64,
    pub carrier: Option<PersonId>,
    pub mass: u64,
}
pub fn location_key(id: InventoryId) -> String {
    format!("store-location:{}", id.0)
}
pub fn roads_key(id: AreaId) -> String {
    format!("roads:{}", id.0)
}
pub fn patch_key(id: u64) -> String {
    format!("patch:{id}")
}
pub fn known<T: serde::de::DeserializeOwned + Clone + 'static>(
    facts: &Facts,
    key: &str,
) -> Option<T> {
    facts.get(key)?.value.decode()
}
pub(super) fn remember(
    tx: &mut Transaction<'_>,
    person: PersonId,
    subject: Subject,
    data: FactValue,
) -> Result<()> {
    let cached = tx.get::<Knowledge>()?.1.shared(&subject);
    let cached = if cached.as_ref().is_some_and(|c| c.value.as_ref() == &data) {
        cached.unwrap()
    } else {
        let key = cached
            .map(|c| c.key.clone())
            .unwrap_or_else(|| subject.key().into());
        tx.edit::<Knowledge>()?.1.insert(
            subject.clone(),
            Cached {
                key,
                value: Arc::new(data),
            },
        );
        tx.get::<Knowledge>()?.1.shared(&subject).unwrap()
    };
    let observed = tx.now();
    let revision = tx.revision_after();
    tx.edit::<Knowledge>()?
        .0
        .modify(person, Facts::default, |facts| {
            facts.insert(
                cached.key.clone(),
                Fact {
                    value: cached.value.clone(),
                    source: person,
                    observed,
                    revision,
                    confidence: 100,
                },
            );
        });
    Ok(())
}
/// Traverse recorded roads lazily. A known road may reach an endpoint whose
/// outgoing roads have not yet been observed.
pub fn route(facts: &Facts, from: AreaId, to: AreaId) -> Result<game_spatial::Route> {
    thread_local! {static ROUTES: std::cell::RefCell<game_spatial::Routes> = Default::default();}
    let version = facts.namespace_version("roads");
    ROUTES.with(|cache| {
        cache.borrow_mut().route(version, from, to, |id| {
            let fact = facts.get(roads_key(id).as_str())?;
            match fact.value.as_ref() {
                FactValue::Roads(area) => Some(Cow::Borrowed(area.as_ref())),
                _ => fact.value.decode::<game_land::Area>().map(Cow::Owned),
            }
        })
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObserveSurroundings {
    pub person: PersonId,
}
impl Request for ObserveSurroundings {
    const KEY: &'static str = "knowledge.observe_surroundings";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ObservePatch {
    pub person: PersonId,
    pub patch: u64,
}
impl Request for ObservePatch {
    const KEY: &'static str = "knowledge.observe_patch";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<ObserveSurroundings>(|tx, r| {
        let _measurement = game_diagnostics::span("observe surroundings");
        let area = game_transport::area(tx.get()?, r.person)?;
        if tx.has_before::<ObservePatch>() {
            let roads = tx
                .get::<game_land::Land>()?
                .0
                .shared(&area)
                .ok_or_else(|| Error::Missing("area".into()))?;
            remember(tx, r.person, Subject::Roads(area), FactValue::Roads(roads))?;
            let patches: Vec<_> = tx
                .get::<game_ecology::Ecology>()
                .map(|world| world.in_area(area).collect())
                .unwrap_or_default();
            for patch in patches {
                tx.call(ObservePatch {
                    person: r.person,
                    patch,
                })?;
            }
        } else {
            let scene = scenes::visible(tx, area)?;
            let stamp = game_facts::Stamp {
                source: r.person,
                observed: tx.now(),
                revision: tx.revision_after(),
                confidence: 100,
            };
            tx.edit::<Knowledge>()?
                .0
                .modify(r.person, Facts::default, |f| f.0.observe(scene, stamp));
        }
        // Inspect newly encountered accessible containers; refreshing a known stock
        // requires deliberate inspection, so remembered stock can still become stale.
        let empty = Table::new();
        let permitted = tx
            .get::<game_property::Property>()?
            .permitted_set(r.person)
            .unwrap_or(&empty);
        let memory = tx.get::<Knowledge>()?;
        let facts = &memory.0[&r.person];
        // Membership is immutable until permissions change. Once every permitted
        // store is known, container motion cannot introduce an unknown store.
        if facts.1.as_ref().is_some_and(|old| old.matches(permitted)) {
            return Ok(());
        }
        let mut all_known = true;
        let inventories: Vec<_> = permitted
            .keys()
            .copied()
            .filter(|id| {
                let known = memory
                    .1
                    .get(&Subject::Stock(*id))
                    .map(|cached| facts.get(cached.key.as_ref()))
                    .unwrap_or_else(|| facts.get(store_key(*id).as_str()))
                    .is_some_and(|f| matches!(f.value.as_ref(), FactValue::Stock(_)));
                if known {
                    return false;
                }
                let local = game_cargo::locality(tx, *id).is_ok_and(|a| a == area);
                all_known &= local;
                local
            })
            .collect();
        let completed = (all_known && !permitted.is_empty()).then(|| permitted.version());
        for inventory in inventories {
            tx.call(ObserveStore {
                person: r.person,
                inventory,
            })?;
        }
        if let Some(version) = completed {
            tx.edit::<Knowledge>()?.0.get_mut(&r.person).unwrap().1 = Some(version);
        }
        Ok(())
    })?;
    b.request::<ObservePatch>(|tx, r| {
        let patch = tx
            .get::<game_ecology::Ecology>()?
            .patches
            .get(&r.patch)
            .ok_or_else(|| Error::Missing("patch".into()))?
            .clone();
        require(
            game_transport::area(tx.get()?, r.person)? == patch.area,
            "cannot observe distant patch",
        )?;
        let subject = Subject::Patch(r.patch);
        remember(tx, r.person, subject, FactValue::Patch(patch))
    })
}
