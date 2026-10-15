//! Promises create obligations; only physical fulfillment transfers goods.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    pub debtor: PersonId,
    pub creditor: PersonId,
    pub from: InventoryId,
    pub to: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
    pub due: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Offered,
    Active,
    Rejected,
    Complete,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreement {
    pub parties: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub terms: Vec<Term>,
    pub fulfilled: BTreeSet<usize>,
    pub overdue: BTreeSet<usize>,
    pub status: Status,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreements(pub Records<AgreementId, Agreement>);
impl Resource for Agreements {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "agreements.promises";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.0.changed(&previous.0).map(|(_, row)| row))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Offer {
    pub proposer: PersonId,
    pub parties: BTreeSet<PersonId>,
    pub terms: Vec<Term>,
}
impl Request for Offer {
    const KEY: &'static str = "agreements.offer";
    type Response = AgreementId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Accept {
    pub person: PersonId,
    pub agreement: AgreementId,
}
impl Request for Accept {
    const KEY: &'static str = "agreements.accept";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Reject {
    pub person: PersonId,
    pub agreement: AgreementId,
}
impl Request for Reject {
    const KEY: &'static str = "agreements.reject";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Fulfill {
    pub person: PersonId,
    pub agreement: AgreementId,
    pub term: usize,
}
impl Request for Fulfill {
    const KEY: &'static str = "agreements.fulfill";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Due {
    agreement: AgreementId,
    term: usize,
}
impl Request for Due {
    const KEY: &'static str = "agreements.due";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Defaulted {
    pub agreement: AgreementId,
    pub debtor: PersonId,
}
impl Event for Defaulted {
    const KEY: &'static str = "agreements.defaulted";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("agreements", 1)?;
    b.resource::<Agreements>()?;
    b.request::<Offer>(|tx, r| {
        require(r.parties.contains(&r.proposer), "proposer is not a party")?;
        require(
            r.terms.iter().all(|t| t.due >= tx.now()),
            "offer is already due",
        )?;
        let id = AgreementId(tx.allocate()?);
        tx.edit::<Agreements>()?.0.insert(
            id,
            Agreement {
                parties: r.parties,
                accepted: BTreeSet::from([r.proposer]),
                terms: r.terms,
                fulfilled: BTreeSet::new(),
                overdue: BTreeSet::new(),
                status: Status::Offered,
            },
        );
        Ok(id)
    })?;
    b.request::<Accept>(|tx, r| {
        let a = tx
            .edit::<Agreements>()?
            .0
            .get_mut(&r.agreement)
            .ok_or_else(|| Error::Missing("agreement".into()))?;
        require(
            a.status == Status::Offered && a.parties.contains(&r.person),
            "cannot accept agreement",
        )?;
        a.accepted.insert(r.person);
        if a.accepted == a.parties {
            a.status = Status::Active;
            let terms = a.terms.clone();
            for (term, t) in terms.iter().enumerate() {
                require(t.due >= tx.now(), "offer has expired")?;
                tx.schedule(
                    t.due,
                    40,
                    Due {
                        agreement: r.agreement,
                        term,
                    },
                )?;
            }
        }
        Ok(())
    })?;
    b.request::<Reject>(|tx, r| {
        let a = tx
            .edit::<Agreements>()?
            .0
            .get_mut(&r.agreement)
            .ok_or_else(|| Error::Missing("agreement".into()))?;
        require(
            a.status == Status::Offered && a.parties.contains(&r.person),
            "cannot reject agreement",
        )?;
        a.status = Status::Rejected;
        Ok(())
    })?;
    b.request::<Fulfill>(|tx, r| {
        let a = tx
            .get::<Agreements>()?
            .0
            .get(&r.agreement)
            .ok_or_else(|| Error::Missing("agreement".into()))?;
        require(
            a.status == Status::Active && !a.fulfilled.contains(&r.term),
            "obligation unavailable",
        )?;
        let t = a
            .terms
            .get(r.term)
            .cloned()
            .ok_or_else(|| Error::Missing("term".into()))?;
        require(t.debtor == r.person, "not the debtor")?;
        tx.call(game_property::Take {
            actor: r.person,
            transfer: game_cargo::Transfer {
                from: t.from,
                to: t.to,
                good: t.good,
                quantity: t.quantity,
            },
            unauthorized: false,
        })?;
        let a = tx.edit::<Agreements>()?.0.get_mut(&r.agreement).unwrap();
        a.fulfilled.insert(r.term);
        a.overdue.remove(&r.term);
        if a.fulfilled.len() == a.terms.len() {
            a.status = Status::Complete;
        }
        Ok(())
    })?;
    b.request::<Due>(|tx, r| {
        let a = tx.edit::<Agreements>()?.0.get_mut(&r.agreement).unwrap();
        if a.status == Status::Active && !a.fulfilled.contains(&r.term) {
            a.overdue.insert(r.term);
            let debtor = a.terms[r.term].debtor;
            let witnesses = a.parties.iter().copied().collect();
            tx.emit(
                Defaulted {
                    agreement: r.agreement,
                    debtor,
                },
                witnesses,
            )?;
        }
        Ok(())
    })
}

impl Agreements {
    fn validate_rows<'a>(&'a self, rows: impl Iterator<Item = &'a Agreement>) -> Result<()> {
        for a in rows {
            require(
                a.parties.len() >= 2 && !a.terms.is_empty() && a.accepted.is_subset(&a.parties),
                "invalid agreement",
            )?;
            for t in &a.terms {
                require(
                    t.quantity > 0
                        && a.parties.contains(&t.debtor)
                        && a.parties.contains(&t.creditor),
                    "invalid term",
                )?;
            }
        }
        Ok(())
    }
}
