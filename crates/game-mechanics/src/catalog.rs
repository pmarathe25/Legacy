use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Install(pub Config);
request!(Install, "mechanics.install", ());
pub fn load(store: &mut game_content::Store, objects: &game_objects::Objects) -> Result<Install> {
    Ok(Install(game_physics::load(store, objects)?))
}
use game_physics::needs;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Install>(|tx, r| {
        require(
            tx.get::<Machines>()?.devices.is_empty() && tx.get::<Machines>()?.work.is_empty(),
            "mechanical definitions are in use",
        )?;
        r.0.validate_objects(tx.get::<game_objects::Objects>()?)?;
        let state = tx.edit::<Machines>()?;
        state.config = Arc::new(r.0);
        state.enabled = true;
        state.validate()
    })?;
    b.request::<Assemble>(|tx, r| {
        let area = ready(tx, r.person)?;
        require(
            !tx.get::<Actions>()?.0.contains_key(&r.person),
            "assembler is busy",
        )?;
        require(
            game_cargo::locality(tx, r.inventory)? == area,
            "parts are elsewhere",
        )?;
        let design = game_physics::design_key(&r.parts);
        let needed = needs(&tx.get::<Machines>()?.config, &r.parts)?;
        let objects = tx.get::<game_objects::Objects>()?;
        let mass = needed.iter().try_fold(0u64, |sum, (g, n)| {
            sum.checked_add(objects.mass(*g, *n)?)
                .ok_or(Error::Overflow)
        })?;
        let inventory = tx.call(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: mass,
        })?;
        tx.call(game_property::ClaimInventory {
            claimant: r.person,
            inventory,
            recognized_by: [r.person].into(),
        })?;
        for (good, quantity) in needed {
            tx.call(game_property::Take {
                actor: r.person,
                unauthorized: true,
                transfer: game_cargo::Transfer {
                    from: r.inventory,
                    to: inventory,
                    good,
                    quantity,
                },
            })?;
        }
        let id = tx.allocate()?;
        tx.edit::<Machines>()?.devices.insert(
            id,
            Device {
                area,
                inventory,
                maker: r.person,
                parts: r.parts,
                broken: BTreeSet::new(),
            },
        );
        tx.edit::<Machines>()?
            .designs
            .entry(r.person)
            .or_default()
            .insert(design);
        let count = tx
            .get::<Machines>()?
            .attempts
            .get(&r.person)
            .copied()
            .unwrap_or(0);
        tx.edit::<Machines>()?
            .attempts
            .insert(r.person, count.saturating_add(1));
        tx.emit(
            Built {
                person: r.person,
                device: id,
                inventory,
            },
            vec![r.person],
        )?;
        Ok(id)
    })?;
    b.request::<Dismantle>(|tx, r| {
        let area = ready(tx, r.person)?;
        let device = tx
            .get::<Machines>()?
            .devices
            .get(&r.device)
            .cloned()
            .ok_or_else(|| Error::Missing("apparatus".into()))?;
        require(
            device.area == area && !busy(tx, r.device),
            "apparatus is distant or occupied",
        )?;
        let goods: Vec<_> = tx.get::<game_cargo::Cargo>()?.0[&device.inventory]
            .goods
            .iter()
            .map(|(g, n)| (*g, *n))
            .collect();
        for (good, quantity) in goods {
            tx.call(game_property::Take {
                actor: r.person,
                unauthorized: true,
                transfer: game_cargo::Transfer {
                    from: device.inventory,
                    to: r.inventory,
                    good,
                    quantity,
                },
            })?;
        }
        tx.edit::<Machines>()?.devices.remove(&r.device);
        Ok(())
    })
}
