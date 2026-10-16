use super::*;
use game_institutions::{AccessRule, Procedure};
fn relation(
    tx: &Transaction<'_>,
    person: PersonId,
    other: PersonId,
) -> game_relationships::Relationship {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&person)?.get(&other))
        .cloned()
        .unwrap_or_default()
}
fn belief(tx: &Transaction<'_>, p: PersonId) -> game_relationships::Beliefs {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.beliefs.get(&p))
        .cloned()
        .unwrap_or_default()
}
fn voice(tx: &Transaction<'_>, p: PersonId) -> i32 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&p)?.get("shared-decisions"))
        .copied()
        .unwrap_or(0)
}
fn cooperative(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> bool {
    let r = relation(tx, p, other);
    belief(tx, p).sharing > 0
        && r.affection >= 16
        && r.trust >= 4
        && r.fear < 10
        && r.grievance < 10
}
fn loyal(tx: &Transaction<'_>, p: PersonId, leader: PersonId) -> bool {
    let r = relation(tx, p, leader);
    let b = belief(tx, p);
    b.authority > 0
        && r.affection >= 24
        && r.trust >= 8
        && r.grievance < 10
        && i64::from(r.affection + r.trust + b.authority.clamp(0, 10) * 2)
            > game_pressure::disposition(tx, p, "ambition") / 20
}
fn allegiance(tx: &Transaction<'_>, p: PersonId, leader: PersonId) -> Result<Option<Attachment>> {
    if game_physiology::capable(tx, leader).is_err() {
        return Ok(None);
    }
    if loyal(tx, p, leader) {
        return Ok(Some(Attachment::Loyalty(leader)));
    }
    if game_physiology::capable(tx, leader).is_ok()
        && game_transport::area(tx.get()?, p)
            .ok()
            .is_some_and(|a| game_transport::area(tx.get().unwrap(), leader).ok() == Some(a))
        && game_pressure::afraid(tx, p, leader)?
    {
        return Ok(Some(Attachment::Fear(leader)));
    }
    Ok(None)
}
fn eligible(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<bool> {
    let s = tx.get::<Communities>()?;
    let player = tx.get::<game_lineage::Lineage>()?.current;
    Ok(s.enabled
        && a != b
        && ![Some(a), Some(b)].contains(&player)
        && s.members.get(&a) == s.members.get(&b))
}
fn choose(
    tx: &Transaction<'_>,
    a: PersonId,
    b: PersonId,
    forced: bool,
) -> Result<Option<(Procedure, BTreeMap<PersonId, Attachment>)>> {
    let mut candidates = vec![];
    for (leader, subject) in [(a, b), (b, a)] {
        if forced && leader != a {
            continue;
        }
        if !forced
            && (game_pressure::disposition(tx, leader, "ambition") < 650
                || belief(tx, leader).authority < 0)
        {
            continue;
        }
        if let Some(basis) = allegiance(tx, subject, leader)? {
            candidates.push((
                game_pressure::disposition(tx, leader, "ambition"),
                std::cmp::Reverse(leader),
                leader,
                subject,
                basis,
            ));
        }
    }
    if let Some((_, _, leader, subject, basis)) = candidates.into_iter().max_by_key(|c| (c.0, c.1))
    {
        return Ok(Some((
            Procedure::Leader(leader),
            BTreeMap::from([(leader, Attachment::Independent), (subject, basis)]),
        )));
    }
    if cooperative(tx, a, b) && cooperative(tx, b, a) {
        let vote = voice(tx, a) >= 6 && voice(tx, b) >= 6;
        Ok(Some((
            if vote {
                Procedure::Unanimity
            } else {
                Procedure::Personal
            },
            [
                (
                    a,
                    if vote {
                        Attachment::Voice
                    } else {
                        Attachment::Cooperation
                    },
                ),
                (
                    b,
                    if vote {
                        Attachment::Voice
                    } else {
                        Attachment::Cooperation
                    },
                ),
            ]
            .into(),
        )))
    } else {
        Ok(None)
    }
}
fn grant(
    tx: &mut Transaction<'_>,
    id: InstitutionId,
    custodian: PersonId,
    inventory: InventoryId,
    users: BTreeSet<PersonId>,
) -> Result<()> {
    for person in &users {
        tx.call(game_property::Permit {
            issuer: custodian,
            person: *person,
            inventory,
            allow: true,
        })?;
    }
    tx.call(game_institutions::RecordPractice {
        institution: id,
        terms: AccessRule {
            custodian,
            inventory,
            users,
        },
    })
}
fn organize(tx: &mut Transaction<'_>, a: PersonId, b: PersonId, forced: bool) -> Result<()> {
    if !eligible(tx, a, b)? {
        return Ok(());
    }
    let Some((procedure, attachments)) = choose(tx, a, b, forced)? else {
        return Ok(());
    };
    let (Some(first), Some(second)) = (supplies(tx, a)?, supplies(tx, b)?) else {
        return Ok(());
    };
    let members = BTreeSet::from([a, b]);
    let existing = tx.get::<Communities>()?.members.get(&a).copied();
    let area = tx.get::<game_social_life::SocialLife>()?.people[&a]
        .area
        .unwrap();
    let name_person = if let Procedure::Leader(p) = procedure {
        p
    } else {
        b
    };
    let person_name = &tx.get::<game_social_life::SocialLife>()?.people[&name_person].name;
    let name = format!(
        "{} {}",
        person_name.split_whitespace().last().unwrap_or(person_name),
        if matches!(procedure, Procedure::Leader(_)) {
            "Company"
        } else {
            "Hearth"
        }
    );
    let id = if let Some(id) = existing {
        if tx.get::<game_institutions::Institutions>()?.groups[&id].procedure != procedure {
            tx.call(game_institutions::Arrange {
                institution: id,
                procedure: procedure.clone(),
                responses: members.clone(),
            })?;
        }
        id
    } else {
        tx.call(game_institutions::Found(game_institutions::Institution {
            name: name.clone(),
            jurisdiction: area,
            members: members.clone(),
            procedure: procedure.clone(),
            offices: BTreeMap::new(),
            rules: vec![],
        }))?
    };
    for (p, inventory) in [(a, first), (b, second)] {
        let users = match procedure {
            Procedure::Leader(leader) if leader != p => BTreeSet::from([leader, p]),
            Procedure::Leader(_) if belief(tx, p).sharing <= 0 => BTreeSet::from([p]),
            _ => members.clone(),
        };
        if matches!(procedure, Procedure::Unanimity) && existing.is_none() {
            let proposal = tx.call(game_institutions::Propose {
                person: p,
                institution: id,
                terms: AccessRule {
                    custodian: p,
                    inventory,
                    users,
                },
            })?;
            for person in &members {
                tx.call(game_institutions::Vote {
                    person: *person,
                    proposal,
                    support: voice(tx, *person) >= 6
                        && cooperative(tx, *person, if *person == a { b } else { a }),
                })?;
            }
        } else {
            grant(tx, id, p, inventory, users)?;
        }
    }
    for (p, basis) in attachments {
        let s = tx.edit::<Communities>()?;
        s.members.insert(p, id);
        s.attachments.insert(p, basis);
    }
    tx.edit::<Communities>()?
        .places
        .entry(area)
        .or_default()
        .insert(id, ());
    if existing.is_none() {
        tx.emit(
            Founded {
                founder: a,
                companion: b,
                institution: id,
                name,
            },
            members.into_iter().collect(),
        )?;
    }
    Ok(())
}
fn withdraw(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let basis = tx.get::<Communities>()?.attachments.get(&p).cloned();
    if let Some(Attachment::Loyalty(leader) | Attachment::Fear(leader)) = basis
        && allegiance(tx, p, leader)?.is_none()
    {
        if let Some(inventory) = supplies(tx, p)? {
            tx.call(game_property::Permit {
                issuer: p,
                person: leader,
                inventory,
                allow: false,
            })?;
        }
        tx.edit::<Communities>()?
            .attachments
            .insert(p, Attachment::Independent);
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<game_institutions::CanCommit>(|tx, r| {
        if tx.get::<Communities>()?.members.get(&r.custodian) != Some(&r.institution) {
            return Ok(true);
        }
        if r.person == r.custodian {
            return Ok(true);
        }
        if tx.get::<game_lineage::Lineage>()?.current == Some(r.custodian) {
            return Ok(false);
        }
        let group = &tx.get::<game_institutions::Institutions>()?.groups[&r.institution];
        match group.procedure {
            Procedure::Leader(p) if p == r.person => Ok(allegiance(tx, r.custodian, p)?.is_some()),
            Procedure::Unanimity | Procedure::Majority => {
                Ok(voice(tx, r.custodian) >= 6 && belief(tx, r.custodian).sharing > 0)
            }
            _ => Ok(false),
        }
    })?;
    b.subscribe::<game_social_life::Encounter>("communities.consider", |tx, e| {
        if !eligible(tx, e.first, e.second)? {
            return Ok(());
        }
        for (p, other) in [(e.first, e.second), (e.second, e.first)] {
            let b = belief(tx, p);
            if b.sharing > 0 && b.authority < 0 && relation(tx, p, other).trust > 0 {
                tx.call(game_relationships::LearnValue {
                    person: p,
                    concept: "shared-decisions".into(),
                    outcome: 1 + (-i64::from(b.authority)).min(10) as i32 / 4,
                })?;
            }
            withdraw(tx, p)?;
        }
        organize(tx, e.first, e.second, false)
    })?;
    b.subscribe::<game_pressure::Submitted>("communities.submission", |tx, e| {
        organize(tx, e.leader, e.subject, true)
    })?;
    b.subscribe::<game_characters::IdleReview>("communities.ambition", |tx, e| {
        let protected = tx.get::<game_lineage::Lineage>()?.current;
        let s = tx.get::<Communities>()?;
        if !s.enabled || Some(e.person) == protected {
            return Ok(());
        }
        if s.members.contains_key(&e.person) {
            return withdraw(tx, e.person);
        }
        if s.retry_at.get(&e.person).is_some_and(|at| *at > tx.now())
            || game_pressure::disposition(tx, e.person, "aggression") < 850
            || game_pressure::disposition(tx, e.person, "ambition") < 650
            || game_pressure::disposition(tx, e.person, "risk-tolerance") < 600
        {
            return Ok(());
        }
        let social = tx.get::<game_social_life::SocialLife>()?;
        let Some(area) = social.people.get(&e.person).and_then(|p| p.area) else {
            return Ok(());
        };
        let target = social.residents[&area]
            .scan_after(Some(&e.person))
            .chain(social.residents[&area].iter())
            .take(16)
            .map(|(p, _)| *p)
            .find(|p| {
                *p != e.person
                    && Some(*p) != protected
                    && !s.members.contains_key(p)
                    && !tx.get::<Actions>().unwrap().0.contains_key(p)
                    && game_physiology::capable(tx, *p).is_ok()
                    && relation(tx, e.person, *p).affection < 16
            });
        let retry = tx.now().after(SimDuration(6 * HOUR.0))?;
        tx.edit::<Communities>()?.retry_at.insert(e.person, retry);
        if let Some(subject) = target {
            tx.call(game_pressure::DemandSubmission {
                leader: e.person,
                subject,
            })?;
        }
        Ok(())
    })
}
