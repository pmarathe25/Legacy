//! Claims have consequences only when particular witnesses notice and choose a response.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grievances {
    pub enabled: bool,
    pub noticed: Table<PersonId, Table<PersonId, ()>>,
    pub forgiven: u64,
}
impl Resource for Grievances {
    const KEY: &'static str = "grievances.noticed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "grievances.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Response {
    pub observer: PersonId,
    pub actor: PersonId,
    pub inventory: InventoryId,
    pub forgiven: bool,
}
impl Event for Response {
    const KEY: &'static str = "grievances.response";
}
fn notice(tx: &mut Transaction<'_>, e: &game_property::Interfered) -> Result<()> {
    if !tx.get::<Grievances>()?.enabled {
        return Ok(());
    }
    let claims = tx
        .get::<game_property::Property>()?
        .claims
        .get(&e.inventory)
        .cloned()
        .unwrap_or_default();
    for witness in &e.witnesses {
        if *witness == e.actor
            || !claims
                .iter()
                .any(|c| c.claimant == *witness || c.recognized_by.contains(witness))
        {
            continue;
        }
        let empathy = game_vitality::disposition(tx, *witness, "empathy");
        // Purpose and locally witnessed conduct are evidence; hidden hunger is not.
        let necessary = ["eating", "feeding", "treatment"].contains(&e.purpose.as_str());
        let forgiven = !tx.get::<game_characters::Minds>()?.running && necessary && empathy >= 650;
        if forgiven {
            tx.edit::<Grievances>()?.forgiven += 1;
        } else {
            tx.call(game_relationships::Interpret {
                observer: *witness,
                actor: e.actor,
                shared: false,
                harmed: true,
                credible: true,
            })?;
            tx.edit::<Grievances>()?
                .noticed
                .entry(*witness)
                .or_default()
                .insert(e.actor, ());
        }
        tx.emit(
            Response {
                observer: *witness,
                actor: e.actor,
                inventory: e.inventory,
                forgiven,
            },
            vec![*witness, e.actor],
        )?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub enum Resolution {
    Forgive,
    Warn,
    Negotiate,
    Report(PersonId),
    Avoid(AreaId),
    Retaliate,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Resolve {
    pub person: PersonId,
    pub other: PersonId,
    pub resolution: Resolution,
}
impl Request for Resolve {
    const KEY: &'static str = "grievances.resolve";
    type Response = ();
}
fn respond(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if !game_vitality::can_converse(tx, person) {
        return Ok(());
    }
    if !tx.get::<Grievances>()?.enabled
        || Some(person) == tx.get::<game_lineage::Lineage>()?.current
    {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(());
    };
    let known: Vec<_> = tx
        .get::<Grievances>()?
        .noticed
        .get(&person)
        .into_iter()
        .flat_map(|r| r.keys())
        .copied()
        .take(16)
        .collect();
    for other in known {
        let r = tx
            .get::<game_relationships::Social>()?
            .relationships
            .get(&person)
            .and_then(|r| r.get(&other))
            .cloned()
            .unwrap_or_default();
        let patience = game_vitality::disposition(tx, person, "patience");
        let empathy = game_vitality::disposition(tx, person, "empathy");
        let aggression = game_vitality::disposition(tx, person, "aggression");
        let ethics = game_violence::ethics(tx, person);
        let local = game_transport::area(tx.get()?, other).ok() == Some(area);
        let mut options = vec![(
            "forgive",
            Resolution::Forgive,
            (patience + empathy) / 20 + i64::from(r.affection) - i64::from(r.grievance) * 2,
        )];
        if local {
            options.extend([
                (
                    "warn",
                    Resolution::Warn,
                    aggression / 20 + i64::from(r.grievance) * 2,
                ),
                (
                    "negotiate",
                    Resolution::Negotiate,
                    patience / 10 + i64::from(r.trust),
                ),
                (
                    "retaliate",
                    Resolution::Retaliate,
                    ethics + aggression / 5 + i64::from(r.grievance) * 3
                        - patience / 10
                        - i64::from(r.fear) * 3,
                ),
            ]);
        }
        if let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person)
            && let Some(roads) =
                game_knowledge::known::<game_land::Area>(facts, &game_knowledge::roads_key(area))
            && let Some(destination) = roads.links.keys().next().copied()
        {
            options.push((
                "avoid",
                Resolution::Avoid(destination),
                i64::from(r.fear) * 2 + patience / 20,
            ));
        }
        let listener = tx
            .get::<game_relationships::Social>()?
            .relationships
            .get(&person)
            .into_iter()
            .flat_map(|r| r.iter())
            .filter(|(p, r)| {
                **p != other
                    && r.trust > 0
                    && game_transport::area(tx.get().unwrap(), **p).ok() == Some(area)
            })
            .max_by_key(|(p, r)| (r.trust, std::cmp::Reverse(**p)))
            .map(|(p, _)| *p);
        if let Some(listener) = listener {
            options.push((
                "report",
                Resolution::Report(listener),
                i64::from(r.grievance) + patience / 10,
            ));
        }
        for (name, resolution, value) in options {
            let request = Resolve {
                person,
                other,
                resolution,
            };
            if !game_choice::offer(
                tx,
                person,
                format!("dispute:{name}:{}", other.0),
                game_choice::motive(0, 0, value, 5, 0),
                game_choice::Undertake {
                    person,
                    kind: "dispute".into(),
                    duration: SimDuration(5 * MINUTE.0),
                    request: Pending::immediate(request.clone()),
                },
            )? && value > 150
            {
                let _ = tx.attempt(request);
                return Ok(());
            }
        }
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("grievances", 1)?;
    b.resource::<Grievances>()?;
    b.request::<Resolve>(|tx, r| {
        require(
            tx.get::<Grievances>()?
                .noticed
                .get(&r.person)
                .is_some_and(|known| known.contains_key(&r.other)),
            "no remembered grievance",
        )?;
        match r.resolution {
            Resolution::Forgive => {
                tx.call(game_relationships::Reconcile {
                    observer: r.person,
                    other: r.other,
                    amount: 100,
                })?;
                tx.edit::<Grievances>()?.forgiven += 1;
            }
            Resolution::Avoid(destination) => {
                tx.call(game_transport::Move {
                    person: r.person,
                    destination,
                })?;
            }
            Resolution::Retaliate => {
                tx.call(game_pressure::DemandSubmission {
                    leader: r.person,
                    subject: r.other,
                })?;
            }
            resolution => {
                let (listener, concept, stance, approach) = match resolution {
                    Resolution::Report(listener) => (
                        listener,
                        format!("claim:{}", r.other.0),
                        -80,
                        game_discourse::Approach::Explain,
                    ),
                    Resolution::Negotiate => (
                        r.other,
                        "share-necessities".into(),
                        50,
                        game_discourse::Approach::Persuade,
                    ),
                    _ => (
                        r.other,
                        "respect-claims".into(),
                        80,
                        game_discourse::Approach::Explain,
                    ),
                };
                tx.call(game_discourse::Advocate {
                    speaker: r.person,
                    listener,
                    concept,
                    stance,
                    approach,
                })?;
            }
        }
        tx.edit::<Grievances>()?
            .noticed
            .get_mut(&r.person)
            .unwrap()
            .remove(&r.other);
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Grievances>()?.enabled = true;
        Ok(())
    })?;
    b.subscribe::<game_property::Interfered>("grievances.witness", notice)?;
    b.subscribe::<game_agriculture::Encroached>("grievances.land", |tx, e| {
        if tx.get::<Grievances>()?.enabled && e.witnesses.contains(&e.steward) {
            let shared = e.purpose == "sowing";
            tx.call(game_relationships::Interpret {
                observer: e.steward,
                actor: e.person,
                shared,
                harmed: !shared,
                credible: true,
            })?;
            if !shared {
                tx.edit::<Grievances>()?
                    .noticed
                    .entry(e.steward)
                    .or_default()
                    .insert(e.person, ());
            }
        }
        Ok(())
    })?;
    game_choice::provider(b, "disputes.responses", |tx, e| respond(tx, e.person))?;
    Ok(())
}
