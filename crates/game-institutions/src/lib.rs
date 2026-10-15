//! Founding, votes, executable access rules, and revocable offices.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Procedure {
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
        Procedure::Leader(p) => votes.get(p) == Some(&true),
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
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("institutions", 1)?;
    b.resource::<Institutions>()?;
    b.request::<Found>(|tx, r| {
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
        let id = tx.allocate()?;
        tx.edit::<Institutions>()?.proposals.insert(
            id,
            Proposal {
                institution: r.institution,
                terms: r.terms,
                votes: BTreeMap::new(),
                adopted: false,
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
            i.members.contains(&r.person) && !proposal.closed,
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
            for person in &proposal.terms.users {
                tx.call(game_property::Permit {
                    issuer: proposal.terms.custodian,
                    person: *person,
                    inventory: proposal.terms.inventory,
                    allow: true,
                })?;
            }
            let s = tx.edit::<Institutions>()?;
            let p = s.proposals.get_mut(&r.proposal).unwrap();
            p.adopted = true;
            p.closed = true;
            s.groups
                .get_mut(&proposal.institution)
                .unwrap()
                .rules
                .push(proposal.terms);
        } else if tx.get::<Institutions>()?.proposals[&r.proposal].votes.len() == i.members.len() {
            tx.edit::<Institutions>()?
                .proposals
                .get_mut(&r.proposal)
                .unwrap()
                .closed = true;
        }
        Ok(adopted)
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
                !i.members.is_empty()
                    && !i.name.is_empty()
                    && i.offices.values().all(|p| i.members.contains(p)),
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
