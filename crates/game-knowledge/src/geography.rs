use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreLocation {
    pub area: AreaId,
    pub carrier: Option<PersonId>,
    pub capacity: u64,
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
pub fn known<T: serde::de::DeserializeOwned>(
    facts: &BTreeMap<String, Fact>,
    key: &str,
) -> Option<T> {
    T::deserialize(facts.get(key)?.value.as_ref()).ok()
}
pub(super) fn remember<T: Serialize>(
    tx: &mut Transaction<'_>,
    person: PersonId,
    key: String,
    data: &T,
) -> Result<()> {
    let fact = Fact {
        value: serde_json::to_value(data)
            .map_err(|e| Error::Rejected(e.to_string()))?
            .into(),
        source: person,
        observed: tx.now(),
        revision: tx.revision_after(),
        confidence: 100,
    };
    tx.edit::<Knowledge>()?
        .0
        .entry(person)
        .or_default()
        .insert(key, fact);
    Ok(())
}
/// Construct only the graph represented in this person's observations and reports.
/// Destination placeholders allow taking a known road into an unvisited area.
pub fn route(
    facts: &BTreeMap<String, Fact>,
    from: AreaId,
    to: AreaId,
) -> Result<game_spatial::Route> {
    if from == to {
        return Ok(game_spatial::Route {
            areas: vec![from],
            duration: SimDuration(0),
        });
    }
    let mut land = game_land::Land::default();
    for key in facts.keys() {
        let Some(id) = key
            .strip_prefix("roads:")
            .and_then(|id| id.parse::<u64>().ok())
        else {
            continue;
        };
        if let Some(area) = known::<game_land::Area>(facts, key) {
            land.0.insert(AreaId(id), area);
        }
    }
    let endpoints: Vec<_> = land
        .0
        .values()
        .flat_map(|a| a.links.keys().copied())
        .collect();
    for id in endpoints {
        land.0.entry(id).or_insert_with(|| game_land::Area {
            name: String::new(),
            links: BTreeMap::new(),
        });
    }
    game_spatial::route(&land, from, to)
}
#[derive(Serialize, Deserialize)]
pub struct ObserveSurroundings {
    pub person: PersonId,
}
impl Request for ObserveSurroundings {
    const KEY: &'static str = "knowledge.observe_surroundings";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
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
        let area = game_transport::area(tx.get()?, r.person)?;
        let roads = tx
            .get::<game_land::Land>()?
            .0
            .get(&area)
            .ok_or_else(|| Error::Missing("area".into()))?
            .clone();
        remember(tx, r.person, roads_key(area), &roads)?;
        // Patches here are exposed resources; distant patches are never enumerated into memory.
        let patches: Vec<_> = tx
            .get::<game_ecology::Ecology>()
            .map(|s| {
                s.patches
                    .iter()
                    .filter(|(_, p)| p.area == area)
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        for patch in patches {
            tx.call(ObservePatch {
                person: r.person,
                patch,
            })?;
        }
        // Inspect newly encountered accessible containers; refreshing a known stock
        // requires deliberate inspection, so remembered stock can still become stale.
        let inventories: Vec<_> = tx
            .get::<game_property::Property>()?
            .permitted(r.person)
            .filter(|id| {
                tx.get::<game_property::Property>()
                    .is_ok_and(|p| p.allowed(r.person, *id))
                    && game_cargo::locality(tx, *id).is_ok_and(|a| a == area)
                    && tx.get::<Knowledge>().is_ok_and(|k| {
                        k.0.get(&r.person)
                            .is_none_or(|m| !m.contains_key(&store_key(*id)))
                    })
            })
            .collect();
        for inventory in inventories {
            tx.call(ObserveStore {
                person: r.person,
                inventory,
            })?;
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
        remember(tx, r.person, patch_key(r.patch), &patch)
    })
}

/// Choose the next known road, including resuming an interrupted journey.
pub fn next_step(
    facts: &BTreeMap<String, Fact>,
    location: &game_transport::Location,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    use game_transport::Location;
    match location {
        Location::At(area) => {
            let route = route(facts, *area, target)?;
            Ok((route.areas.get(1).copied(), route.duration))
        }
        Location::Road {
            from,
            to,
            elapsed,
            duration,
        } => [
            (*from, elapsed.0),
            (
                *to,
                duration
                    .0
                    .checked_sub(elapsed.0)
                    .ok_or_else(|| Error::Rejected("invalid road progress".into()))?,
            ),
        ]
        .into_iter()
        .filter_map(|(area, offset)| {
            let route = route(facts, area, target).ok()?;
            Some((
                Some(area),
                SimDuration(offset.checked_add(route.duration.0)?),
            ))
        })
        .min_by_key(|(area, cost)| (cost.0, *area))
        .ok_or_else(|| Error::Rejected("no known route from road".into())),
        _ => Err(Error::Rejected("already travelling".into())),
    }
}
