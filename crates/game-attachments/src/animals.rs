use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Hunt {
    pub person: PersonId,
    pub animal: u64,
    pub inventory: InventoryId,
}
impl Request for Hunt {
    const KEY: &'static str = "attachments.hunt";
    type Response = SimTime;
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Hunt>(|tx, r| {
        require(
            game_vitality::can_walk(tx, r.person) && game_vitality::can_manipulate(tx, r.person),
            "hunting exceeds motor capacity",
        )?;
        tx.call(game_choice::Undertake {
            person: r.person,
            kind: "hunt".into(),
            duration: SimDuration(30 * MINUTE.0),
            request: Pending::new(game_fauna::Catch {
                person: r.person,
                animal: r.animal,
                inventory: r.inventory,
            })?,
        })
    })?;
    game_choice::provider(b, "sentiment.animals", |tx, e| {
        if !game_choice::collecting(tx, e.person) || !game_vitality::can_manipulate(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let animals: Vec<_> = tx
            .get::<game_fauna::Fauna>()?
            .animals
            .iter()
            .filter(|(_, a)| a.area == area)
            .map(|(id, a)| (*id, a.hunger.min(100), a.caregiver))
            .take(16)
            .collect();
        let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        for (animal, need, caregiver) in animals {
            let attachment = i64::from(value(tx, p, &Target::Animal(animal)));
            let concern = attachment / 5
                + game_vitality::disposition(tx, p, "empathy") / 20
                + need as i64 / 2;
            for inventory in &packs {
                let food = tx.get::<game_cargo::Cargo>()?.0[inventory]
                    .goods
                    .iter()
                    .find_map(|(g, q)| {
                        tx.get::<game_objects::Objects>()
                            .unwrap()
                            .by_id(*g)
                            .ok()?
                            .food
                            .as_ref()
                            .filter(|f| *q >= f.portion)
                            .map(|f| (*g, f.portion))
                    });
                if let Some((good, quantity)) = food {
                    game_choice::offer(
                        tx,
                        p,
                        format!("animal:feed:{animal}:{}", inventory.0),
                        game_choice::motive(0, 0, concern, 10, 0),
                        game_fauna::Feed {
                            person: p,
                            animal,
                            inventory: *inventory,
                            good,
                            quantity,
                        },
                    )?;
                }
            }
            if caregiver != Some(p) {
                game_choice::offer(
                    tx,
                    p,
                    format!("animal:tame:{animal}"),
                    game_choice::motive(0, 0, attachment / 5, 10, 0),
                    game_fauna::Tame { person: p, animal },
                )?;
            }
            let hunger = tx.get::<game_physiology::Bodies>()?.people[&p]
                .hunger
                .min(1000) as i64;
            if let Some(inventory) = packs.first().filter(|_| game_vitality::can_walk(tx, p)) {
                game_choice::offer(
                    tx,
                    p,
                    format!("animal:hunt:{animal}"),
                    game_choice::motive(hunger, 0, -attachment, 20, 0),
                    Hunt {
                        person: p,
                        animal,
                        inventory: *inventory,
                    },
                )?;
            }
        }
        Ok(())
    })
}
