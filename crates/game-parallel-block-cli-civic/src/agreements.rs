//! Commands for explicit delivery promises.
use super::{end, number};
use game_parallel::{AgreementDecision, AgreementDelivery, AgreementProposal};
use game_parallel_fused_physical::Kernel;
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("agreement-policy") => {
            let person = number(&mut words, "person")?;
            let (honor, bulk) = match words.next() {
                Some("ignore") => (false, false),
                Some("honor") => (true, false),
                Some("bulk") => (true, true),
                _ => return Err("policy must be ignore, honor, or bulk".into()),
            };
            end(&mut words)?;
            engine.set_agreement_honor(person, honor, bulk)?;
        }
        Some("agreement-propose") => {
            let debtor = number(&mut words, "debtor")?;
            let creditor = number(&mut words, "creditor")?;
            let good = number(&mut words, "good")?;
            let quantity = number(&mut words, "quantity")?;
            let due = number(&mut words, "due tick")?;
            end(&mut words)?;
            let ids = engine.propose_agreements(&[AgreementProposal {
                debtor,
                creditor,
                good,
                quantity,
                due,
            }])?;
            println!("agreement={}", ids[0]);
        }
        Some("agreement-decide") => {
            let contract = number(&mut words, "agreement")?;
            let actor = number(&mut words, "actor")?;
            let accept = match words.next() {
                Some("accept") => true,
                Some("decline") => false,
                _ => return Err("decision must be accept or decline".into()),
            };
            end(&mut words)?;
            engine.decide_agreements(&[AgreementDecision {
                contract,
                actor,
                accept,
            }])?;
        }
        Some("agreement-deliver") => {
            let contract = number(&mut words, "agreement")?;
            let actor = number(&mut words, "actor")?;
            let quantity = number(&mut words, "quantity")?;
            end(&mut words)?;
            engine.deliver_agreements(&[AgreementDelivery {
                contract,
                actor,
                quantity,
            }])?;
        }
        Some("agreements") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let state = world.agreements.as_ref().ok_or("agreements are disabled")?;
            let policy = state.policies.iter().find(|policy| policy.person == person);
            let honor = policy.is_some_and(|policy| policy.honor);
            let fear = policy.and_then(|policy| policy.fear_honor_threshold);
            println!(
                "person={person} honor={honor} bulk={} fear_honor_threshold={fear:?}",
                state.bulk_debtors.contains(&person)
            );
            for c in state
                .contracts
                .iter()
                .filter(|c| c.debtor == person || c.creditor == person)
            {
                println!(
                    "agreement={} debtor={} creditor={} good={} delivered={} quantity={} due={} status={:?}",
                    c.id, c.debtor, c.creditor, c.good, c.delivered, c.quantity, c.due, c.status
                );
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
