//! Personal cooperation, accepted decision rights, and individually motivated allegiance.
mod organizing;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Communities {
    pub enabled: bool,
    pub members: Table<PersonId, InstitutionId>,
    pub places: Table<AreaId, Table<InstitutionId, ()>>,
    pub attachments: Table<PersonId, Attachment>,
    pub retry_at: Table<PersonId, SimTime>,
}
impl Resource for Communities {
    const KEY: &'static str = "communities.compacts";
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attachment {
    Independent,
    Cooperation,
    Voice,
    Loyalty(PersonId),
    Fear(PersonId),
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "communities.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Founded {
    pub founder: PersonId,
    pub companion: PersonId,
    pub institution: InstitutionId,
    pub name: String,
}
impl Event for Founded {
    const KEY: &'static str = "communities.founded";
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityView {
    pub institution: InstitutionId,
    pub name: String,
    pub members: BTreeSet<PersonId>,
    pub procedure: game_institutions::Procedure,
    pub attachments: BTreeMap<PersonId, Attachment>,
}
pub fn view(h: &Host, person: PersonId) -> Result<Vec<CommunityView>> {
    let area = h
        .get::<game_social_life::SocialLife>()?
        .people
        .get(&person)
        .and_then(|p| p.area);
    let state = h.get::<Communities>()?;
    Ok(area
        .into_iter()
        .flat_map(|a| state.places.get(&a))
        .flat_map(|ids| ids.keys().copied())
        .filter_map(|id| {
            let i = h
                .get::<game_institutions::Institutions>()
                .ok()?
                .groups
                .get(&id)?;
            (Some(i.jurisdiction) == area).then(|| CommunityView {
                institution: id,
                name: i.name.clone(),
                members: i.members.clone(),
                procedure: i.procedure.clone(),
                attachments: i
                    .members
                    .iter()
                    .filter(|p| **p == person)
                    .filter_map(|p| state.attachments.get(p).cloned().map(|a| (*p, a)))
                    .collect(),
            })
        })
        .collect())
}
fn supplies(tx: &Transaction<'_>, person: PersonId) -> Result<Option<InventoryId>> {
    let facts = tx.get::<game_knowledge::Knowledge>()?;
    let Some(facts) = facts.0.get(&person) else {
        return Ok(None);
    };
    // Search only the prospective member's known containers, then check actual custody.
    Ok(game_knowledge::with_prefix(facts, "store:").filter_map(|(key, _)| {
        let id = InventoryId(key.strip_prefix("store:")?.parse().ok()?);
        let property = tx.get::<game_property::Property>().ok()?;
        let cargo = tx.get::<game_cargo::Cargo>().ok()?;
        (property.claims.get(&id).is_some_and(|claims| claims.iter().any(|c| c.claimant == person))
            && matches!(cargo.0.get(&id)?.custody, game_cargo::Custody::Carried(p) if p == person))
            .then_some(id)
    }).next())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("communities", 1)?;
    b.resource::<Communities>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Communities>()?.enabled = true;
        Ok(())
    })?;
    b.validate_save(|h| {
        let state = h.get::<Communities>()?;
        require(
            state.enabled || (state.members.is_empty() && state.places.is_empty()),
            "disabled community records",
        )?;
        for (person, institution) in &state.members {
            require(
                state.attachments.get(person).is_some_and(|a| match a {
                    Attachment::Loyalty(p) | Attachment::Fear(p) => {
                        p != person && state.members.get(p) == Some(institution)
                    }
                    _ => true,
                }),
                "invalid personal attachment",
            )?;
            require(
                h.get::<game_institutions::Institutions>()?
                    .groups
                    .get(institution)
                    .is_some_and(|i| {
                        i.members.contains(person)
                            && state
                                .places
                                .get(&i.jurisdiction)
                                .is_some_and(|ids| ids.contains_key(institution))
                    }),
                "compact membership without institution",
            )?;
        }
        for (area, ids) in &state.places {
            for id in ids.keys() {
                require(
                    h.get::<game_institutions::Institutions>()?
                        .groups
                        .get(id)
                        .is_some_and(|i| {
                            i.jurisdiction == *area
                                && i.members.iter().all(|p| state.members.get(p) == Some(id))
                        }),
                    "invalid community locality index",
                )?;
            }
        }
        require(
            state
                .attachments
                .keys()
                .all(|p| state.members.contains_key(p)),
            "attachment without membership",
        )?;
        Ok(())
    });
    organizing::register(b)
}
