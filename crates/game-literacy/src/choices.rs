use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    game_choice::deliberative_provider(b, "literacy.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        let known = first_script(tx, p);
        let cargo = tx.get::<game_cargo::Cargo>()?;
        let materials: Vec<_> = cargo
            .carried(p)
            .filter(|id| game_inscription::supplies(tx, p, *id).is_some())
            .collect();
        for inventory in materials {
            let curiosity = game_vitality::disposition(tx, p, "openness") / 10
                + game_vitality::disposition(tx, p, "creativity") / 10;
            if let Some(script) = known {
                let keys: Vec<_> = tx
                    .get::<game_knowledge::Knowledge>()?
                    .0
                    .get(&p)
                    .into_iter()
                    .flat_map(|r| game_knowledge::with_prefix(r, "theory:"))
                    .map(|(k, _)| k.to_string())
                    .take(4)
                    .collect();
                for key in keys {
                    if tx.get::<Literacy>()?.documents.values().any(|d| {
                        d.author == p
                            && d.key == key
                            && tx
                                .get::<game_knowledge::Knowledge>()
                                .ok()
                                .and_then(|k| k.0.get(&p)?.get(key.as_str()))
                                .is_some_and(|fact| {
                                    game_knowledge::same_value(fact.value, &d.fact.value)
                                })
                    }) {
                        continue;
                    }
                    game_choice::offer(
                        tx,
                        p,
                        format!(
                            "literacy:write:{}:{}",
                            inventory.0,
                            game_choice::identity(&key)
                        ),
                        game_choice::motive(0, 0, curiosity, 10, 0),
                        Inscribe {
                            person: p,
                            inventory,
                            key,
                            script,
                        },
                    )?;
                }
            } else {
                game_choice::offer(
                    tx,
                    p,
                    format!("literacy:develop:{}", inventory.0),
                    game_choice::motive(0, 0, curiosity, 30, 0),
                    Develop {
                        person: p,
                        inventory,
                    },
                )?;
            }
        }
        let documents: Vec<_> = tx
            .get::<Literacy>()?
            .seen
            .get(&p)
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .take(8)
            .collect();
        for document in documents {
            let d = &tx.get::<Literacy>()?.documents[&document];
            if d.author == p || !exposed(tx, p, d) {
                continue;
            }
            if tx
                .get::<game_knowledge::Knowledge>()?
                .0
                .get(&p)
                .is_some_and(|r| r.contains_key(d.key.as_str()))
            {
                continue;
            }
            game_choice::offer(
                tx,
                p,
                format!("literacy:read:{document}"),
                game_choice::motive(
                    0,
                    0,
                    game_vitality::disposition(tx, p, "contemplation") / 10,
                    5,
                    0,
                ),
                game_choice::Undertake {
                    person: p,
                    kind: "read-note".into(),
                    duration: SimDuration(10 * MINUTE.0),
                    request: Pending::immediate(Read {
                        person: p,
                        document,
                    }),
                },
            )?;
        }
        Ok(())
    })?;
    b.subscribe::<game_social_life::Encounter>("literacy.conversation", |tx, e| {
        for (person, listener) in [(e.first, e.second), (e.second, e.first)] {
            let interest = game_vitality::disposition(tx, person, "altruism")
                + game_vitality::disposition(tx, person, "sociability");
            if interest < 900 {
                continue;
            }
            let script = first_script(tx, person);
            if let Some(script) = script {
                let _ = tx.attempt(Teach {
                    teacher: person,
                    student: listener,
                    script,
                });
            }
            let document = tx
                .get::<Literacy>()?
                .seen
                .get(&person)
                .into_iter()
                .flat_map(|r| r.keys())
                .copied()
                .find(|id| exposed(tx, person, &tx.get::<Literacy>().unwrap().documents[id]));
            if let Some(document) = document {
                let _ = tx.attempt(Show {
                    person,
                    listener,
                    document,
                });
            }
        }
        Ok(())
    })
}
