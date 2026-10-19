//! Recovery, gifts, remembrance and animal care compete with other personally chosen work.
use game_core::*;
use game_sentiment::{Gift, Recall, Sentiment, Target, TellMemory, value};
use game_sim::*;
mod animals;
pub use animals::Hunt;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Recover {
    person: PersonId,
    item: u64,
    source: InventoryId,
    destination: InventoryId,
}
impl Request for Recover {
    const KEY: &'static str = "attachments.recover";
    type Response = game_choice::Outcome;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("attachments", 1)?;
    b.request::<Recover>(|tx, r| {
        if !tx.call(game_sentiment::Locate {
            person: r.person,
            item: r.item,
            inventory: r.source,
        })? {
            return Ok(game_choice::Outcome::Reconsider);
        }
        tx.call(game_items::Move {
            person: r.person,
            item: r.item,
            destination: r.destination,
        })?;
        Ok(game_choice::Outcome::Performed)
    })?;
    animals::register(b)?;
    game_choice::provider(b, "sentiment.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let memories: Vec<_> = tx
            .get::<Sentiment>()?
            .people
            .get(&p)
            .into_iter()
            .flat_map(|r| r.values())
            .filter(|m| m.value > 0)
            .take(16)
            .map(|m| (m.target.clone(), m.value))
            .collect();
        let peers: Vec<_> = tx
            .get::<game_relationships::Social>()?
            .relationships
            .get(&p)
            .into_iter()
            .flat_map(|r| r.iter())
            .filter(|(q, r)| {
                r.affection > 0 && game_transport::area(tx.get().unwrap(), **q).ok() == Some(area)
            })
            .map(|(q, r)| (*q, r.affection))
            .take(4)
            .collect();
        let since = tx
            .get::<Sentiment>()?
            .recalled
            .get(&p)
            .map_or(DAY.0, |at| tx.now().0.saturating_sub(at.0));
        for (target, sentiment) in memories {
            let value = i64::from(sentiment) * since.min(DAY.0) as i64 / DAY.0 as i64 / 10;
            let mut offer = prepared(p, target.clone(), None)?;
            offer.motive.value = value;
            offer.motive.effort = 10;
            game_choice::submit(tx, p, offer)?;
            if let Target::Object(item) = target {
                let seen = tx
                    .get::<Sentiment>()?
                    .locations
                    .get(&p)
                    .and_then(|r| r.get(&item))
                    .copied();
                let own = tx.get::<game_cargo::Cargo>()?.carried(p).next();
                if let (Some(source), Some(destination)) = (seen, own) {
                    if source != destination
                        && tx
                            .get::<game_knowledge::Knowledge>()?
                            .0
                            .get(&p)
                            .and_then(|f| {
                                game_knowledge::known::<game_knowledge::StoreLocation>(
                                    f,
                                    &game_knowledge::location_key(source),
                                )
                            })
                            .is_some_and(|l| l.area == area)
                    {
                        game_choice::offer(
                            tx,
                            p,
                            format!("memory:recover:{item}"),
                            game_choice::motive(
                                0,
                                0,
                                i64::from(sentiment) / 5,
                                5,
                                game_property::reluctance(tx, p, source),
                            ),
                            Recover {
                                person: p,
                                item,
                                source,
                                destination,
                            },
                        )?;
                    }
                    if source == destination {
                        for (recipient, affection) in &peers {
                            let generosity = game_vitality::disposition(tx, p, "altruism") / 10
                                + i64::from(*affection)
                                - i64::from(sentiment) / 5;
                            game_choice::offer(
                                tx,
                                p,
                                format!("memory:gift:{item}:{}", recipient.0),
                                game_choice::motive(0, 0, generosity, 5, 0),
                                Gift {
                                    person: p,
                                    recipient: *recipient,
                                    item,
                                },
                            )?;
                        }
                    }
                }
            }
            for (listener, affection) in &peers {
                let mut offer = prepared(p, target.clone(), Some(*listener))?;
                offer.motive.value = i64::from(*affection) / 2;
                offer.motive.effort = 5;
                game_choice::submit(tx, p, offer)?;
            }
        }
        Ok(())
    })
}

thread_local! {static PREPARED:std::cell::RefCell<game_choice::Templates<(PersonId,Target,Option<PersonId>)>>=std::cell::RefCell::new(game_choice::Templates::new(8192));}
fn prepared(
    person: PersonId,
    target: Target,
    listener: Option<PersonId>,
) -> Result<game_choice::Proposal> {
    PREPARED.with(|cache| {
        cache
            .borrow_mut()
            .get((person, target.clone(), listener), || {
                if let Some(listener) = listener {
                    game_choice::Proposal::new(
                        format!("memory:tell:{}:{}", target.key(), listener.0),
                        game_choice::motive(0, 0, 0, 5, 0),
                        game_choice::Undertake {
                            person,
                            kind: "recount-memory".into(),
                            duration: SimDuration(10 * MINUTE.0),
                            request: Pending::new(TellMemory {
                                person,
                                listener,
                                target,
                            })?,
                        },
                    )
                } else {
                    game_choice::Proposal::new(
                        format!("memory:{}", target.key()),
                        game_choice::motive(0, 0, 0, 10, 0),
                        game_choice::Undertake {
                            person,
                            kind: "reminisce".into(),
                            duration: HOUR,
                            request: Pending::new(Recall { person, target })?,
                        },
                    )
                }
            })
    })
}
