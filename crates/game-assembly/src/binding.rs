use game_api::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Execute {
    command: Command,
}
impl Request for Execute {
    const KEY: &'static str = "binding.command";
    type Response = CommandResult;
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.plugin("client-binding", 1)?;
    b.request::<Execute>(|tx, r| {
        let person = tx
            .get::<game_lineage::Lineage>()?
            .current
            .ok_or_else(|| Error::Rejected("no living ruler".into()))?;
        let result = route(tx, person, r.command)?;
        Ok(CommandResult {
            time: tx.now(),
            revision: tx.revision_after(),
            result,
        })
    })
}
fn value<R: Request>(tx: &mut Transaction<'_>, request: R) -> Result<serde_json::Value> {
    serde_json::to_value(tx.call(request)?).map_err(|e| Error::Rejected(e.to_string()))
}
pub fn submit(host: &mut Host, sequence: u64, command: Command) -> Result<CommandResult> {
    host.submit(sequence, Execute { command })
}
fn route(
    tx: &mut Transaction<'_>,
    person: PersonId,
    command: Command,
) -> Result<serde_json::Value> {
    match command {
        Command::Move { destination } => value(
            tx,
            game_transport::Move {
                person,
                destination,
            },
        ),
        Command::Eat { inventory } => value(tx, game_physiology::Eat { person, inventory }),
        Command::Rest => value(tx, game_physiology::Rest { person }),
        Command::PresentWork { work, listener } => value(
            tx,
            game_works::Present {
                holder: person,
                listener,
                work,
            },
        ),
        Command::ReadWork { work } => value(
            tx,
            game_works::Read {
                reader: person,
                work,
            },
        ),
        Command::Observe { inventory } => {
            value(tx, game_knowledge::ObserveStore { person, inventory })
        }
        Command::ObserveSurroundings => value(tx, game_knowledge::ObserveSurroundings { person }),
        Command::ObservePatch { patch } => {
            value(tx, game_knowledge::ObservePatch { person, patch })
        }
        Command::Tell { recipient, key } => value(
            tx,
            game_testimony::Tell {
                sender: person,
                recipient,
                key,
            },
        ),
        Command::Talk { other } => value(tx, game_social_life::Talk { person, other }),
        Command::Take {
            from,
            to,
            good,
            quantity,
            unauthorized,
        } => value(
            tx,
            game_property::Take {
                actor: person,
                transfer: game_cargo::Transfer {
                    from,
                    to,
                    good,
                    quantity,
                },
                unauthorized,
            },
        ),
        Command::Sow { plot, seed } => value(tx, game_agriculture::Sow { person, plot, seed }),
        Command::Harvest { plot, destination } => value(
            tx,
            game_agriculture::Harvest {
                person,
                plot,
                destination,
            },
        ),
        Command::Craft { recipe, inventory } => value(
            tx,
            game_workshop::Craft {
                person,
                recipe,
                inventory,
            },
        ),
        Command::Plant { crop, seed } => value(
            tx,
            game_horticulture::Plant {
                person,
                crop,
                inventory: seed,
            },
        ),
        Command::Pursue {
            activity,
            inventory,
        } => value(
            tx,
            game_pastimes::Pursue {
                person,
                activity,
                inventory,
            },
        ),
        Command::Treat { patient, supplies } => value(
            tx,
            game_health::Treat {
                caregiver: person,
                patient,
                supplies,
            },
        ),
        Command::Gather {
            patch,
            destination,
            quantity,
        } => value(
            tx,
            game_ecology::Gather {
                person,
                patch,
                destination,
                quantity,
            },
        ),
        Command::Haul {
            from,
            to,
            pack,
            good,
            quantity,
        } => value(
            tx,
            game_logistics::Haul {
                carrier: person,
                from,
                to,
                pack,
                good,
                quantity,
            },
        ),
        Command::Cancel => {
            tx.cancel_action(person)?;
            Ok(serde_json::Value::Null)
        }
        Command::Accept { agreement } => value(tx, game_agreements::Accept { person, agreement }),
        Command::Reject { agreement } => value(tx, game_agreements::Reject { person, agreement }),
        Command::Fulfill { agreement, term } => value(
            tx,
            game_agreements::Fulfill {
                person,
                agreement,
                term,
            },
        ),
        Command::Vote { proposal, support } => value(
            tx,
            game_institutions::Vote {
                person,
                proposal,
                support,
            },
        ),
        Command::Decide { proposal, support } => value(
            tx,
            game_institutions::Decide {
                person,
                proposal,
                support,
            },
        ),
        Command::Build { project } => value(tx, game_projects::Build { person, project }),
        Command::GuardianOrder {
            guardian,
            destination,
        } => value(
            tx,
            game_guardians::TravelOrder {
                guardian,
                destination,
            },
        ),
        Command::Invoke { lives } => value(tx, game_invocation::Invoke { person, lives }),
        Command::Consent { step, accept } => value(tx, game_invocation::Consent { step, accept }),
        Command::Dismiss => value(tx, game_invocation::Dismiss),
        Command::Combat { battle, action } => value(
            tx,
            game_conflict::Order {
                person,
                battle,
                action,
            },
        ),
        Command::Depart { mission } => value(tx, game_missions::Depart { person, mission }),
        Command::Return { mission } => value(tx, game_missions::Return { person, mission }),
    }
}
pub fn observe(host: &Host) -> Result<PlayerView> {
    let person = game_lineage::current(host)?;
    Ok(PlayerView {
        time: host.now(),
        revision: host.revision(),
        person,
        body: person
            .map(|p| game_physiology::view(host, p))
            .transpose()?
            .flatten(),
        location: person.and_then(|p| {
            host.get::<game_transport::Positions>()
                .ok()?
                .0
                .get(&p)
                .cloned()
        }),
        action: person.and_then(|p| {
            host.get::<Actions>()
                .ok()?
                .0
                .get(&p)
                .map(|a| a.kind.clone())
        }),
        facts: person
            .map(|p| game_knowledge::view(host, p))
            .transpose()?
            .unwrap_or_default(),
        warning: host.get::<game_invocation::Invocation>()?.warning.clone(),
        control_until: host.get::<game_invocation::Invocation>()?.control_until,
        past_lives: host
            .get::<game_lineage::Lineage>()?
            .history
            .keys()
            .copied()
            .collect(),
        borrowed_memories: game_invocation::memories(host)?,
        lexicon: person
            .and_then(|p| {
                host.get::<game_communication::Communication>()
                    .ok()?
                    .lexicons
                    .get(&p)
            })
            .map_or_else(Default::default, |r| {
                r.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }),
        works: person
            .map(|p| game_works::view(host, p))
            .transpose()?
            .unwrap_or_default(),
        pastimes: person
            .and_then(|p| {
                host.get::<game_pastimes::Pastimes>()
                    .ok()?
                    .experiences
                    .get(&p)
            })
            .map(|rows| rows.iter().map(|(k, n)| (k.clone(), *n)).collect())
            .unwrap_or_default(),
        battles: person
            .map(|p| game_conflict::view(host, p))
            .transpose()?
            .unwrap_or_default(),
        communities: person
            .map(|p| game_communities::view(host, p))
            .transpose()?
            .unwrap_or_default(),
        neighbors: person
            .map(|p| {
                game_social_life::neighbors(host, p).map(|peers| {
                    peers
                        .into_iter()
                        .map(|other| game_api::Neighbor {
                            person: other,
                            name: host
                                .query(game_social_life::NameFor {
                                    observer: p,
                                    person: other,
                                    fallback: host
                                        .get::<game_social_life::SocialLife>()
                                        .unwrap()
                                        .people[&other]
                                        .name
                                        .clone(),
                                })
                                .unwrap(),
                            action: host
                                .get::<game_sim::Actions>()
                                .unwrap()
                                .0
                                .get(&other)
                                .map(|a| a.kind.clone()),
                        })
                        .collect()
                })
            })
            .transpose()?
            .unwrap_or_default(),
        events: host
            .notices()
            .filter(|n| person.is_some_and(|p| n.witnesses.contains(&p)))
            .map(|n| PlayerEvent {
                sequence: n.sequence,
                at: n.at,
                kind: n.kind.clone(),
                data: n.data.clone(),
            })
            .collect(),
    })
}
