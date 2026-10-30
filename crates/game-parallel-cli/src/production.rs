use crate::{end, number};
use game_parallel::{
    Amount, GoodsDirection, GoodsTransfer, StartWork, TradeOutcome, TradeProposal, World,
};
use std::str::SplitWhitespace;

pub fn command(
    world: &mut World,
    verb: &str,
    words: &mut SplitWhitespace<'_>,
) -> Result<(), String> {
    match verb {
        "work" => {
            let job = StartWork {
                id: number(words, "job")?,
                worker: number(words, "worker")?,
                region: number(words, "region")?,
                recipe: number(words, "recipe")?,
                units: number(words, "units")?,
            };
            end(words)?;
            world.start_work(&[job])?;
            println!("job={} started", job.id);
        }
        "cancel-work" => {
            let id = number(words, "job")?;
            end(words)?;
            world.cancel_work(id)?;
            println!("job={id} cancelled");
        }
        "assign-work" => {
            let id = number(words, "job")?;
            let worker = number(words, "worker")?;
            end(words)?;
            world.reassign_work(id, worker)?;
            println!("job={id} worker={worker}");
        }
        "pickup-good" | "deposit-good" => {
            let person = number(words, "person")?;
            let good = number(words, "good")?;
            let quantity = number(words, "quantity")?;
            end(words)?;
            let direction = if verb == "pickup-good" {
                GoodsDirection::Pickup
            } else {
                GoodsDirection::Deposit
            };
            world.transfer_goods(&[GoodsTransfer {
                person,
                good,
                quantity,
                direction,
            }])?;
            println!("person={person} good={good} quantity={quantity} {verb}");
        }
        "goods" => {
            let person: u64 = number(words, "person")?;
            end(words)?;
            let cargo = world
                .goods_cargo
                .as_ref()
                .ok_or("goods cargo is disabled")?;
            if let Some(holding) = cargo
                .holdings
                .iter()
                .find(|holding| holding.person == person)
            {
                for item in &holding.items {
                    println!(
                        "person={person} good={} quantity={}",
                        item.good, item.quantity
                    );
                }
            }
        }
        "offer" => {
            let a = number(words, "person a")?;
            let good_a = number(words, "good a")?;
            let quantity_a = number(words, "quantity a")?;
            let b = number(words, "person b")?;
            let good_b = number(words, "good b")?;
            let quantity_b = number(words, "quantity b")?;
            end(words)?;
            let offer_a = if quantity_a == 0 {
                vec![]
            } else {
                vec![Amount {
                    good: good_a,
                    quantity: quantity_a,
                }]
            };
            let offer_b = if quantity_b == 0 {
                vec![]
            } else {
                vec![Amount {
                    good: good_b,
                    quantity: quantity_b,
                }]
            };
            let id = world.propose_trade(TradeProposal {
                proposer: a,
                recipient: b,
                give: offer_a,
                receive: offer_b,
            })?;
            println!("offer={id} proposer={a} recipient={b}");
        }
        "accept" | "decline" | "withdraw" => {
            let id = number(words, "offer")?;
            let actor = number(words, "actor")?;
            end(words)?;
            let outcome = match verb {
                "accept" => TradeOutcome::Accepted,
                "decline" => TradeOutcome::Declined,
                _ => TradeOutcome::Withdrawn,
            };
            world.decide_trade(id, actor, outcome)?;
            println!("offer={id} actor={actor} outcome={outcome:?}");
        }
        "offers" => {
            let person: u64 = number(words, "person")?;
            end(words)?;
            let market = world.market.as_ref().ok_or("market is disabled")?;
            for offer in market
                .offers
                .iter()
                .filter(|offer| offer.proposer == person || offer.recipient == person)
            {
                println!(
                    "offer={} proposer={} recipient={} give={:?} receive={:?}",
                    offer.id, offer.proposer, offer.recipient, offer.give, offer.receive
                );
            }
        }
        "recover-good" => {
            let actor = number(words, "actor")?;
            let deceased = number(words, "deceased")?;
            let good = number(words, "good")?;
            let quantity = number(words, "quantity")?;
            end(words)?;
            world.recover_goods(actor, deceased, good, quantity)?;
            println!("person={actor} recovered good={good} quantity={quantity}");
        }
        "inventory" => {
            let region: u64 = number(words, "region")?;
            end(words)?;
            let production = world.production.as_ref().ok_or("production is disabled")?;
            let inventory = production
                .inventories
                .iter()
                .find(|i| i.region == region)
                .ok_or("unknown inventory")?;
            for stock in &inventory.stocks {
                println!(
                    "good={} quantity={} capacity={}",
                    stock.good, stock.quantity, stock.capacity
                );
            }
            for job in production.jobs.iter().filter(|j| j.region == region) {
                println!(
                    "job={} worker={} recipe={} remaining={} phase={:?}",
                    job.id, job.worker, job.recipe, job.remaining, job.phase
                );
            }
        }
        _ => return Err("unknown command; use help".into()),
    }
    Ok(())
}
