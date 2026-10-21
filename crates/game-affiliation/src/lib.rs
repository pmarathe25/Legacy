//! Admission, voluntary exit, and succession depend on individual cooperation, loyalty, or fear.
use game_communities::Attachment;
use game_core::*;
use game_institutions::Procedure;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
    if let Some(Attachment::Fear(leader)) = tx
        .get::<game_communities::Communities>()?
        .attachments
        .get(&p)
        && game_physiology::capable(tx, *leader).is_ok()
        && game_transport::area(tx.get()?, p).ok() == game_transport::area(tx.get()?, *leader).ok()
        && game_pressure::afraid(tx, p, *leader)?
    {
        return Ok(Some(Attachment::Fear(*leader)));
    }
    if let Some(leader) = group.procedure.sole_decider() {
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
        Ok(Some(if group.procedure.consults() {
            Attachment::Voice
        } else {
            Attachment::Cooperation
        }))
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
            && (accepts(tx, *p, newcomer) || group.procedure.sole_decider() == Some(*p))
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
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ArrangementDiscussed {
    pub proposer: PersonId,
    pub institution: InstitutionId,
    pub procedure: Procedure,
    pub responses: BTreeMap<PersonId, bool>,
}
impl Event for ArrangementDiscussed {
    const KEY: &'static str = "affiliation.arrangement-discussed";
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
        Procedure::Consultation(_) => true,
        Procedure::Majority | Procedure::Unanimity => voice > 0,
        Procedure::Personal => social.beliefs.get(&p).is_none_or(|b| b.authority <= 0),
    }
}
fn preference(tx: &Transaction<'_>, p: PersonId, procedure: &Procedure) -> i64 {
    let social = tx.get::<game_relationships::Social>().unwrap();
    let values = social.values.get(&p);
    let value = |key: &str| {
        values
            .and_then(|v| v.get(key))
            .copied()
            .map_or(0, i64::from)
    };
    let authority = social.beliefs.get(&p).map_or(0, |b| i64::from(b.authority));
    match procedure {
        Procedure::Personal => game_deliberation::personal_preference(tx, p),
        Procedure::Consultation(rule) => game_deliberation::preference(tx, p, rule),
        Procedure::Majority => value("shared-decisions") * 4,
        Procedure::Unanimity => {
            value("shared-decisions") * 3 + game_vitality::disposition(tx, p, "patience") / 100
        }
        Procedure::Leader(l) if *l == p => {
            game_vitality::disposition(tx, p, "ambition") / 10 - 50 + authority * 3
        }
        Procedure::Leader(l) => social.regard(p, *l) / 3 + authority * 3,
    }
}
fn change_cost(
    tx: &Transaction<'_>,
    p: PersonId,
    id: InstitutionId,
    alternative: &Procedure,
) -> i64 {
    game_vitality::disposition(tx, p, "rigidity") / 50
        + game_identity::resistance(
            tx,
            p,
            &format!("institution:{}", id.0),
            &commitment(alternative),
        )
}
fn commitment(procedure: &Procedure) -> String {
    match procedure {
        Procedure::Consultation(_) => {
            format!("arrangement:{}", fingerprint(&format!("{procedure:?}")))
        }
        _ => format!("{procedure:?}"),
    }
}
fn response(
    tx: &Transaction<'_>,
    p: PersonId,
    id: InstitutionId,
    current: &Procedure,
    proposed: &Procedure,
) -> Result<Option<Attachment>> {
    let willing = assent(tx, p, proposed)
        && preference(tx, p, proposed)
            > preference(tx, p, current) + change_cost(tx, p, id, proposed);
    if let Some(leader) = proposed.sole_decider() {
        if willing {
            return Ok(Some(if leader == p {
                Attachment::Independent
            } else {
                Attachment::Loyalty(leader)
            }));
        }
        if leader != p
            && game_physiology::capable(tx, leader).is_ok()
            && game_transport::area(tx.get()?, p).ok()
                == game_transport::area(tx.get()?, leader).ok()
            && game_pressure::afraid(tx, p, leader)?
        {
            return Ok(Some(Attachment::Fear(leader)));
        }
        return Ok(None);
    }
    Ok(willing.then_some(if proposed.consults() {
        Attachment::Voice
    } else {
        Attachment::Independent
    }))
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
        && group.procedure.sole_decider() != Some(p)
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
    for procedure in [
        Procedure::Personal,
        Procedure::Consultation(game_deliberation::construct(tx, p, &group.members)),
    ] {
        if procedure == group.procedure {
            continue;
        }
        let value = preference(tx, p, &procedure) - preference(tx, p, &group.procedure);
        if !assent(tx, p, &procedure) || value <= change_cost(tx, p, id, &procedure) {
            continue;
        }
        game_choice::offer(
            tx,
            p,
            format!(
                "affiliation:arrangement:{}:{}",
                id.0,
                fingerprint(&format!("{procedure:?}"))
            ),
            game_choice::motive(0, 0, value, 5 + change_cost(tx, p, id, &procedure), 0),
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
        if group.members.iter().any(|p| {
            Some(*p) == tx.get::<game_lineage::Lineage>().unwrap().current
                || !game_vitality::can_converse(tx, *p)
                || game_transport::area(tx.get().unwrap(), *p).ok() != Some(area)
        }) {
            return Ok(game_choice::Outcome::Reconsider);
        }
        let attachments: BTreeMap<_, _> = group
            .members
            .iter()
            .map(|p| {
                Ok((
                    *p,
                    if *p == r.person && r.procedure.sole_decider() == Some(*p) {
                        // Claiming power is an expressed act, even if the claimant
                        // has no private belief in the legitimacy of authority.
                        Some(Attachment::Independent)
                    } else {
                        response(tx, *p, r.institution, &group.procedure, &r.procedure)?
                    },
                ))
            })
            .collect::<Result<_>>()?;
        let responses: BTreeMap<_, _> =
            attachments.iter().map(|(p, a)| (*p, a.is_some())).collect();
        tx.emit(
            ArrangementDiscussed {
                proposer: r.person,
                institution: r.institution,
                procedure: r.procedure.clone(),
                responses: responses.clone(),
            },
            group.members.iter().copied().collect(),
        )?;
        if responses.values().any(|accepted| !accepted) {
            return Ok(game_choice::Outcome::Reconsider);
        }
        tx.call(game_institutions::Arrange {
            institution: r.institution,
            procedure: r.procedure.clone(),
            responses: group.members.clone(),
        })?;
        for (p, attachment) in attachments {
            let attachment = attachment.unwrap();
            // Outward submission is not a private commitment to the ruler's terms.
            if !matches!(attachment, Attachment::Fear(_)) {
                tx.call(game_identity::Commit {
                    person: p,
                    domain: format!("institution:{}", r.institution.0),
                    preference: commitment(&r.procedure),
                })?;
            }
            tx.call(game_communities::Membership {
                institution: r.institution,
                person: p,
                attachment: Some(attachment),
            })?;
        }
        Ok(game_choice::Outcome::Performed)
    })?;
    b.subscribe::<game_social_life::Encounter>("z.affiliation.admission", |tx, e| {
        encounter(tx, e.first, e.second)
    })?;
    b.subscribe::<game_pressure::Submitted>("z.affiliation.submission", |tx, e| {
        if matches!(e.purpose, game_pressure::Purpose::Allegiance) {
            let s = tx.get::<game_communities::Communities>()?;
            if let Some(id) = s.members.get(&e.leader).copied()
                && s.members.get(&e.subject) == Some(&id)
                && tx.get::<game_institutions::Institutions>()?.groups[&id]
                    .members
                    .len()
                    > 2
            {
                // Individual submission matters even while others resist.
                tx.call(game_communities::Membership {
                    institution: id,
                    person: e.subject,
                    attachment: Some(Attachment::Fear(e.leader)),
                })?;
                let pack = tx.get::<game_cargo::Cargo>()?.carried(e.subject).next();
                if let Some(inventory) = pack
                    && tx
                        .attempt(game_property::Permit {
                            issuer: e.subject,
                            person: e.leader,
                            inventory,
                            allow: true,
                        })
                        .is_ok()
                {
                    tx.call(game_institutions::RecordPractice {
                        institution: id,
                        terms: game_institutions::AccessRule {
                            custodian: e.subject,
                            inventory,
                            users: [e.leader].into(),
                        },
                    })?;
                }
                // Each affected member can still resist; defeating one member is
                // not evidence that everyone else has submitted.
                tx.call(Discuss {
                    person: e.leader,
                    institution: id,
                    procedure: Procedure::Consultation(game_deliberation::DecisionRule {
                        influence: [(e.leader, 1)].into(),
                        required_support: 1,
                    }),
                })?;
            }
        }
        Ok(())
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
            let same_group = s.members.get(&e.person) == s.members.get(&other);
            let can_press = !s.members.contains_key(&e.person) || same_group;
            if same_group {
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
                && can_press
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
