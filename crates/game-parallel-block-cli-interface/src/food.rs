//! Individual food choices, requests and physical gifts.
use game_parallel_fused_physical::Kernel;
fn number<T: std::str::FromStr>(
    words: &mut std::str::SplitWhitespace<'_>,
    name: &str,
) -> Result<T, String> {
    words
        .next()
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
fn end(words: &mut std::str::SplitWhitespace<'_>) -> Result<(), String> {
    if words.next().is_some() {
        Err("unexpected argument".into())
    } else {
        Ok(())
    }
}
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("eat-good") => {
            let person = number(&mut words, "person")?;
            let good = number(&mut words, "good")?;
            end(&mut words)?;
            engine.eat_good(person, good)?;
            println!("eat-good person={person} good={good}");
        }
        Some("confrontation") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_confrontation(person, enabled)?;
            println!("confrontation person={person} enabled={enabled}");
        }
        Some("store-building") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_store_building(person, enabled)?;
            println!("store-building person={person} enabled={enabled}");
        }
        Some("deposit-claim") => {
            let person = number(&mut words, "person")?;
            let preference = match words.next() {
                Some("none") => None,
                Some("accept") => Some(false),
                Some("object") => Some(true),
                _ => return Err("expected none, accept, or object".into()),
            };
            end(&mut words)?;
            engine.set_deposit_claim(person, preference)?;
            println!("deposit-claim person={person} preference={preference:?}");
        }
        Some("store-claim") => {
            let person = number(&mut words, "person")?;
            let store = number(&mut words, "store")?;
            let claim = match words.next() {
                Some("assert") => true,
                Some("withdraw") => false,
                _ => return Err("expected assert or withdraw".into()),
            };
            end(&mut words)?;
            engine.set_food_store_claim(person, store, claim)?;
            println!("food-store={store} claimant={person} asserted={claim}");
        }
        Some("store-objection") => {
            let person = number(&mut words, "person")?;
            let store = number(&mut words, "store")?;
            let object = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_store_objection(person, store, object)?;
            println!("food-store={store} claimant={person} object={object}");
        }
        Some("store-build") => {
            let person = number(&mut words, "person")?;
            let store = number(&mut words, "store")?;
            let blueprint = number(&mut words, "blueprint")?;
            end(&mut words)?;
            engine.build_food_store(person, store, blueprint)?;
            println!("food-store={store} builder={person} blueprint={blueprint}");
        }
        Some("food-stores") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let region = world
                .regions
                .iter()
                .find(|r| r.people.iter().any(|p| p.id == person && p.health > 0))
                .ok_or("person must be alive and local")?;
            let cargo = world.cargo.as_ref().ok_or("personal cargo is disabled")?;
            for store in cargo
                .food_stores
                .iter()
                .filter(|store| store.region == region.id)
            {
                let own = store.claimant == Some(person)
                    || cargo
                        .food_store_claims
                        .iter()
                        .any(|claim| claim.store == store.id && claim.person == person);
                println!(
                    "food-store={} food={} capacity={} your-claim={own}",
                    store.id, store.food, store.capacity
                );
            }
        }
        Some("store-take") | Some("store-put") => {
            let deposit = line.split_whitespace().next() == Some("store-put");
            let person = number(&mut words, "person")?;
            let store = number(&mut words, "store")?;
            let amount = number(&mut words, "amount")?;
            end(&mut words)?;
            engine.transfer_food_store(person, store, amount, deposit)?;
            println!("food-store={store} person={person} amount={amount} deposit={deposit}");
        }
        Some("food-source") => {
            let person = number(&mut words, "person")?;
            let source = words.next().ok_or("missing store or region")?;
            let store = if source == "region" {
                None
            } else {
                Some(source.parse().map_err(|_| "invalid store")?)
            };
            end(&mut words)?;
            engine.set_food_source(person, store)?;
            println!("food-source person={person} source={source}");
        }
        Some("food-seeking") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_food_seeking(person, enabled)?;
            println!("food-seeking person={person} enabled={enabled}");
        }
        Some("food-asking") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_food_asking(person, enabled)?;
            println!("food-asking person={person} enabled={enabled}");
        }
        Some("food-deposit") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_food_deposit(person, enabled)?;
            println!("food-deposit person={person} enabled={enabled}");
        }
        Some("food-aid") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_food_aid(person, enabled)?;
            println!("food-aid person={person} enabled={enabled}");
        }
        Some("request-food") => {
            let requester = number(&mut words, "requester")?;
            let giver = number(&mut words, "giver")?;
            let id = number(&mut words, "request")?;
            let amount = number(&mut words, "amount")?;
            let expires_at = number(&mut words, "expires")?;
            end(&mut words)?;
            engine.request_food_batch(&[game_parallel::FoodRequest {
                id,
                requester,
                giver,
                amount,
                expires_at,
            }])?;
            println!("food-request={id} requester={requester} giver={giver}");
        }
        Some("food-requests") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let cargo = world.cargo.as_ref().ok_or("personal cargo is disabled")?;
            for request in cargo
                .food_requests
                .iter()
                .filter(|request| request.requester == person || request.giver == person)
            {
                println!(
                    "food-request={} requester={} giver={} amount={} expires={}",
                    request.id,
                    request.requester,
                    request.giver,
                    request.amount,
                    request.expires_at
                );
            }
        }
        Some("withdraw-food") => {
            let person = number(&mut words, "person")?;
            let id = number(&mut words, "request")?;
            end(&mut words)?;
            engine.withdraw_food_request(person, id)?;
            println!("food-request={id} withdrawn by person={person}");
        }
        Some("give-food") => {
            let giver = number(&mut words, "giver")?;
            let recipient = number(&mut words, "recipient")?;
            let amount = number(&mut words, "food")?;
            end(&mut words)?;
            engine.give_food_batch(&[game_parallel::FoodGift {
                giver,
                recipient,
                amount,
            }])?;
            println!("food giver={giver} recipient={recipient} amount={amount}");
        }
        _ => return Ok(false),
    }
    Ok(true)
}
