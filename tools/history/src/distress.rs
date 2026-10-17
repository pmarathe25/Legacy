//! Bounded developer snapshots for prolonged hunger, separate from the game journal.
use super::*;
#[derive(Default)]
pub(super) struct Watch {
    episodes: BTreeMap<PersonId, (SimTime, SimTime, u32)>,
}
fn stock(h: &game_sim::Host, id: InventoryId) -> Result<Value> {
    let inventory = &h.get::<game_cargo::Cargo>()?.0[&id];
    let objects = h.get::<game_objects::Objects>()?;
    let mut goods: Vec<_> = inventory.goods.iter().filter(|(_, q)| **q > 0).collect();
    goods.sort_by_key(|(good, q)| {
        (
            std::cmp::Reverse(objects.mass(**good, **q).unwrap_or(0)),
            **good,
        )
    });
    let food: Vec<_> = inventory.goods.iter().filter_map(|(good, q)| {
        let item = objects.by_id(*good).ok()?;
        let food = item.food.as_ref()?;
        (*q >= food.portion).then(|| json!({"good":good,"name":item.name,"quantity":q,"portion":food.portion,"relief":food.relief}))
    }).collect();
    Ok(
        json!({"inventory":id,"custody":inventory.custody,"capacity":inventory.capacity,"free_mass":inventory.capacity.saturating_sub(inventory.used()?),
        "food":food,"bulky_goods":goods.into_iter().take(8).map(|(g,q)| json!({"good":g,"name":objects.by_id(*g).unwrap().name,"quantity":q})).collect::<Vec<_>>()}),
    )
}
fn intention(h: &game_sim::Host, person: PersonId) -> Result<Value> {
    let mind = h.get::<game_characters::Minds>()?.people.get(&person);
    Ok(
        json!({"person":person,"body":h.get::<game_physiology::Bodies>()?.body(&person),
        "position":h.get::<game_transport::Positions>()?.0.get(&person),"mind":mind,
        "action":h.get::<game_sim::Actions>()?.0.get(&person),
        "carried":h.get::<game_cargo::Cargo>()?.carried(person).map(|id|stock(h,id)).collect::<Result<Vec<_>>>()?}),
    )
}
impl Watch {
    pub(super) fn collect(
        &mut self,
        h: &game_sim::Host,
        epoch: u64,
        out: &mut impl Write,
    ) -> Result<()> {
        let urgent = h.get::<game_characters::Minds>()?.rules.urgent_hunger;
        let bodies = h.get::<game_physiology::Bodies>()?;
        self.episodes.retain(|p, _| {
            bodies
                .people
                .get(p)
                .is_some_and(|b| b.alive && b.hunger >= urgent)
        });
        for (person, body) in bodies.people.iter().filter(|(_, b)| {
            b.alive && b.kind == game_physiology::Kind::Mortal && b.hunger >= urgent
        }) {
            let episode = self
                .episodes
                .entry(*person)
                .or_insert((h.now(), SimTime(0), 0));
            if episode.2 >= 8
                || (episode.2 > 0 && h.now().0.saturating_sub(episode.1.0) < 2 * DAY.0)
            {
                continue;
            }
            episode.1 = h.now();
            episode.2 += 1;
            let area = game_transport::area(h.get()?, *person).ok();
            let local:Vec<_>=area.into_iter().flat_map(|a|h.get::<game_ecology::Ecology>().unwrap().in_area(a))
                .filter_map(|id| {
                    let patch=&h.get::<game_ecology::Ecology>().ok()?.patches[&id];
                    let item=h.get::<game_objects::Objects>().ok()?.by_id(patch.good).ok()?;
                    item.food.as_ref()?;
                    Some(json!({"patch":id,"good":patch.good,"name":item.name,"stock":patch.stock,"available":h.get::<game_ecology::Ecology>().unwrap().available(id).ok()}))
                }).take(12).collect();
            let households = h.get::<game_households::Households>()?;
            let caregiver = households.dependents.get(person).copied();
            let traits = h.get::<game_traits::Traits>()?;
            let nutrition = h.get::<game_nutrition::Nutrition>()?;
            let mut row = intention(h, *person)?;
            row["day"] = json!(h.now().0.saturating_sub(epoch) / DAY.0);
            row["age"] = json!(h.now().0.saturating_sub(body.born.0) as f64 / (365 * DAY.0) as f64);
            row["episode_since"] = json!(episode.0);
            row["sample"] = json!(episode.2);
            row["actual_local_food"] = json!(local);
            row["intelligence"] = json!(traits.ability(*person, "intelligence"));
            row["foresight"] = json!(traits.ability(*person, "foresight"));
            row["nutrition"] = json!(
                nutrition
                    .people
                    .get(person)
                    .map(|d| d.status(&nutrition.needs, h.now()))
            );
            row["caregiver"] = caregiver
                .map(|p| intention(h, p))
                .transpose()?
                .unwrap_or(Value::Null);
            row["dependents"]=json!(households.dependents_of(*person).map(|child|json!({"child":child,"last_seen":households.nursery.get(&child),"position":h.get::<game_transport::Positions>().unwrap().0.get(&child)})).collect::<Vec<_>>());
            serde_json::to_writer(&mut *out, &row)?;
            writeln!(out)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distress_snapshots_are_bounded_and_leave_the_world_unchanged() {
        let mut sim =
            game_assembly::Simulation::uncontrolled(3, 1, &mut game_content::Store::standard())
                .unwrap();
        let epoch = sim.now().0;
        let area = *sim
            .debug_host()
            .get::<game_land::Land>()
            .unwrap()
            .0
            .keys()
            .max_by_key(|a| {
                sim.debug_host()
                    .get::<game_environment::Regions>()
                    .unwrap()
                    .0
                    .get(a)
                    .unwrap_or(sim.debug_host().get::<game_environment::Climate>().unwrap())
                    .at(sim.now())
                    .unwrap()
                    .warmth
            })
            .unwrap();
        let person = sim
            .debug_host_mut()
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(epoch.saturating_sub(25 * 365 * DAY.0)),
            })
            .unwrap();
        sim.debug_host_mut()
            .dispatch(game_transport::Place { person, area })
            .unwrap();
        sim.debug_host_mut()
            .dispatch(game_heredity::Founder { person })
            .unwrap();
        // Isolate urgent hunger from random inherited maintenance demand.
        let mut profile = sim
            .debug_host()
            .get::<game_traits::Traits>()
            .unwrap()
            .people
            .get(&person)
            .cloned()
            .unwrap_or_default();
        for key in [
            "metabolism",
            "size",
            "strength",
            "constitution",
            "recovery",
            "robustness",
        ] {
            profile.insert(key.into(), 1000);
        }
        profile.insert("resilience".into(), 16000);
        sim.debug_host_mut()
            .dispatch(game_traits::Set { person, profile })
            .unwrap();
        let urgent = sim
            .debug_host()
            .get::<game_characters::Minds>()
            .unwrap()
            .rules
            .urgent_hunger;
        for day in 1..=10 {
            let target = SimTime(epoch + day * DAY.0);
            while sim.advance(target, 128).unwrap().stop == game_sim::Stop::Budget {}
            let body = &sim
                .debug_host()
                .get::<game_physiology::Bodies>()
                .unwrap()
                .people[&person];
            assert!(
                body.alive,
                "diagnostic subject died before reaching urgent hunger"
            );
            if body.hunger >= urgent {
                break;
            }
        }
        let before = sim.save().unwrap();
        let mut watch = Watch::default();
        let mut output = vec![];
        watch.collect(sim.debug_host(), epoch, &mut output).unwrap();
        let rows: Vec<Value> = String::from_utf8(output.clone())
            .unwrap()
            .lines()
            .map(|r| serde_json::from_str(r).unwrap())
            .collect();
        assert!(rows.iter().any(|r| r["person"] == json!(person)));
        let bytes = output.len();
        watch.collect(sim.debug_host(), epoch, &mut output).unwrap();
        assert_eq!(bytes, output.len());
        assert_eq!(sim.save().unwrap(), before);
    }
}
