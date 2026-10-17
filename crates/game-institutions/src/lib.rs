//! Founding, votes, executable access rules, and revocable offices.
mod membership;
use game_core::*;
use game_sim::*;
use game_storage::Records;
pub use membership::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Procedure {
    /// No joint decision procedure: each custodian controls their own commitments.
    Personal,
    Leader(PersonId),
    Majority,
    Unanimity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRule {
    pub custodian: PersonId,
    pub inventory: InventoryId,
    pub users: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Institution {
    pub name: String,
    pub jurisdiction: AreaId,
    pub members: BTreeSet<PersonId>,
    pub procedure: Procedure,
    pub offices: BTreeMap<String, PersonId>,
    pub rules: Vec<AccessRule>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub institution: InstitutionId,
    pub terms: AccessRule,
    pub votes: BTreeMap<PersonId, bool>,
    pub adopted: bool,
    #[serde(default)]
    pub enacted: bool,
    pub closed: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Institutions {
    pub groups: Records<InstitutionId, Institution>,
    pub proposals: Records<u64, Proposal>,
}
impl Resource for Institutions {
    fn allocated_ids(&self) -> Vec<u64> {
        self.groups
            .keys()
            .map(|id| id.0)
            .chain(self.proposals.keys().copied())
            .collect()
    }
    const KEY: &'static str = "institutions.groups";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.groups.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.groups.changed(&previous.groups).map(|(_, row)| row))
    }
}
pub fn passed(
    procedure: &Procedure,
    members: &BTreeSet<PersonId>,
    votes: &BTreeMap<PersonId, bool>,
) -> bool {
    match procedure {
        Procedure::Personal | Procedure::Leader(_) => false,
        Procedure::Majority => {
            votes
                .iter()
                .filter(|(p, v)| members.contains(p) && **v)
                .count()
                > members.len() / 2
        }
        Procedure::Unanimity => members.iter().all(|p| votes.get(p) == Some(&true)),
    }
}
/// Bounded construction combines available holders with observed unmet access.
pub fn construct(
    custodian: PersonId,
    inventory: InventoryId,
    known_needy: &BTreeSet<PersonId>,
    limit: usize,
) -> Vec<AccessRule> {
    known_needy
        .iter()
        .take(limit)
        .map(|person| AccessRule {
            custodian,
            inventory,
            users: BTreeSet::from([*person]),
        })
        .collect()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Found(pub Institution);
impl Request for Found {
    const KEY: &'static str = "institutions.found";
    type Response = InstitutionId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Propose {
    pub person: PersonId,
    pub institution: InstitutionId,
    pub terms: AccessRule,
}
impl Request for Propose {
    const KEY: &'static str = "institutions.propose";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Vote {
    pub person: PersonId,
    pub proposal: u64,
    pub support: bool,
}
/// A decision by a recognized individual is not a ballot.
#[derive(Clone, Serialize, Deserialize)]
pub struct Decide {
    pub person: PersonId,
    pub proposal: u64,
    pub support: bool,
}
impl Request for Decide {
    const KEY: &'static str = "institutions.decide";
    type Response = bool;
}
/// Record an independently enacted practice without manufacturing a vote.
#[derive(Clone, Serialize, Deserialize)]
pub struct RecordPractice {
    pub institution: InstitutionId,
    pub terms: AccessRule,
}
impl Request for RecordPractice {
    const KEY: &'static str = "institutions.record-practice";
    type Response = ();
}
/// Affected people's actual responses; compliance does not imply approval or a ballot.
#[derive(Clone, Serialize, Deserialize)]
pub struct Arrange {
    pub institution: InstitutionId,
    pub procedure: Procedure,
    pub responses: BTreeSet<PersonId>,
}
impl Request for Arrange {
    const KEY: &'static str = "institutions.arrange";
    type Response = ();
}
/// An upper social owner can check whether a real custodian will carry out this decision.
#[derive(Clone, Serialize, Deserialize)]
pub struct CanCommit {
    pub institution: InstitutionId,
    pub person: PersonId,
    pub custodian: PersonId,
}
impl Request for CanCommit {
    const KEY: &'static str = "institutions.can-commit";
    type Response = bool;
}
impl Request for Vote {
    const KEY: &'static str = "institutions.vote";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Appoint {
    pub issuer: PersonId,
    pub institution: InstitutionId,
    pub office: String,
    pub person: Option<PersonId>,
}
impl Request for Appoint {
    const KEY: &'static str = "institutions.appoint";
    type Response = ();
}
pub fn holds(
    groups: &Institutions,
    person: PersonId,
    institution: InstitutionId,
    office: &str,
) -> bool {
    groups
        .groups
        .get(&institution)
        .and_then(|i| i.offices.get(office))
        == Some(&person)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RuleAdopted {
    pub institution: InstitutionId,
    pub proposal: u64,
    pub enacted: bool,
}
impl Event for RuleAdopted {
    const KEY: &'static str = "institutions.rule-adopted";
}
fn enact(tx: &mut Transaction<'_>, p: &Proposal, person: PersonId) -> Result<bool> {
    if tx.registered::<CanCommit>()
        && !tx.call(CanCommit {
            institution: p.institution,
            person,
            custodian: p.terms.custodian,
        })?
    {
        return Ok(false);
    }
    for user in &p.terms.users {
        tx.call(game_property::Permit {
            issuer: p.terms.custodian,
            person: *user,
            inventory: p.terms.inventory,
            allow: true,
        })?;
    }
    tx.edit::<Institutions>()?
        .groups
        .get_mut(&p.institution)
        .unwrap()
        .rules
        .push(p.terms.clone());
    Ok(true)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("institutions", 1)?;
    b.resource::<Institutions>()?;
    b.request::<RecordPractice>(|tx, r| {
        let group = tx
            .get::<Institutions>()?
            .groups
            .get(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(
            group.members.contains(&r.terms.custodian)
                && r.terms.users.is_subset(&group.members)
                && r.terms.users.iter().all(|p| {
                    tx.get::<game_property::Property>()
                        .is_ok_and(|s| s.allowed(*p, r.terms.inventory))
                }),
            "practice has not been enacted",
        )?;
        let rules = &mut tx
            .edit::<Institutions>()?
            .groups
            .get_mut(&r.institution)
            .unwrap()
            .rules;
        if !rules.contains(&r.terms) {
            rules.push(r.terms);
        }
        Ok(())
    })?;
    b.request::<Arrange>(|tx, r| {
        let group = tx
            .get::<Institutions>()?
            .groups
            .get(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(
            r.responses == group.members,
            "affected people have not responded to the arrangement",
        )?;
        if let Procedure::Leader(p) = r.procedure {
            require(group.members.contains(&p), "unknown leader")?;
        }
        let previous = group.procedure.clone();
        tx.edit::<Institutions>()?
            .groups
            .get_mut(&r.institution)
            .unwrap()
            .procedure = r.procedure.clone();
        if previous != r.procedure {
            tx.emit(
                ProcedureChanged {
                    institution: r.institution,
                    previous,
                    procedure: r.procedure,
                },
                r.responses.into_iter().collect(),
            )?;
        }
        Ok(())
    })?;
    membership::register(b)?;
    b.request::<Found>(|tx, r| {
        require(!r.0.members.is_empty(), "founding needs members")?;
        require(
            r.0.rules.is_empty(),
            "found with proposed rather than adopted rules",
        )?;
        let id = InstitutionId(tx.allocate()?);
        tx.edit::<Institutions>()?.groups.insert(id, r.0);
        Ok(id)
    })?;
    b.request::<Propose>(|tx, r| {
        let i = tx
            .get::<Institutions>()?
            .groups
            .get(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(i.members.contains(&r.person), "not a member")?;
        require(
            i.members.contains(&r.terms.custodian) && r.terms.users.is_subset(&i.members),
            "terms exceed the participating members",
        )?;
        let id = tx.allocate()?;
        tx.edit::<Institutions>()?.proposals.insert(
            id,
            Proposal {
                institution: r.institution,
                terms: r.terms,
                votes: BTreeMap::new(),
                adopted: false,
                enacted: false,
                closed: false,
            },
        );
        Ok(id)
    })?;
    b.request::<Vote>(|tx, r| {
        let proposal = tx
            .get::<Institutions>()?
            .proposals
            .get(&r.proposal)
            .ok_or_else(|| Error::Missing("proposal".into()))?
            .clone();
        let i = tx.get::<Institutions>()?.groups[&proposal.institution].clone();
        require(
            i.members.contains(&r.person)
                && !proposal.closed
                && matches!(i.procedure, Procedure::Majority | Procedure::Unanimity),
            "vote unavailable",
        )?;
        let p = tx
            .edit::<Institutions>()?
            .proposals
            .get_mut(&r.proposal)
            .unwrap();
        p.votes.insert(r.person, r.support);
        let adopted = passed(&i.procedure, &i.members, &p.votes);
        if adopted {
            let enacted = enact(tx, &proposal, r.person)?;
            let p = tx
                .edit::<Institutions>()?
                .proposals
                .get_mut(&r.proposal)
                .unwrap();
            p.adopted = true;
            p.enacted = enacted;
            p.closed = true;
            tx.emit(
                RuleAdopted {
                    institution: proposal.institution,
                    proposal: r.proposal,
                    enacted,
                },
                i.members.iter().copied().collect(),
            )?;
        } else if tx.get::<Institutions>()?.proposals[&r.proposal].votes.len() == i.members.len() {
            tx.edit::<Institutions>()?
                .proposals
                .get_mut(&r.proposal)
                .unwrap()
                .closed = true;
        }
        Ok(adopted)
    })?;
    b.request::<Decide>(|tx, r| {
        let proposal = tx
            .get::<Institutions>()?
            .proposals
            .get(&r.proposal)
            .ok_or_else(|| Error::Missing("proposal".into()))?
            .clone();
        let group = &tx.get::<Institutions>()?.groups[&proposal.institution];
        require(
            !proposal.closed
                && group.members.contains(&r.person)
                && match group.procedure {
                    Procedure::Leader(p) => p == r.person,
                    Procedure::Personal => proposal.terms.custodian == r.person,
                    _ => false,
                },
            "individual decision unavailable",
        )?;
        let enacted = if r.support {
            enact(tx, &proposal, r.person)?
        } else {
            false
        };
        if r.support {
            tx.emit(
                RuleAdopted {
                    institution: proposal.institution,
                    proposal: r.proposal,
                    enacted,
                },
                vec![r.person],
            )?;
        }
        let p = tx
            .edit::<Institutions>()?
            .proposals
            .get_mut(&r.proposal)
            .unwrap();
        p.adopted = r.support;
        p.enacted = enacted;
        p.closed = true;
        Ok(r.support)
    })?;
    b.request::<Appoint>(|tx, r| {
        let i = tx
            .edit::<Institutions>()?
            .groups
            .get_mut(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(
            matches!(i.procedure,Procedure::Leader(p) if p==r.issuer),
            "appointment requires recognized leader",
        )?;
        require(!r.office.is_empty(), "office needs name")?;
        if let Some(person) = r.person {
            require(i.members.contains(&person), "not a member")?;
            i.offices.insert(r.office, person);
        } else {
            i.offices.remove(&r.office);
        }
        Ok(())
    })
}
impl Institutions {
    fn validate_rows<'a>(&'a self, rows: impl Iterator<Item = &'a Institution>) -> Result<()> {
        for i in rows {
            require(
                !i.name.is_empty() && i.offices.values().all(|p| i.members.contains(p)),
                "invalid institution",
            )?;
            if let Procedure::Leader(p) = i.procedure {
                require(i.members.contains(&p), "leader is not a member")?;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn procedures_and_proposal_bounds_are_explicit() {
        let members = BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]);
        let votes = BTreeMap::from([(PersonId(1), true), (PersonId(2), true)]);
        assert!(passed(&Procedure::Majority, &members, &votes));
        assert!(!passed(&Procedure::Unanimity, &members, &votes));
        assert_eq!(construct(PersonId(4), InventoryId(5), &members, 2).len(), 2);
    }
}
