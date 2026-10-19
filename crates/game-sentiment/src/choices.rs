use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Recall {
    pub person: PersonId,
    pub target: Target,
}
impl Request for Recall {
    const KEY: &'static str = "sentiment.recall";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Gift {
    pub person: PersonId,
    pub recipient: PersonId,
    pub item: u64,
}
impl Request for Gift {
    const KEY: &'static str = "sentiment.gift";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TellMemory {
    pub person: PersonId,
    pub listener: PersonId,
    pub target: Target,
}
impl Request for TellMemory {
    const KEY: &'static str = "sentiment.tell-memory";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Locate {
    pub person: PersonId,
    pub item: u64,
    pub inventory: InventoryId,
}
impl Request for Locate {
    const KEY: &'static str = "sentiment.locate";
    type Response = bool;
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Locate>(|tx, r| {
        let present = tx.call(game_items::Inspect {
            person: r.person,
            item: r.item,
            inventory: r.inventory,
        })?;
        let row = tx
            .edit::<Sentiment>()?
            .locations
            .entry(r.person)
            .or_default();
        if present {
            row.insert(r.item, r.inventory);
        } else if row.get(&r.item) == Some(&r.inventory) {
            row.remove(&r.item);
        }
        Ok(present)
    })?;
    b.request::<Recall>(|tx, r| {
        let memory = tx
            .get::<Sentiment>()?
            .people
            .get(&r.person)
            .and_then(|m| m.get(&r.target.key()))
            .ok_or_else(|| Error::Missing("personal memory".into()))?;
        let mood = memory.value / 100;
        if tx.registered::<game_vitality::Experience>() {
            tx.call(game_vitality::Experience {
                person: r.person,
                stress: 0,
                satisfaction: mood,
            })?;
        }
        let at = tx.now();
        tx.edit::<Sentiment>()?.recalled.insert(r.person, at);
        Ok(())
    })?;
    b.request::<Gift>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.person)?
                == game_transport::area(tx.get()?, r.recipient)?,
            "gift needs contact",
        )?;
        let item = tx
            .get::<game_items::Items>()?
            .objects
            .get(&r.item)
            .ok_or_else(|| Error::Missing("item".into()))?;
        require(
            item.inventory.is_some_and(|i| {
                tx.get::<game_cargo::Cargo>().unwrap().0[&i].custody
                    == game_cargo::Custody::Carried(r.person)
            }),
            "gift is not carried",
        )?;
        let regard = tx
            .get::<game_relationships::Social>()?
            .regard(r.recipient, r.person);
        if regard + game_vitality::disposition(tx, r.recipient, "openness") / 20 < 0 {
            return Ok(false);
        }
        let destination = tx
            .get::<game_cargo::Cargo>()?
            .carried(r.recipient)
            .next()
            .ok_or_else(|| Error::Missing("recipient pack".into()))?;
        tx.call(game_items::Move {
            person: r.person,
            item: r.item,
            destination,
        })?;
        tx.call(game_relationships::Converse {
            observer: r.recipient,
            other: r.person,
            warmth: 3,
        })?;
        Ok(true)
    })?;
    b.request::<TellMemory>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.person)?
                == game_transport::area(tx.get()?, r.listener)?,
            "remembrance needs contact",
        )?;
        let memory = tx
            .get::<Sentiment>()?
            .people
            .get(&r.person)
            .and_then(|m| m.get(&r.target.key()))
            .cloned()
            .ok_or_else(|| Error::Missing("personal memory".into()))?;
        if game_communication::active(tx)
            && !game_communication::understood(
                tx,
                r.person,
                r.listener,
                &format!("memory:{}", r.target.key()),
                true,
            )?
        {
            return Ok(false);
        }
        let regard = tx
            .get::<game_relationships::Social>()?
            .regard(r.listener, r.person);
        let intensity =
            (regard / 10 + game_vitality::disposition(tx, r.listener, "contemplation") / 100 - 5)
                .clamp(-20, 20) as i32;
        if intensity == 0 {
            return Ok(false);
        }
        let at = tx.now();
        let row = tx
            .edit::<Sentiment>()?
            .people
            .entry(r.listener)
            .or_default();
        let entry = row.entry(r.target.key()).or_insert_with(|| Memory {
            target: r.target,
            value: 0,
            at,
            reason: format!("heard a memory from {}: {}", r.person.0, memory.reason),
            experiences: 0,
        });
        entry.value = (entry.value + intensity).clamp(-1000, 1000);
        entry.experiences = entry.experiences.saturating_add(1);
        Ok(true)
    })?;
    Ok(())
}
