//! Deferred exchange and transferable promises depend on individual acceptance.
macro_rules! request {
    ($ty:ty,$key:literal,$response:ty) => {
        impl game_sim::Request for $ty {
            const KEY: &'static str = $key;
            type Response = $response;
        }
    };
}
use game_core::*;
use game_sim::*;
use game_storage::Table;
use game_vitality::{ability, disposition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub quantity: u64,
    pub partners: BTreeSet<PersonId>,
    pub exchanges: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credit {
    pub units: Table<PersonId, Table<GoodId, Unit>>,
    pub known: Table<PersonId, BTreeSet<AgreementId>>,
    pub assigned: u64,
    #[serde(default)]
    pub discussed: Table<PersonId, Table<PersonId, SimTime>>,
    #[serde(default)]
    pub promises: Table<PersonId, Table<AgreementId, Vec<game_agreements::Term>>>,
    #[serde(default)]
    pub paid: Table<PersonId, Table<AgreementId, BTreeSet<usize>>>,
}
impl Credit {
    fn promise(&self, p: PersonId, id: AgreementId) -> Option<&Vec<game_agreements::Term>> {
        self.promises.get(&p)?.get(&id)
    }
}
impl Resource for Credit {
    const KEY: &'static str = "credit.conventions";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Borrow {
    pub borrower: PersonId,
    pub lender: PersonId,
    pub good: GoodId,
    pub quantity: u64,
    pub repayment: u64,
    pub due: SimTime,
}
request!(Borrow, "credit.borrow", AgreementId);
#[derive(Clone, Serialize, Deserialize)]
pub struct Endorse {
    pub holder: PersonId,
    pub recipient: PersonId,
    pub agreement: AgreementId,
    pub term: usize,
    pub good: GoodId,
    pub quantity: u64,
}
request!(Endorse, "credit.endorse", bool);
#[derive(Clone, Serialize, Deserialize)]
pub struct ProposeUnit {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub good: GoodId,
    pub quantity: u64,
}
request!(ProposeUnit, "credit.propose-unit", bool);
#[derive(Clone, Serialize, Deserialize)]
pub struct TellPromise {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub agreement: AgreementId,
}
request!(TellPromise, "credit.tell-promise", bool);
#[derive(Clone, Serialize, Deserialize)]
struct Pay {
    person: PersonId,
    agreement: AgreementId,
    term: usize,
    expected: game_agreements::Term,
}
request!(Pay, "credit.pay", ());
fn remember(
    tx: &mut Transaction<'_>,
    person: PersonId,
    agreement: AgreementId,
    terms: Vec<game_agreements::Term>,
) -> Result<()> {
    let credit = tx.edit::<Credit>()?;
    credit.known.entry(person).or_default().insert(agreement);
    credit
        .promises
        .entry(person)
        .or_default()
        .insert(agreement, terms);
    Ok(())
}
fn pack(tx: &Transaction<'_>, p: PersonId) -> Result<InventoryId> {
    tx.get::<game_cargo::Cargo>()?
        .carried(p)
        .next()
        .ok_or_else(|| Error::Missing("personal pack".into()))
}
fn local(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    game_physiology::capable(tx, a)?;
    game_physiology::capable(tx, b)?;
    require(
        a != b && game_transport::area(tx.get()?, a)? == game_transport::area(tx.get()?, b)?,
        "agreement needs contact",
    )
}
use game_exchange::credit_trust as trust;
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b > 0 {
        (a, b) = (b, a % b);
    }
    a
}
#[derive(Clone, Serialize, Deserialize)]
struct Discuss {
    person: PersonId,
    peer: PersonId,
    request: Pending,
}
request!(Discuss, "credit.discuss", game_choice::Outcome);
fn age(tx: &Transaction<'_>, p: PersonId, q: PersonId) -> Result<i64> {
    Ok(tx
        .get::<Credit>()?
        .discussed
        .get(&p)
        .and_then(|r| r.get(&q))
        .map_or(168, |at| {
            (tx.now().0.saturating_sub(at.0) / HOUR.0).min(168)
        }) as i64)
}
fn conversation<R: Request>(
    tx: &mut Transaction<'_>,
    p: PersonId,
    q: PersonId,
    key: String,
    value: i64,
    request: R,
) -> Result<()> {
    let mut offer = discussion(p, q, key, request)?;
    offer.motive.value = value * age(tx, p, q)? / 168;
    game_choice::submit(tx, p, offer)
}
fn discussion<R: Request>(
    p: PersonId,
    q: PersonId,
    key: String,
    request: R,
) -> Result<game_choice::Proposal> {
    game_choice::Proposal::new(
        key,
        game_choice::motive(0, 0, 0, 5, 0),
        game_choice::Undertake {
            person: p,
            kind: "discuss-exchange".into(),
            duration: SimDuration(5 * MINUTE.0),
            request: Pending::new(Discuss {
                person: p,
                peer: q,
                request: Pending::new(request)?,
            })?,
        },
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("credit", 1)?;
    b.resource::<Credit>()?;
    b.request::<Discuss>(|tx, r| {
        let result = tx.attempt(game_choice::Run(r.request));
        let at = tx.now();
        tx.edit::<Credit>()?
            .discussed
            .entry(r.person)
            .or_default()
            .insert(r.peer, at);
        Ok(result.unwrap_or(game_choice::Outcome::Reconsider))
    })?;
    b.request::<TellPromise>(|tx, r| {
        local(tx, r.speaker, r.listener)?;
        let terms = tx
            .get::<Credit>()?
            .promise(r.speaker, r.agreement)
            .cloned()
            .ok_or_else(|| Error::Missing("remembered promise".into()))?;
        if trust(tx, r.listener, r.speaker) + disposition(tx, r.listener, "openness") / 20 < 0 {
            return Ok(false);
        }
        remember(tx, r.listener, r.agreement, terms)?;
        Ok(true)
    })?;
    b.request::<Pay>(|tx, r| {
        let actual = tx
            .get::<game_agreements::Agreements>()?
            .0
            .get(&r.agreement)
            .and_then(|a| a.terms.get(r.term));
        require(
            actual == Some(&r.expected),
            "creditor or terms need confirmation",
        )?;
        tx.call(game_agreements::Fulfill {
            person: r.person,
            agreement: r.agreement,
            term: r.term,
        })
    })?;
    b.subscribe::<game_agreements::Settled>("credit.witnessed-payment", |tx, e| {
        let positions = tx.get::<game_transport::Positions>()?;
        let debtor_area = game_transport::area(positions, e.debtor).ok();
        let witnessed = debtor_area.is_some()
            && game_transport::area(positions, e.creditor).ok() == debtor_area;
        for person in [e.debtor, e.creditor] {
            if person == e.debtor || witnessed {
                tx.edit::<Credit>()?
                    .paid
                    .entry(person)
                    .or_default()
                    .entry(e.agreement)
                    .or_default()
                    .insert(e.term);
            }
        }
        Ok(())
    })?;
    b.request::<Borrow>(|tx, r| {
        local(tx, r.borrower, r.lender)?;
        require(
            (1..=1000).contains(&r.quantity)
                && (1..=10000).contains(&r.repayment)
                && r.due > tx.now(),
            "invalid deferred terms",
        )?;
        let inventory = pack(tx, r.lender)?;
        let destination = pack(tx, r.borrower)?;
        let body = &tx.get::<game_physiology::Bodies>()?.people[&r.lender];
        let held = tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(r.good);
        let generosity = disposition(tx, r.lender, "altruism") / 10;
        let confidence = trust(tx, r.lender, r.borrower) + generosity;
        let risk = (1000 - disposition(tx, r.lender, "risk-tolerance")) / 20;
        let delay = (r.due.0 - tx.now().0) / DAY.0;
        let motive = (i128::from(confidence) * i128::from(r.repayment) / i128::from(r.quantity))
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        require(
            held >= r.quantity
                && motive > risk + delay.min(1000) as i64 + body.hunger.min(500) as i64 / 5,
            "lender refuses the risk or cost",
        )?;
        let agreement = tx.call(game_agreements::Offer {
            proposer: r.borrower,
            parties: BTreeSet::from([r.borrower, r.lender]),
            terms: vec![
                game_agreements::Term {
                    debtor: r.lender,
                    creditor: r.borrower,
                    from: inventory,
                    to: destination,
                    good: r.good,
                    quantity: r.quantity,
                    due: tx.now().after(HOUR)?,
                },
                game_agreements::Term {
                    debtor: r.borrower,
                    creditor: r.lender,
                    from: destination,
                    to: inventory,
                    good: r.good,
                    quantity: r.repayment,
                    due: r.due,
                },
            ],
        })?;
        tx.call(game_agreements::Accept {
            person: r.lender,
            agreement,
        })?;
        tx.call(game_agreements::Fulfill {
            person: r.lender,
            agreement,
            term: 0,
        })?;
        let terms = tx.get::<game_agreements::Agreements>()?.0[&agreement]
            .terms
            .clone();
        for person in [r.borrower, r.lender] {
            remember(tx, person, agreement, terms.clone())?;
        }
        Ok(agreement)
    })?;
    b.request::<Endorse>(|tx, r| {
        local(tx, r.holder, r.recipient)?;
        let mut terms = tx
            .get::<Credit>()?
            .promise(r.holder, r.agreement)
            .cloned()
            .ok_or_else(|| Error::Missing("remembered promise".into()))?;
        let agreement = &tx.get::<game_agreements::Agreements>()?.0[&r.agreement];
        let term = agreement
            .terms
            .get(r.term)
            .ok_or_else(|| Error::Missing("promise term".into()))?
            .clone();
        require(
            term.creditor == r.holder && !agreement.fulfilled.contains(&r.term),
            "promise is not held or already paid",
        )?;
        require(
            terms.get(r.term) == Some(&term),
            "promise needs confirmation",
        )?;
        // The recipient evaluates their own regard for the debtor; the holder cannot inspect it beforehand.
        if trust(tx, r.recipient, term.debtor) + disposition(tx, r.recipient, "risk-tolerance") / 20
            < 30
        {
            return Ok(false);
        }
        let from = pack(tx, r.recipient)?;
        let to = pack(tx, r.holder)?;
        if !game_exchange::accept_promise(tx, r.recipient, from, &term, r.good, r.quantity)? {
            return Ok(false);
        }
        tx.call(game_property::Take {
            actor: r.recipient,
            transfer: game_cargo::Transfer {
                from,
                to,
                good: r.good,
                quantity: r.quantity,
            },
            unauthorized: true,
        })?;
        tx.call(game_agreements::Assign {
            creditor: r.holder,
            recipient: r.recipient,
            agreement: r.agreement,
            term: r.term,
            to: from,
        })?;
        let described = terms
            .get_mut(r.term)
            .ok_or_else(|| Error::Missing("remembered term".into()))?;
        described.creditor = r.recipient;
        described.to = from;
        for person in [r.holder, r.recipient] {
            remember(tx, person, r.agreement, terms.clone())?;
        }
        tx.edit::<Credit>()?.assigned += 1;
        Ok(true)
    })?;
    b.request::<ProposeUnit>(|tx, r| {
        local(tx, r.speaker, r.listener)?;
        require(
            r.quantity > 0
                && tx
                    .get::<Credit>()?
                    .units
                    .get(&r.speaker)
                    .is_some_and(|u| u.contains_key(&r.good)),
            "speaker has no experienced convention",
        )?;
        let existing = tx
            .get::<Credit>()?
            .units
            .get(&r.listener)
            .and_then(|u| u.get(&r.good));
        let familiarity = existing.map_or(0, |u| u.exchanges.min(100) as i64);
        if trust(tx, r.listener, r.speaker) + disposition(tx, r.listener, "openness") / 20
            < familiarity
        {
            return Ok(false);
        }
        tx.edit::<Credit>()?
            .units
            .entry(r.listener)
            .or_default()
            .entry(r.good)
            .or_default()
            .quantity = r.quantity;
        Ok(true)
    })?;
    b.subscribe::<game_exchange::Traded>("credit.observed-units", |tx, e| {
        for (p, q) in [(e.first, e.second), (e.second, e.first)] {
            for (good, quantity) in [
                (e.offered, e.offered_quantity),
                (e.requested, e.requested_quantity),
            ] {
                let unit = tx
                    .edit::<Credit>()?
                    .units
                    .entry(p)
                    .or_default()
                    .entry(good)
                    .or_default();
                unit.quantity = gcd(unit.quantity, quantity);
                unit.partners.insert(q);
                unit.exchanges = unit.exchanges.saturating_add(1);
            }
        }
        Ok(())
    })?;
    game_choice::provider(b, "credit.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        let known: Vec<_> = tx
            .get::<Credit>()?
            .known
            .get(&p)
            .into_iter()
            .flatten()
            .copied()
            .take(32)
            .collect();
        for agreement in known {
            let a = tx.get::<Credit>()?.promise(p, agreement);
            let paid = tx
                .get::<Credit>()?
                .paid
                .get(&p)
                .and_then(|row| row.get(&agreement));
            let terms: Vec<_> = a
                .into_iter()
                .flatten()
                .enumerate()
                .filter(|(i, t)| t.debtor == p && !paid.is_some_and(|s| s.contains(i)))
                .map(|(i, t)| (i, t.creditor, t.due, t.clone()))
                .collect();
            for (term, creditor, due, expected) in terms {
                let honesty = tx
                    .get::<game_relationships::Social>()?
                    .beliefs
                    .get(&p)
                    .map_or(0, |b| i64::from(b.honesty));
                let urgency = if due <= tx.now() { 30 } else { 0 };
                let value = honesty * 5
                    + trust(tx, p, creditor)
                    + urgency
                    + disposition(tx, p, "altruism") / 20;
                game_choice::offer(
                    tx,
                    p,
                    format!("credit:repay:{}:{term}", agreement.0),
                    game_choice::motive(0, 0, value, 5, 0),
                    Pay {
                        person: p,
                        agreement,
                        term,
                        expected,
                    },
                )?;
            }
        }
        let bodies = tx.get::<game_physiology::Bodies>()?;
        let hunger = bodies.people[&p].hunger.min(300) as i64;
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let peers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()?
            .residents
            .get(&area)
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .filter(|q| *q != p)
            .take(8)
            .collect();
        let units: Vec<_> = tx
            .get::<Credit>()?
            .units
            .get(&p)
            .into_iter()
            .flat_map(|r| r.iter())
            .filter(|(_, u)| u.exchanges >= 2 && u.quantity > 0)
            .take(2)
            .map(|(g, u)| (*g, u.quantity))
            .collect();
        let claims: Vec<_> = tx
            .get::<Credit>()?
            .promises
            .get(&p)
            .into_iter()
            .flat_map(|r| r.iter())
            .flat_map(|(id, terms)| terms.iter().enumerate().map(move |(i, t)| (*id, i, t)))
            .filter(|(_, _, t)| t.creditor == p)
            .take(4)
            .map(|(id, i, t)| (id, i, t.clone()))
            .collect();
        let foresight = ability(tx, p, "foresight").min(16000) as i64 / 20;
        for peer in &peers {
            for (good, quantity) in units.iter().copied() {
                unit_offer(tx, p, *peer, good, quantity, foresight)?;
            }
            for (agreement, term, t) in claims.iter().cloned() {
                conversation(
                    tx,
                    p,
                    *peer,
                    format!("credit:endorse:{}:{}:{term}", peer.0, agreement.0),
                    hunger + 30,
                    Endorse {
                        holder: p,
                        recipient: *peer,
                        agreement,
                        term,
                        good: t.good,
                        quantity: t.quantity.div_ceil(2),
                    },
                )?;
            }
        }
        if hunger < 30 {
            return Ok(());
        }
        let Ok(own) = pack(tx, p) else { return Ok(()) };
        let good = tx
            .get::<game_nutrition::Nutrition>()
            .ok()
            .and_then(|n| n.people.get(&p))
            .and_then(|d| d.learned.keys().next())
            .copied()
            .or_else(|| {
                tx.get::<game_objects::Objects>()
                    .ok()?
                    .food_rules()
                    .keys()
                    .find(|g| tx.get::<game_cargo::Cargo>().unwrap().0[&own].amount(**g) > 0)
                    .copied()
            });
        for lender in peers {
            if let Some(good) = good {
                let quantity = tx
                    .get::<game_objects::Objects>()?
                    .by_id(good)?
                    .food
                    .as_ref()
                    .unwrap()
                    .portion;
                let horizon = ability(tx, p, "foresight").min(16000);
                let premium = (1000 - horizon as i64 / 20
                    + hunger * 3
                    + disposition(tx, p, "risk-tolerance") / 2
                    - trust(tx, p, lender) * 2)
                    .clamp(0, 3000) as u64;
                let repayment = quantity.saturating_add(quantity.saturating_mul(premium) / 1000);
                let request = Borrow {
                    borrower: p,
                    lender,
                    good,
                    quantity,
                    repayment,
                    due: tx.now().after(SimDuration((1 + horizon / 4000) * DAY.0))?,
                };
                conversation(
                    tx,
                    p,
                    lender,
                    format!("credit:ask:{}:{}", lender.0, good.0),
                    hunger,
                    request,
                )?;
            }
        }
        Ok(())
    })
}

thread_local! {static UNITS:std::cell::RefCell<game_choice::Templates<(PersonId,PersonId,GoodId,u64)>>=std::cell::RefCell::new(game_choice::Templates::new(4096));}
fn unit_offer(
    tx: &mut Transaction<'_>,
    p: PersonId,
    q: PersonId,
    good: GoodId,
    quantity: u64,
    value: i64,
) -> Result<()> {
    let mut offer = UNITS.with(|cache| {
        cache.borrow_mut().get((p, q, good, quantity), || {
            discussion(
                p,
                q,
                format!("credit:unit:{}:{}", q.0, good.0),
                ProposeUnit {
                    speaker: p,
                    listener: q,
                    good,
                    quantity,
                },
            )
        })
    })?;
    offer.motive.value = value * age(tx, p, q)? / 168;
    game_choice::submit(tx, p, offer)
}
