use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_fauna::Tended>("sentiment.animal-care", |tx, e| {
        tx.call(Experience {
            person: e.person,
            target: Target::Animal(e.animal),
            intensity: 3,
            reason: "fed and tended an animal".into(),
        })?;
        Ok(())
    })?;
    b.subscribe::<game_fauna::Died>("sentiment.animal-loss", |tx, e| {
        let observers = if tx.registered::<game_property::Observers>() {
            tx.call(game_property::Observers { area: e.area })?
        } else {
            vec![]
        };
        let target = Target::Animal(e.animal);
        for person in observers {
            let Some(memory) = tx
                .get::<Sentiment>()?
                .people
                .get(&person)
                .and_then(|r| r.get(&target.key()))
                .cloned()
            else {
                continue;
            };
            if tx.registered::<game_vitality::Experience>() {
                tx.call(game_vitality::Experience {
                    person,
                    stress: memory.value / 10,
                    satisfaction: -memory.value / 10,
                })?;
            }
            let reason = "witnessed an animal's death".to_string();
            tx.edit::<Sentiment>()?
                .people
                .get_mut(&person)
                .unwrap()
                .get_mut(&target.key())
                .unwrap()
                .reason = reason.clone();
            tx.emit(
                Changed {
                    person,
                    target: target.clone(),
                    value: memory.value,
                    reason,
                },
                vec![person],
            )?;
        }
        Ok(())
    })?;
    Ok(())
}
