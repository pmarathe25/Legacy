//! Chosen identities and the names particular speakers actually use.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
mod history;
pub use history::Changes;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Nickname,
    Title,
    Epithet,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    pub kind: Kind,
    pub at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub given: String,
    pub chosen_by: PersonId,
    pub changes: Changes,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Names {
    pub enabled: bool,
    pub identities: Records<PersonId, Identity>,
    pub heard: Table<PersonId, Table<PersonId, String>>,
    pub offers: Table<PersonId, Table<PersonId, String>>,
    pub usage: Table<PersonId, Table<PersonId, Vec<Alias>>>,
}
fn valid(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 80 && !name.chars().any(char::is_control)
}
impl Resource for Names {
    const KEY: &'static str = "names.identities";
    fn validate(&self) -> Result<()> {
        require(
            self.identities
                .values()
                .all(|i| valid(&i.given) && i.changes.iter().all(|(_, n)| valid(n)))
                && self
                    .heard
                    .values()
                    .all(|row| row.values().all(|n| valid(n)))
                && self.usage.values().all(|row| {
                    row.values()
                        .all(|aliases| aliases.len() <= 3 && aliases.iter().all(|a| valid(&a.name)))
                }),
            "invalid personal names",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (person, i) in self.identities.changed(&old.identities) {
            require(
                valid(&i.given)
                    && i.changes
                        .valid_since(old.identities.get(person).map(|i| &i.changes)),
                "invalid identity",
            )?;
        }
        for (_, row) in self.usage.changed(&old.usage) {
            require(
                row.values()
                    .all(|aliases| aliases.len() <= 3 && aliases.iter().all(|a| valid(&a.name))),
                "invalid aliases",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "names.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct NameChild {
    pub child: PersonId,
}
impl Request for NameChild {
    const KEY: &'static str = "names.child";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SuggestChild {
    pub chooser: PersonId,
    pub child: PersonId,
}
impl Request for SuggestChild {
    const KEY: &'static str = "names.suggest-child";
    type Response = String;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct LearnName {
    pub listener: PersonId,
    pub person: PersonId,
}
impl Request for LearnName {
    const KEY: &'static str = "names.learn";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub person: PersonId,
    pub name: String,
}
impl Request for Change {
    const KEY: &'static str = "names.change";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Label {
    pub speaker: PersonId,
    pub person: PersonId,
    pub name: String,
    pub kind: Kind,
}
impl Request for Label {
    const KEY: &'static str = "names.label";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Changed {
    pub person: PersonId,
    pub chooser: PersonId,
    pub name: String,
    pub given: bool,
}
impl Event for Changed {
    const KEY: &'static str = "names.changed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Called {
    pub speaker: PersonId,
    pub person: PersonId,
    pub name: String,
    pub kind: Kind,
}
impl Event for Called {
    const KEY: &'static str = "names.called";
}
fn presentable(tx: &Transaction<'_>, speaker: PersonId, listener: PersonId) -> bool {
    speaker == listener
        || !game_communication::active(tx)
        || (game_physiology::capable(tx, speaker).is_ok()
            && game_physiology::capable(tx, listener).is_ok()
            && game_vitality::can_converse(tx, speaker)
            && game_communication::ability(tx, speaker, "vocalization") >= 80
            && game_communication::ability(tx, listener, "hearing") >= 80
            && game_communication::ability(tx, listener, "interpretation") >= 80)
}
fn hear(tx: &mut Transaction<'_>, listener: PersonId, person: PersonId) -> Result<()> {
    hear_from(tx, listener, person, person)
}
fn hear_from(
    tx: &mut Transaction<'_>,
    listener: PersonId,
    person: PersonId,
    speaker: PersonId,
) -> Result<()> {
    if !presentable(tx, speaker, listener) {
        return Ok(());
    }
    if !tx.get::<Names>()?.identities.contains_key(&listener) {
        return Ok(());
    }
    if let Some(p) = tx
        .get::<game_social_life::SocialLife>()?
        .people
        .get(&person)
    {
        if tx
            .get::<Names>()?
            .heard
            .get(&listener)
            .and_then(|row| row.get(&person))
            == Some(&p.name)
        {
            return Ok(());
        }
        let name = p.name.clone();
        tx.edit::<Names>()?
            .heard
            .entry(listener)
            .or_default()
            .insert(person, name);
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("names", 1)?;
    b.resource::<Names>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Names>()?.enabled = true;
        Ok(())
    })?;
    b.request::<LearnName>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.listener)
                .ok()
                .is_some_and(|area| {
                    game_transport::area(tx.get::<game_transport::Positions>().unwrap(), r.person)
                        .ok()
                        == Some(area)
                }),
            "name was not heard locally",
        )?;
        hear(tx, r.listener, r.person)
    })?;
    b.subscribe::<game_social_life::Registered>("names.identity", |tx, e| {
        let at = tx.now();
        tx.edit::<Names>()?.identities.insert(
            e.person,
            Identity {
                given: e.name.clone(),
                chosen_by: e.person,
                changes: [(at, e.name.clone())].into_iter().collect(),
            },
        );
        hear(tx, e.person, e.person)
    })?;
    b.subscribe::<game_social_life::Renamed>("names.chosen", |tx, e| {
        let at = tx.now();
        tx.edit::<Names>()?
            .identities
            .get_mut(&e.person)
            .ok_or_else(|| Error::Missing("name history".into()))?
            .changes
            .push((at, e.name.clone()));
        hear(tx, e.person, e.person)
    })?;
    b.request::<game_social_life::NameFor>(|tx, r| {
        if !tx.get::<Names>()?.enabled {
            return Ok(r.fallback);
        }
        let names = tx.get::<Names>()?;
        Ok(names
            .usage
            .get(&r.observer)
            .and_then(|row| row.get(&r.person))
            .and_then(|a| a.last())
            .map(|a| a.name.clone())
            .or_else(|| names.heard.get(&r.observer)?.get(&r.person).cloned())
            .unwrap_or_else(|| "Unknown".into()))
    })?;
    b.request::<NameChild>(|tx, r| {
        let family = tx
            .get::<game_lifecycle::Population>()?
            .families
            .get(&r.child)
            .cloned()
            .ok_or_else(|| Error::Missing("child's family".into()))?;
        require(
            !tx.get::<game_social_life::SocialLife>()?
                .people
                .contains_key(&r.child),
            "child already named",
        )?;
        let chooser = [family.caregiver, family.mother, family.father]
            .into_iter()
            .find(|p| {
                tx.get::<Names>()
                    .is_ok_and(|n| n.identities.contains_key(p))
                    && game_physiology::capable(tx, *p).is_ok()
                    && game_transport::area(tx.get::<game_transport::Positions>().unwrap(), *p).ok()
                        == game_transport::area(
                            tx.get::<game_transport::Positions>().unwrap(),
                            r.child,
                        )
                        .ok()
            });
        let chooser = chooser.ok_or_else(|| {
            Error::Rejected("no parent or caregiver present to name child".into())
        })?;
        let name = if tx.registered::<SuggestChild>() {
            tx.call(SuggestChild {
                chooser,
                child: r.child,
            })?
        } else {
            tx.get::<game_social_life::SocialLife>()?.people[&chooser]
                .name
                .clone()
        };
        tx.call(game_social_life::Register {
            person: r.child,
            name: name.clone(),
        })?;
        tx.edit::<Names>()?
            .identities
            .get_mut(&r.child)
            .unwrap()
            .chosen_by = chooser;
        for parent in [family.mother, family.father, family.caregiver] {
            if game_transport::area(tx.get()?, parent).ok()
                == game_transport::area(tx.get()?, r.child).ok()
            {
                hear_from(tx, parent, r.child, chooser)?;
                hear(tx, r.child, parent)?;
            }
        }
        tx.emit(
            Changed {
                person: r.child,
                chooser,
                name,
                given: true,
            },
            if presentable(tx, chooser, r.child) {
                vec![chooser, r.child]
            } else {
                vec![chooser]
            },
        )
    })?;
    b.request::<Change>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(valid(&r.name), "invalid chosen name")?;
        let old = tx
            .get::<game_social_life::SocialLife>()?
            .people
            .get(&r.person)
            .ok_or_else(|| Error::Missing("social identity".into()))?
            .name
            .clone();
        if old == r.name {
            return Ok(());
        }
        tx.call(game_social_life::Rename {
            person: r.person,
            name: r.name.clone(),
        })?;
        let area = game_transport::area(tx.get()?, r.person)?;
        let witnesses: Vec<_> = tx
            .get::<game_social_life::SocialLife>()?
            .residents
            .get(&area)
            .into_iter()
            .flat_map(|row| row.keys().copied())
            .filter(|p| presentable(tx, r.person, *p))
            .collect();
        for witness in &witnesses {
            hear(tx, *witness, r.person)?;
        }
        tx.emit(
            Changed {
                person: r.person,
                chooser: r.person,
                name: r.name,
                given: false,
            },
            witnesses,
        )
    })?;
    b.request::<Label>(|tx, r| {
        game_physiology::capable(tx, r.speaker)?;
        require(
            r.speaker != r.person && valid(&r.name),
            "invalid form of address",
        )?;
        require(
            tx.get::<Names>()?
                .heard
                .get(&r.speaker)
                .is_some_and(|row| row.contains_key(&r.person)),
            "speaker has not encountered this person",
        )?;
        let at = tx.now();
        let aliases = tx
            .edit::<Names>()?
            .usage
            .entry(r.speaker)
            .or_default()
            .entry(r.person)
            .or_default();
        if aliases.iter().any(|a| a.name == r.name && a.kind == r.kind) {
            return Ok(());
        }
        aliases.retain(|a| a.kind != r.kind);
        aliases.push(Alias {
            name: r.name.clone(),
            kind: r.kind.clone(),
            at,
        });
        let present = game_transport::area(tx.get()?, r.speaker)
            .ok()
            .is_some_and(|area| {
                game_transport::area(tx.get::<game_transport::Positions>().unwrap(), r.person).ok()
                    == Some(area)
            });
        if present && presentable(tx, r.speaker, r.person) && r.kind == Kind::Nickname {
            tx.edit::<Names>()?
                .offers
                .entry(r.person)
                .or_default()
                .insert(r.speaker, r.name.clone());
        }
        tx.emit(
            Called {
                speaker: r.speaker,
                person: r.person,
                name: r.name,
                kind: r.kind,
            },
            vec![r.speaker],
        )
    })?;
    b.validate_save(|h| {
        let names = h.get::<Names>()?;
        let social = h.get::<game_social_life::SocialLife>()?;
        require(
            names.identities.iter().all(|(p, i)| {
                social
                    .people
                    .get(p)
                    .is_some_and(|s| i.changes.last().is_some_and(|(_, n)| n == &s.name))
            }),
            "name history differs from chosen name",
        )?;
        require(
            names.heard.iter().all(|(p, row)| {
                names.identities.contains_key(p)
                    && row.keys().all(|other| names.identities.contains_key(other))
            }) && names.usage.iter().all(|(p, row)| {
                row.keys().all(|other| {
                    names
                        .heard
                        .get(p)
                        .is_some_and(|known| known.contains_key(other))
                })
            }),
            "name usage without an encounter",
        )
    });
    Ok(())
}
