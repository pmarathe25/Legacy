//! Local offers, individual consent, and atomic fulfillment through agreements.
mod valuation;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub use valuation::{accept_promise, credit_trust, marginal};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    pub enabled: bool,
    pub next: Table<PersonId, SimTime>,
    pub completed: u64,
    pub aid_requests: u64,
    #[serde(default)]
    pub failures: Table<PersonId, Table<PersonId, u32>>,
}
impl Resource for Exchange {
    const KEY: &'static str = "exchange.encounters";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "exchange.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Barter {
    pub first: PersonId,
    pub second: PersonId,
    pub offered: GoodId,
    pub requested: GoodId,
    pub quantity: u64,
}
impl Request for Barter {
    const KEY: &'static str = "exchange.barter";
    type Response = AgreementId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Trade {
    pub first: PersonId,
    pub second: PersonId,
    pub offered: GoodId,
    pub requested: GoodId,
    pub offered_quantity: u64,
    pub requested_quantity: u64,
}
impl Request for Trade {
    const KEY: &'static str = "exchange.trade";
    type Response = AgreementId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Traded {
    pub first: PersonId,
    pub second: PersonId,
    pub agreement: AgreementId,
    pub offered: GoodId,
    pub requested: GoodId,
    pub offered_quantity: u64,
    pub requested_quantity: u64,
}
impl Event for Traded {
    const KEY: &'static str = "exchange.traded";
}
fn pack(tx: &Transaction<'_>, p: PersonId) -> Option<InventoryId> {
    tx.get::<game_cargo::Cargo>().ok()?.carried(p).next()
}
fn willing(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> Result<bool> {
    let tie = tx
        .get::<game_relationships::Social>()?
        .relationships
        .get(&p)
        .and_then(|r| r.get(&other));
    let regard = tie.map_or(0, |r| {
        i64::from(r.trust + r.affection / 2 - r.grievance - r.fear)
    });
    let patience = game_vitality::disposition(tx, p, "patience");
    let openness = game_vitality::disposition(tx, p, "openness");
    let occupied = tx.get::<Actions>()?.0.contains_key(&p);
    Ok(Some(p) != tx.get::<game_lineage::Lineage>()?.current
        && game_physiology::capable(tx, p).is_ok()
        && regard + (patience + openness) / 20 > if occupied { 80 } else { 10 })
}
fn encounter(tx: &mut Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    if !tx.get::<Exchange>()?.enabled {
        return Ok(());
    }
    let now = tx.now();
    tx.edit::<Exchange>()?.next.insert(a, now);
    let (Some(first), Some(second)) = (pack(tx, a), pack(tx, b)) else {
        return Ok(());
    };
    // Bounded, voluntary disclosure. Neither party sees undisclosed stocks or beliefs.
    let offers = |p, id| {
        let attention =
            (2 + game_vitality::ability(tx, p, "intelligence") / 250).clamp(2, 64) as usize;
        let goods: Vec<_> = tx.get::<game_cargo::Cargo>().unwrap().0[&id]
            .goods
            .iter()
            .filter(|(_, q)| **q > 0)
            .collect();
        let start = random(tx.seed(), p.0, tx.now().0 / MINUTE.0) as usize % goods.len().max(1);
        goods
            .iter()
            .cycle()
            .skip(start)
            .take(goods.len().min(attention))
            .filter(|(g, _)| game_sentiment::relinquishing(tx, p, id, **g, 1) < 100)
            .map(|(g, q)| (**g, (**q).min(64)))
            .collect::<Vec<_>>()
    };
    let custom_checks = tx.has_before::<Trade>();
    // Each participant can decline before spending attention on possible terms.
    if !custom_checks
        && (willing(tx, a, b).is_ok_and(|yes| !yes) || willing(tx, b, a).is_ok_and(|yes| !yes))
    {
        return Ok(());
    }
    let first_offers = offers(a, first);
    let second_offers = offers(b, second);
    let budget = (game_vitality::ability(tx, a, "intelligence") / 125).clamp(1, 64) as usize;
    let mut proposals = vec![];
    let mut values = valuation::Offers::new(a, first);
    for (offered, held) in first_offers {
        for (requested, available) in &second_offers {
            if offered == *requested {
                continue;
            }
            let costs = [1, 2, 4, 8]
                .into_iter()
                .filter(|q| *q <= held)
                .map(|q| Ok((q, values.cost(tx, offered, q)?)))
                .collect::<Result<Vec<_>>>()?;
            let benefits = [1, 2, 4, 8]
                .into_iter()
                .filter(|q| *q <= *available)
                .map(|q| Ok((q, values.benefit(tx, *requested, q)?)))
                .collect::<Result<Vec<_>>>()?;
            for (offered_quantity, (cost, attachment)) in costs {
                for (requested_quantity, benefit) in &benefits {
                    let gain = benefit - cost - attachment;
                    let r = Trade {
                        first: a,
                        second: b,
                        offered,
                        requested: *requested,
                        offered_quantity,
                        requested_quantity: *requested_quantity,
                    };
                    if gain > 0 {
                        proposals.push((gain, r));
                    }
                }
            }
        }
    }
    let rank = |(gain, r): &(i64, Trade)| {
        (
            std::cmp::Reverse(*gain),
            r.offered,
            r.requested,
            r.offered_quantity,
            r.requested_quantity,
        )
    };
    if proposals.len() > budget {
        proposals.select_nth_unstable_by_key(budget, rank);
        proposals.truncate(budget);
    }
    proposals.sort_unstable_by_key(rank);
    let mut recipient = valuation::Offers::new(b, second);
    for (_, r) in proposals.into_iter().take(budget) {
        // The recipient evaluates their own unchanged snapshot. Failed terms still
        // consume one proposal slot; settlement rechecks both parties and all hooks.
        if !custom_checks && recipient.accepts(tx, &r).is_ok_and(|accepts| !accepts) {
            continue;
        }
        if tx.attempt(r).is_ok() {
            tx.edit::<Exchange>()?
                .failures
                .entry(a)
                .or_default()
                .remove(&b);
            return Ok(());
        }
    }
    let n = tx
        .edit::<Exchange>()?
        .failures
        .entry(a)
        .or_default()
        .entry(b)
        .or_default();
    *n = n.saturating_add(1).min(100);
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Negotiate {
    person: PersonId,
    other: PersonId,
}
impl Request for Negotiate {
    const KEY: &'static str = "exchange.negotiate";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("exchange", 1)?;
    b.resource::<Exchange>()?;
    b.request::<Negotiate>(|tx, r| encounter(tx, r.person, r.other))?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Exchange>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Barter>(|tx, r| {
        tx.call(Trade {
            first: r.first,
            second: r.second,
            offered: r.offered,
            requested: r.requested,
            offered_quantity: r.quantity,
            requested_quantity: r.quantity,
        })
    })?;
    b.request::<Trade>(|tx, r| {
        require(
            r.first != r.second
                && r.offered != r.requested
                && r.offered_quantity > 0
                && r.requested_quantity > 0,
            "invalid barter",
        )?;
        require(
            willing(tx, r.first, r.second)? && willing(tx, r.second, r.first)?,
            "offer refused",
        )?;
        require(
            game_transport::area(tx.get()?, r.first)? == game_transport::area(tx.get()?, r.second)?,
            "exchange needs contact",
        )?;
        let first = pack(tx, r.first).ok_or_else(|| Error::Missing("first pack".into()))?;
        let second = pack(tx, r.second).ok_or_else(|| Error::Missing("second pack".into()))?;
        require(
            valuation::gain(
                tx,
                r.first,
                first,
                r.offered,
                r.offered_quantity,
                r.requested,
                r.requested_quantity,
            )? > 0
                && valuation::gain(
                    tx,
                    r.second,
                    second,
                    r.requested,
                    r.requested_quantity,
                    r.offered,
                    r.offered_quantity,
                )? > 0,
            "terms refused",
        )?;
        let due = tx.now().after(HOUR)?;
        let terms = vec![
            game_agreements::Term {
                debtor: r.first,
                creditor: r.second,
                from: first,
                to: second,
                good: r.offered,
                quantity: r.offered_quantity,
                due,
            },
            game_agreements::Term {
                debtor: r.second,
                creditor: r.first,
                from: second,
                to: first,
                good: r.requested,
                quantity: r.requested_quantity,
                due,
            },
        ];
        let agreement = tx.call(game_agreements::Offer {
            proposer: r.first,
            parties: BTreeSet::from([r.first, r.second]),
            terms,
        })?;
        tx.call(game_agreements::Accept {
            person: r.second,
            agreement,
        })?;
        tx.call(game_agreements::Fulfill {
            person: r.first,
            agreement,
            term: 0,
        })?;
        tx.call(game_agreements::Fulfill {
            person: r.second,
            agreement,
            term: 1,
        })?;
        tx.edit::<Exchange>()?.completed += 1;
        if tx.registered::<game_demand::ObserveSettlement>() {
            for observer in [r.first, r.second] {
                tx.call(game_demand::ObserveSettlement {
                    observer,
                    agreement,
                })?;
            }
        }
        tx.emit(
            Traded {
                first: r.first,
                second: r.second,
                agreement,
                offered: r.offered,
                requested: r.requested,
                offered_quantity: r.offered_quantity,
                requested_quantity: r.requested_quantity,
            },
            vec![r.first, r.second],
        )?;
        Ok(agreement)
    })?;
    b.subscribe::<game_social_life::Encounter>("exchange.offers", |tx, e| {
        if tx.get::<game_characters::Minds>().is_ok_and(|m| m.running) {
            tx.notify(game_choice::Changed { person: e.first })?;
            tx.notify(game_choice::Changed { person: e.second })
        } else {
            encounter(tx, e.first, e.second)
        }
    })?;
    game_choice::provider(b, "exchange.opportunities", |tx, e| {
        if !tx.get::<Exchange>()?.enabled
            || Some(e.person) == tx.get::<game_lineage::Lineage>()?.current
        {
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
            .flat_map(|r| {
                r.scan_after(Some(&e.person))
                    .chain(r.iter())
                    .take(r.len().min(16))
            })
            .map(|(p, _)| *p)
            .filter(|p| *p != e.person)
            .collect();
        let own = &tx.get::<game_physiology::Bodies>()?.people[&e.person];
        let hunger = own.hunger.min(200) as i64;
        for other in peers {
            let recent = tx.get::<Exchange>()?.next.get(&e.person).map_or(0, |at| {
                30i64.saturating_sub((tx.now().0.saturating_sub(at.0) / MINUTE.0).min(30) as i64)
            });
            let failures = tx
                .get::<Exchange>()?
                .failures
                .get(&e.person)
                .and_then(|r| r.get(&other))
                .copied()
                .unwrap_or(0);
            let value = 20 + hunger / 4 + game_vitality::disposition(tx, e.person, "openness") / 50;
            game_choice::offer(
                tx,
                e.person,
                format!("exchange:{}", other.0),
                game_choice::motive(0, 0, value, 5 + recent + i64::from(failures) * 10, 0),
                game_choice::Undertake {
                    person: e.person,
                    kind: "negotiate".into(),
                    duration: SimDuration(5 * MINUTE.0),
                    request: Pending::new(Negotiate {
                        person: e.person,
                        other,
                    })?,
                },
            )?;
        }
        Ok(())
    })?;
    Ok(())
}
