//! Local company, mutual consent, remembered encounters, and finite conversations.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod conversation;
mod persistence;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub duration: SimDuration,
    pub contact_interval: SimDuration,
    pub retry: SimDuration,
    pub max_hunger: u64,
    pub max_fatigue: u64,
    pub candidates: usize,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            duration: SimDuration(5 * MINUTE.0),
            contact_interval: HOUR,
            retry: SimDuration(10 * MINUTE.0),
            max_hunger: 30,
            max_fatigue: 10,
            candidates: 16,
        }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<()> {
        require(
            self.duration.0 > 0
                && self.contact_interval.0 > 0
                && self.retry.0 > 0
                && self.max_hunger > 0
                && self.max_fatigue > 0
                && (1..=64).contains(&self.candidates),
            "invalid social rules",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub last_contact: SimTime,
    pub retry_at: SimTime,
    pub recent: Option<PersonId>,
    pub encounters: u64,
    pub area: Option<AreaId>,
    pub cursor: Option<PersonId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialLife {
    pub people: Table<PersonId, Person>,
    pub rules: Rules,
    pub meetings: Table<PersonId, conversation::Meeting>,
    pub residents: Table<AreaId, Table<PersonId, ()>>,
}
impl Resource for SocialLife {
    const KEY: &'static str = "social-life.people";
    fn validate(&self) -> Result<()> {
        persistence::validate(self)
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.rules.validate()?;
        for (_, p) in self.people.changed(&previous.people) {
            require(!p.name.trim().is_empty(), "empty personal name")?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure(pub Rules);
impl Request for Configure {
    const KEY: &'static str = "social-life.configure";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Registered {
    pub person: PersonId,
    pub name: String,
}
impl Event for Registered {
    const KEY: &'static str = "social-life.registered";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rename {
    pub person: PersonId,
    pub name: String,
}
impl Request for Rename {
    const KEY: &'static str = "social-life.rename";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Renamed {
    pub person: PersonId,
    pub name: String,
}
impl Event for Renamed {
    const KEY: &'static str = "social-life.renamed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct NameFor {
    pub observer: PersonId,
    pub person: PersonId,
    pub fallback: String,
}
impl Request for NameFor {
    const KEY: &'static str = "social-life.name-for";
    type Response = String;
}
impl Request for Register {
    const KEY: &'static str = "social-life.register";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Talk {
    pub person: PersonId,
    pub other: PersonId,
}
impl Request for Talk {
    const KEY: &'static str = "social-life.talk";
    type Response = SimTime;
}
/// The autonomy owner supplies the player-controlled person, who never consents implicitly.
#[derive(Clone, Serialize, Deserialize)]
pub struct SeekCompany {
    pub person: PersonId,
    pub protected: Option<PersonId>,
}
impl Request for SeekCompany {
    const KEY: &'static str = "social-life.seek";
    type Response = bool;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Encounter {
    pub first: PersonId,
    pub second: PersonId,
}
impl Event for Encounter {
    const KEY: &'static str = "social-life.encounter";
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbor {
    pub person: PersonId,
    pub name: String,
    pub action: Option<String>,
}
pub fn neighbors(host: &Host, observer: PersonId) -> Result<Vec<Neighbor>> {
    let Ok(area) = game_transport::area(host.get()?, observer) else {
        return Ok(vec![]);
    };
    let state = host.get::<SocialLife>()?;
    Ok(state
        .residents
        .get(&area)
        .into_iter()
        .flat_map(|r| r.keys())
        .filter(|p| **p != observer)
        .map(|p| Neighbor {
            person: *p,
            name: host
                .query(NameFor {
                    observer,
                    person: *p,
                    fallback: state.people[p].name.clone(),
                })
                .unwrap_or_else(|_| state.people[p].name.clone()),
            action: host
                .get::<Actions>()
                .ok()
                .and_then(|a| a.0.get(p).map(|a| a.kind.clone())),
        })
        .collect())
}
fn relocate(tx: &mut Transaction<'_>, person: PersonId, area: Option<AreaId>) -> Result<()> {
    let state = tx.edit::<SocialLife>()?;
    if let Some(p) = state.people.get_mut(&person) {
        if let Some(old) = p.area {
            let residents = state.residents.get_mut(&old).unwrap();
            residents.remove(&person);
            if residents.is_empty() {
                state.residents.remove(&old);
            }
        }
        p.area = area;
        if let Some(area) = area {
            state.residents.entry(area).or_default().insert(person, ());
        }
    }
    Ok(())
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Rules> {
    let doc: game_content::Document<Rules> = store.load(&format!("social-life/{key}.toml"))?;
    doc.data.validate()?;
    Ok(doc.data)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("social-life", 1)?;
    b.resource::<SocialLife>()?;
    b.validate_save(persistence::validate_host);
    b.request::<Configure>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<SocialLife>()?.people.is_empty(),
            "social life already initialized",
        )?;
        tx.edit::<SocialLife>()?.rules = r.0;
        Ok(())
    })?;
    b.request::<Register>(|tx, r| {
        require(!r.name.trim().is_empty(), "empty personal name")?;
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            !tx.get::<SocialLife>()?.people.contains_key(&r.person),
            "person already registered",
        )?;
        let now = tx.now();
        tx.edit::<SocialLife>()?.people.insert(
            r.person,
            Person {
                name: r.name.clone(),
                last_contact: now,
                retry_at: now,
                recent: None,
                encounters: 0,
                area: None,
                cursor: Some(r.person),
            },
        );
        relocate(tx, r.person, Some(area))?;
        tx.emit(
            Registered {
                person: r.person,
                name: r.name,
            },
            vec![r.person],
        )
    })?;
    b.request::<Rename>(|tx, r| {
        require(!r.name.trim().is_empty(), "empty personal name")?;
        tx.edit::<SocialLife>()?
            .people
            .get_mut(&r.person)
            .ok_or_else(|| Error::Missing("social identity".into()))?
            .name = r.name.clone();
        tx.emit(
            Renamed {
                person: r.person,
                name: r.name,
            },
            vec![r.person],
        )
    })?;
    b.before::<game_transport::Move>("social-life.depart", |tx, r| relocate(tx, r.person, None))?;
    b.before::<game_transport::Carry>("social-life.carried", |tx, r| {
        relocate(tx, r.passenger, None)
    })?;
    b.subscribe::<game_transport::Arrived>("social-life.arrive", |tx, e| {
        relocate(tx, e.person, Some(e.area))
    })?;
    b.subscribe::<game_physiology::Died>("social-life.death", |tx, e| {
        relocate(tx, e.person, None)
    })?;
    conversation::register(b)
}

/// A personal motive, not an action lock: loneliness and sociability compete with other interests.
pub fn company_motivation(tx: &Transaction<'_>, person: PersonId) -> i64 {
    let Ok(state) = tx.get::<SocialLife>() else {
        return 0;
    };
    let Some(p) = state.people.get(&person) else {
        return 0;
    };
    let sociability = game_vitality::disposition(tx, person, "sociability");
    let fellowship = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&person)?.get("fellowship"))
        .map_or(0, |value| i64::from(*value));
    let elapsed = tx.now().0.saturating_sub(p.last_contact.0);
    let scale = state.rules.contact_interval.0.max(1);
    ((u128::from(elapsed) * (sociability + fellowship * 4).max(0) as u128) / u128::from(scale))
        .min(10000) as i64
}
