//! Admission, voluntary exit, and succession depend on individual cooperation, loyalty, or fear.
use game_communities::Attachment;
use game_core::*;
use game_institutions::Procedure;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affiliation {
    pub next: Table<PersonId, SimTime>,
}
impl Resource for Affiliation {
    const KEY: &'static str = "affiliation.intentions";
}
fn accepts(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> bool {
    let tie = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&other));
    tie.is_some_and(|r| r.affection >= 20 && r.trust >= 5 && r.grievance < 15 && r.fear < 20)
}
fn basis(
    tx: &Transaction<'_>,
    p: PersonId,
    group: &game_institutions::Institution,
) -> Result<Option<Attachment>> {
    if let Procedure::Leader(leader) = group.procedure {
        if accepts(tx, p, leader)
            && tx
                .get::<game_relationships::Social>()?
                .beliefs
                .get(&p)
                .is_some_and(|b| b.authority > 0)
        {
            return Ok(Some(Attachment::Loyalty(leader)));
        }
        if game_transport::area(tx.get()?, p).ok() == game_transport::area(tx.get()?, leader).ok()
            && game_pressure::afraid(tx, p, leader)?
        {
            return Ok(Some(Attachment::Fear(leader)));
        }
        return Ok(None);
    }
    if group.members.iter().any(|m| accepts(tx, p, *m)) {
        Ok(Some(
            if matches!(group.procedure, Procedure::Majority | Procedure::Unanimity) {
                Attachment::Voice
            } else {
                Attachment::Cooperation
            },
        ))
    } else {
        Ok(None)
    }
}
fn leave(tx: &mut Transaction<'_>, p: PersonId, id: InstitutionId) -> Result<()> {
    // Departing custodians withdraw their own old compact permissions.
    let group = tx.get::<game_institutions::Institutions>()?.groups[&id].clone();
    for rule in &group.rules {
        if rule.custodian == p {
            for user in &rule.users {
                if *user != p {
                    tx.call(game_property::Permit {
                        issuer: p,
                        person: *user,
                        inventory: rule.inventory,
                        allow: false,
                    })?;
                }
            }
        }
    }
    tx.call(game_institutions::Leave {
        institution: id,
        person: p,
    })?;
    tx.call(game_communities::Membership {
        institution: id,
        person: p,
        attachment: None,
    })
}
fn encounter(tx: &mut Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    let s = tx.get::<game_communities::Communities>()?;
    if !s.enabled || [Some(a), Some(b)].contains(&tx.get::<game_lineage::Lineage>()?.current) {
        return Ok(());
    }
    let (member, newcomer, id) = match (s.members.get(&a), s.members.get(&b)) {
        (Some(id), None) => (a, b, *id),
        (None, Some(id)) => (b, a, *id),
        _ => return Ok(()),
    };
    let group = tx.get::<game_institutions::Institutions>()?.groups[&id].clone();
    let Some(attachment) = basis(tx, newcomer, &group)? else {
        return Ok(());
    };
    let area = game_transport::area(tx.get()?, member)?;
    let mut responses = BTreeSet::from([newcomer]);
    for p in &group.members {
        if game_physiology::capable(tx, *p).is_ok()
            && game_transport::area(tx.get()?, *p).ok() == Some(area)
            && (accepts(tx, *p, newcomer)
                || matches!(group.procedure,Procedure::Leader(l) if l==*p))
        {
            responses.insert(*p);
        }
    }
    if tx
        .attempt(game_institutions::Join {
            institution: id,
            person: newcomer,
            responses,
        })
        .is_ok()
    {
        tx.call(game_communities::Membership {
            institution: id,
            person: newcomer,
            attachment: Some(attachment),
        })?;
    }
    Ok(())
}
fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let s = tx.get::<game_communities::Communities>()?;
    if !s.enabled
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
        || tx
            .get::<Affiliation>()?
            .next
            .get(&p)
            .is_some_and(|at| *at > tx.now())
    {
        return Ok(());
    }
    let Some(id) = s.members.get(&p).copied() else {
        return Ok(());
    };
    let next = tx.now().after(SimDuration(7 * DAY.0))?;
    tx.edit::<Affiliation>()?.next.insert(p, next);
    let group = tx.get::<game_institutions::Institutions>()?.groups[&id].clone();
    if group.members.len() > 1
        && !matches!(group.procedure,Procedure::Leader(l) if l==p)
        && basis(tx, p, &group)?.is_none()
    {
        return leave(tx, p, id);
    }
    if !matches!(group.procedure, Procedure::Personal) || group.members.len() < 2 {
        return Ok(());
    }
    // Each survivor considers a local arrangement. Votes require an expressed wish for shared decisions.
    let area = game_transport::area(tx.get()?, p)?;
    if !group
        .members
        .iter()
        .all(|m| game_transport::area(tx.get().unwrap(), *m).ok() == Some(area))
    {
        return Ok(());
    }
    let social = tx.get::<game_relationships::Social>()?;
    let voting = group.members.iter().all(|m| {
        social
            .values
            .get(m)
            .and_then(|v| v.get("shared-decisions"))
            .is_some_and(|n| *n >= 6)
    });
    let leader = group.members.iter().copied().find(|l| {
        tx.get::<game_traits::Traits>()
            .is_ok_and(|t| t.disposition(*l, "ambition") >= 650)
            && group.members.iter().all(|m| {
                m == l
                    || (accepts(tx, *m, *l)
                        && social.beliefs.get(m).is_some_and(|b| b.authority > 0))
            })
    });
    let procedure = if voting {
        Some(Procedure::Majority)
    } else {
        leader.map(Procedure::Leader)
    };
    if let Some(procedure) = procedure {
        tx.call(game_institutions::Arrange {
            institution: id,
            procedure: procedure.clone(),
            responses: group.members.clone(),
        })?;
        for m in group.members {
            let attachment = match procedure {
                Procedure::Leader(l) if m != l => Attachment::Loyalty(l),
                Procedure::Majority => Attachment::Voice,
                _ => Attachment::Independent,
            };
            tx.call(game_communities::Membership {
                institution: id,
                person: m,
                attachment: Some(attachment),
            })?;
        }
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("affiliation", 1)?;
    b.resource::<Affiliation>()?;
    b.subscribe::<game_social_life::Encounter>("z.affiliation.admission", |tx, e| {
        encounter(tx, e.first, e.second)
    })?;
    b.subscribe::<game_characters::IdleReview>("a.affiliation.consider", |tx, e| {
        consider(tx, e.person)
    })?;
    b.subscribe::<game_physiology::Died>("affiliation.departure", |tx, e| {
        if let Some(id) = tx
            .get::<game_communities::Communities>()?
            .members
            .get(&e.person)
            .copied()
        {
            leave(tx, e.person, id)?;
        }
        Ok(())
    })?;
    Ok(())
}
