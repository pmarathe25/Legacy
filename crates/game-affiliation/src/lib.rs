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
    tie.is_some_and(|r| {
        r.affection + r.respect / 2 + r.admiration / 2 >= 20
            && r.trust >= 5
            && r.grievance < 15
            && r.fear < 20
    })
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
#[derive(Clone, Serialize, Deserialize)]
struct Depart {
    person: PersonId,
    institution: InstitutionId,
}
impl Request for Depart {
    const KEY: &'static str = "affiliation.depart";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Discuss {
    person: PersonId,
    institution: InstitutionId,
    procedure: Procedure,
}
impl Request for Discuss {
    const KEY: &'static str = "affiliation.discuss";
    type Response = bool;
}
// Called only when a proposal has actually been addressed to this participant.
fn assent(tx: &Transaction<'_>, p: PersonId, procedure: &Procedure) -> bool {
    let social = tx.get::<game_relationships::Social>().unwrap();
    let voice = social
        .values
        .get(&p)
        .and_then(|v| v.get("shared-decisions"))
        .copied()
        .unwrap_or(0);
    match procedure {
        Procedure::Leader(l) => {
            *l == p || accepts(tx, p, *l) && social.beliefs.get(&p).is_some_and(|b| b.authority > 0)
        }
        Procedure::Majority | Procedure::Unanimity => voice > 0,
        Procedure::Personal => social.beliefs.get(&p).is_none_or(|b| b.authority <= 0),
    }
}
fn consider(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if !game_vitality::can_converse(tx, p)
        || !tx.get::<game_communities::Communities>()?.enabled
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
    {
        return Ok(());
    }
    let Some(id) = tx
        .get::<game_communities::Communities>()?
        .members
        .get(&p)
        .copied()
    else {
        return Ok(());
    };
    let group = tx.get::<game_institutions::Institutions>()?.groups[&id].clone();
    if group.members.len() > 1
        && !matches!(group.procedure, Procedure::Leader(l) if l == p)
        && basis(tx, p, &group)?.is_none()
        && !game_choice::offer(
            tx,
            p,
            format!("affiliation:leave:{}", id.0),
            game_choice::motive(0, 60, 0, 1, 0),
            Depart {
                person: p,
                institution: id,
            },
        )?
    {
        leave(tx, p, id)?;
    }
    let own = tx.get::<game_relationships::Social>()?;
    let voice = own
        .values
        .get(&p)
        .and_then(|v| v.get("shared-decisions"))
        .copied()
        .unwrap_or(0);
    let independence = own
        .values
        .get(&p)
        .and_then(|v| v.get("independence"))
        .copied()
        .unwrap_or(0);
    for (name, procedure, value) in [
        ("majority", Procedure::Majority, i64::from(voice) * 4),
        (
            "unanimity",
            Procedure::Unanimity,
            i64::from(voice) * 3 + game_vitality::disposition(tx, p, "patience") / 100,
        ),
        ("personal", Procedure::Personal, i64::from(independence) * 3),
        (
            "lead",
            Procedure::Leader(p),
            game_vitality::disposition(tx, p, "ambition") / 10 - 50,
        ),
    ] {
        if procedure == group.procedure || group.members.len() < 2 {
            continue;
        }
        let recent = tx.get::<Affiliation>()?.next.get(&p).map_or(0, |at| {
            60i64.saturating_sub((tx.now().0.saturating_sub(at.0) / HOUR.0).min(60) as i64)
        });
        game_choice::offer(
            tx,
            p,
            format!("affiliation:{name}:{}", id.0),
            game_choice::motive(0, 0, value, 5 + recent, 0),
            Discuss {
                person: p,
                institution: id,
                procedure,
            },
        )?;
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("affiliation", 1)?;
    b.resource::<Affiliation>()?;
    b.request::<Depart>(|tx, r| leave(tx, r.person, r.institution))?;
    b.request::<Discuss>(|tx, r| {
        let group = tx.get::<game_institutions::Institutions>()?.groups[&r.institution].clone();
        require(group.members.contains(&r.person), "not a member")?;
        let area = game_transport::area(tx.get()?, r.person)?;
        let now = tx.now();
        tx.edit::<Affiliation>()?.next.insert(r.person, now);
        for p in &group.members {
            if Some(*p) == tx.get::<game_lineage::Lineage>()?.current
                || game_transport::area(tx.get()?, *p).ok() != Some(area)
                || !assent(tx, *p, &r.procedure)
            {
                return Ok(false);
            }
        }
        tx.call(game_institutions::Arrange {
            institution: r.institution,
            procedure: r.procedure.clone(),
            responses: group.members.clone(),
        })?;
        for p in group.members {
            let attachment = match r.procedure {
                Procedure::Leader(l) if l != p => Attachment::Loyalty(l),
                Procedure::Majority | Procedure::Unanimity => Attachment::Voice,
                _ => Attachment::Independent,
            };
            tx.call(game_communities::Membership {
                institution: r.institution,
                person: p,
                attachment: Some(attachment),
            })?;
        }
        Ok(true)
    })?;
    b.subscribe::<game_social_life::Encounter>("z.affiliation.admission", |tx, e| {
        encounter(tx, e.first, e.second)
    })?;
    game_choice::provider(b, "a.affiliation.consider", |tx, e| {
        if !game_vitality::can_converse(tx, e.person) {
            return Ok(());
        }
        consider(tx, e.person)?;
        if !game_choice::collecting(tx, e.person)
            || !tx.get::<game_communities::Communities>()?.enabled
        {
            return Ok(());
        }
        let protected = tx.get::<game_lineage::Lineage>()?.current;
        if Some(e.person) == protected {
            return Ok(());
        }
        let Ok(area) = game_transport::area(tx.get()?, e.person) else {
            return Ok(());
        };
        let peers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()?
            .residents
            .get(&area)
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .filter(|p| *p != e.person && Some(*p) != protected)
            .take(16)
            .collect();
        for other in peers {
            let relation = tx
                .get::<game_relationships::Social>()?
                .relationships
                .get(&e.person)
                .and_then(|r| r.get(&other))
                .cloned()
                .unwrap_or_default();
            let s = tx.get::<game_communities::Communities>()?;
            if s.members.get(&e.person) == s.members.get(&other) {
                let value = i64::from(relation.affection + relation.trust - relation.grievance);
                game_choice::offer(
                    tx,
                    e.person,
                    format!("community:cooperate:{}", other.0),
                    game_choice::motive(0, 0, value, 10, 0),
                    game_communities::Cooperate {
                        person: e.person,
                        other,
                    },
                )?;
            }
            if game_vitality::can_manipulate(tx, e.person)
                && game_vitality::can_converse(tx, other)
                && !tx
                    .get::<game_communities::Communities>()?
                    .members
                    .contains_key(&e.person)
            {
                let goal = game_vitality::disposition(tx, e.person, "aggression") / 10
                    + game_vitality::disposition(tx, e.person, "ambition") / 10
                    - 100;
                let risk = (1000 - game_vitality::disposition(tx, e.person, "risk-tolerance")) / 10
                    + i64::from(relation.affection.max(0));
                game_choice::offer(
                    tx,
                    e.person,
                    format!("community:pressure:{}", other.0),
                    game_choice::motive(0, goal, 0, 10, risk),
                    game_pressure::DemandSubmission {
                        leader: e.person,
                        subject: other,
                    },
                )?;
            }
        }
        Ok(())
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
