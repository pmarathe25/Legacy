use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trip {
    at: SimTime,
    from: AreaId,
    to: AreaId,
    context: BTreeMap<String, i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meal {
    health: u64,
    fatigue: u64,
    mood: i32,
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_exposure::Changed>("inquiry.felt-exposure", |tx, e| {
        tx.call(Witness {
            observer: e.person,
            observation: Observation {
                subject: "bodily-exposure".into(),
                context: BTreeMap::from([(
                    "felt-temperature".into(),
                    i64::from(e.equivalent / 5 * 5),
                )]),
                outcome: "pain".into(),
                value: e.damage.min(i64::MAX as u64) as i64,
                trial: 0,
            },
        })
    })?;
    b.subscribe::<game_transport::Departed>("inquiry.travel-start", |tx, e| {
        if let Some(game_transport::Location::Transit { from, to, .. }) =
            tx.get::<game_transport::Positions>()?.0.get(&e.person)
        {
            let mut context = weather(tx, e.person);
            let mass = tx
                .get::<game_cargo::Cargo>()?
                .carried(e.person)
                .map(|i| tx.get::<game_cargo::Cargo>().unwrap().0[&i].mass)
                .sum::<u64>();
            context.insert("load".into(), mass.min(100000) as i64 / 100 * 100);
            let trip = Trip {
                at: tx.now(),
                from: *from,
                to: *to,
                context,
            };
            tx.edit::<Observations>()?.trips.insert(e.person, trip);
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("inquiry.travel-outcome", |tx, e| {
        if let Some(trip) = tx.edit::<Observations>()?.trips.remove(&e.person) {
            let trip = std::sync::Arc::unwrap_or_clone(trip);
            let value = tx.now().0.saturating_sub(trip.at.0) / MINUTE.0;
            tx.call(Witness {
                observer: e.person,
                observation: Observation {
                    subject: format!("road:{}:{}", trip.from.0, trip.to.0),
                    context: trip.context,
                    outcome: "travel-time".into(),
                    value: value.min(i64::MAX as u64) as i64,
                    trial: 0,
                },
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Eating>("inquiry.before-tasting", |tx, r| {
        let body = &tx.get::<game_physiology::Bodies>()?.people[&r.person];
        let meal = Meal {
            health: body.health,
            fatigue: body.fatigue,
            mood: game_vitality::condition(tx, r.person).map_or(0, |(_, m)| m),
        };
        tx.edit::<Observations>()?.meals.insert(r.person, meal);
        Ok(())
    })?;
    b.subscribe::<game_physiology::Fed>("inquiry.experienced-effects", |tx, e| {
        if let Some(meal) = tx.edit::<Observations>()?.meals.remove(&e.patient) {
            let body = tx.get::<game_physiology::Bodies>()?.people[&e.patient];
            let mood = game_vitality::condition(tx, e.patient).map_or(0, |(_, m)| m);
            for (outcome, value) in [
                ("health", body.health as i64 - meal.health as i64),
                ("alertness", meal.fatigue as i64 - body.fatigue as i64),
                ("mood", i64::from(mood - meal.mood)),
            ] {
                tx.call(Witness {
                    observer: e.patient,
                    observation: Observation {
                        subject: format!("good:{}", e.good.0),
                        context: BTreeMap::from([(
                            "dose".into(),
                            e.quantity.min(i64::MAX as u64) as i64,
                        )]),
                        outcome: outcome.into(),
                        value,
                        trial: 0,
                    },
                })?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_agriculture::Harvested>("inquiry.harvest", |tx, e| {
        let context = weather(tx, e.person);
        tx.call(Witness {
            observer: e.person,
            observation: Observation {
                subject: format!("crop:{}", e.good.0),
                context,
                outcome: "harvest".into(),
                value: e.quantity.min(i64::MAX as u64) as i64,
                trial: 0,
            },
        })
    })?;
    b.before::<game_workshop::Craft>("inquiry.craft-start", |tx, r| {
        let skill = &tx
            .get::<game_workshop::Workshop>()?
            .recipes
            .get(&r.recipe)
            .ok_or_else(|| Error::Missing("recipe".into()))?
            .skill;
        let practice = tx
            .get::<game_skills::Skills>()?
            .levels
            .get(&r.person)
            .and_then(|r| r.get(skill.as_str()))
            .copied()
            .unwrap_or(0);
        let at = tx.now();
        tx.edit::<Observations>()?
            .crafts
            .insert(r.person, (at, practice));
        Ok(())
    })?;
    b.subscribe::<game_workshop::Produced>("inquiry.work-outcome", |tx, e| {
        if let Some((at, practice)) = tx.edit::<Observations>()?.crafts.remove(&e.person) {
            let value = tx.now().0.saturating_sub(at.0) / MINUTE.0;
            tx.call(Witness {
                observer: e.person,
                observation: Observation {
                    subject: format!("craft:{}", e.good.0),
                    context: BTreeMap::from([(
                        "practice".into(),
                        practice.min(i64::MAX as u64) as i64 / 10 * 10,
                    )]),
                    outcome: "work-time".into(),
                    value: value.min(i64::MAX as u64) as i64,
                    trial: 0,
                },
            })?;
        }
        Ok(())
    })
}
