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
    let attraction = r.affection + r.respect + r.admiration;
    (b.authority > 0 || r.admiration >= 50 || r.respect >= 50)
        && attraction >= 24
        && r.trust >= 8
        && r.grievance < 10
        && i64::from(attraction + r.trust + b.authority.clamp(0, 10) * 2)
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
    if !forced && (!game_vitality::can_converse(tx, a) || !game_vitality::can_converse(tx, b)) {
        return Ok(None);
    }
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
            Procedure::Consultation(game_deliberation::DecisionRule {
                influence: [(leader, 1)].into(),
                required_support: 1,
            }),
            BTreeMap::from([(leader, Attachment::Independent), (subject, basis)]),
        )));
    }
    if cooperative(tx, a, b) && cooperative(tx, b, a) {
        let arrangement = game_deliberation::construct(tx, a, &BTreeSet::from([a, b]));
        let consult = [a, b].iter().all(|p| {
            game_deliberation::preference(tx, *p, &arrangement)
                > game_deliberation::personal_preference(tx, *p)
        });
        Ok(Some((
            if consult {
                Procedure::Consultation(arrangement)
            } else {
                Procedure::Personal
            },
            [a, b]
                .into_iter()
                .map(|p| {
                    (
                        p,
                        if consult {
                            Attachment::Voice
                        } else {
                            Attachment::Cooperation
                        },
                    )
                })
                .collect(),
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
) -> Result<bool> {
    let terms = AccessRule {
        custodian,
        inventory,
        users,
    };
    let recorded = tx.get::<game_institutions::Institutions>()?.groups[&id]
        .rules
        .contains(&terms);
    if recorded
        && terms.users.iter().all(|p| {
            tx.get::<game_property::Property>()
                .unwrap()
                .allowed(*p, inventory)
        })
    {
        return Ok(false);
    }
    for person in &terms.users {
        tx.call(game_property::Permit {
            issuer: custodian,
            person: *person,
            inventory,
            allow: true,
        })?;
    }
    tx.call(game_institutions::RecordPractice {
        institution: id,
        terms,
    })?;
    Ok(true)
}
fn organize(tx: &mut Transaction<'_>, a: PersonId, b: PersonId, forced: bool) -> Result<bool> {
    if !eligible(tx, a, b)? {
        return Ok(false);
    }
    if tx.get::<Communities>()?.members.get(&a).is_some_and(|id| {
        tx.get::<game_institutions::Institutions>()
            .is_ok_and(|s| s.groups[id].members.len() > 2)
    }) {
        return Ok(false);
    }
    let Some((mut procedure, mut attachments)) = choose(tx, a, b, forced)? else {
        return Ok(false);
    };
    let (Some(first), Some(second)) = (supplies(tx, a)?, supplies(tx, b)?) else {
        return Ok(false);
    };
    let members = BTreeSet::from([a, b]);
    let existing = tx.get::<Communities>()?.members.get(&a).copied();
    let area = tx.get::<game_social_life::SocialLife>()?.people[&a]
        .area
        .unwrap();
    let name_person = procedure.sole_decider().unwrap_or(b);
    let person_name = &tx.get::<game_social_life::SocialLife>()?.people[&name_person].name;
    let name = format!(
        "{} {}",
        person_name.split_whitespace().last().unwrap_or(person_name),
        if procedure.sole_decider().is_some() {
            "Company"
        } else {
            "Hearth"
        }
    );
    // Cooperation can establish a joint arrangement, but renewing access does
    // not rewrite an agreement. Changes use the deliberation owner.
    if !forced && let Some(id) = existing {
        let current = &tx.get::<game_institutions::Institutions>()?.groups[&id].procedure;
        if *current != Procedure::Personal {
            procedure = current.clone();
            for p in [a, b] {
                let basis = if let Some(leader) = procedure.sole_decider() {
                    if leader == p {
                        Attachment::Independent
                    } else {
                        let Some(basis) = allegiance(tx, p, leader)? else {
                            return Ok(false);
                        };
                        basis
                    }
                } else {
                    Attachment::Voice
                };
                attachments.insert(p, basis);
            }
        }
    }
    let mut changed = existing.is_none();
    let id = if let Some(id) = existing {
        if tx.get::<game_institutions::Institutions>()?.groups[&id].procedure != procedure {
            changed = true;
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
        let users = match procedure.sole_decider() {
            Some(leader) if leader != p => BTreeSet::from([leader, p]),
            Some(_) if belief(tx, p).sharing <= 0 => BTreeSet::from([p]),
            _ => members.clone(),
        };
        if procedure.consults() && procedure.sole_decider().is_none() && existing.is_none() {
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
                    support: cooperative(tx, *person, if *person == a { b } else { a }),
                })?;
            }
        } else {
            changed |= grant(tx, id, p, inventory, users)?;
        }
    }
    for (p, basis) in attachments {
        changed |= tx.get::<Communities>()?.attachments.get(&p) != Some(&basis);
        let s = tx.edit::<Communities>()?;
        s.members.insert(p, id);
        s.attachments.insert(p, basis);
    }
    let jurisdiction = tx.get::<game_institutions::Institutions>()?.groups[&id].jurisdiction;
    tx.edit::<Communities>()?
        .places
        .entry(jurisdiction)
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
    Ok(changed)
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
#[derive(Clone, Serialize, Deserialize)]
pub struct Cooperate {
    pub person: PersonId,
    pub other: PersonId,
}
impl Request for Cooperate {
    const KEY: &'static str = "communities.cooperate";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Cooperate>(|tx, r| {
        require(
            organize(tx, r.person, r.other, false)?,
            "cooperation made no progress",
        )
    })?;
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
        if let Some(p) = group.procedure.sole_decider() {
            return Ok(p == r.person && allegiance(tx, r.custodian, p)?.is_some());
        }
        match &group.procedure {
            Procedure::Consultation(rule) => Ok(belief(tx, r.custodian).sharing > 0
                && game_deliberation::preference(tx, r.custodian, rule)
                    >= game_deliberation::personal_preference(tx, r.custodian)),
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
        }
        if tx.get::<game_characters::Minds>().is_ok_and(|m| m.running) {
            tx.notify(game_choice::Changed { person: e.first })?;
            tx.notify(game_choice::Changed { person: e.second })?;
            return Ok(());
        }
        for p in [e.first, e.second] {
            withdraw(tx, p)?;
        }
        organize(tx, e.first, e.second, false).map(|_| ())
    })?;
    b.subscribe::<game_pressure::Submitted>("communities.submission", |tx, e| {
        if matches!(e.purpose, game_pressure::Purpose::Allegiance) {
            organize(tx, e.leader, e.subject, true).map(|_| ())
        } else {
            Ok(())
        }
    })?;
    Ok(())
}
