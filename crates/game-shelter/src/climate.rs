use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Extinguish {
    pub room: AreaId,
}
impl Request for Extinguish {
    const KEY: &'static str = "shelter.extinguish";
    type Response = ();
}
pub(super) fn refresh(tx: &mut Transaction<'_>, area: AreaId) -> Result<()> {
    let room = tx
        .get::<Shelters>()?
        .rooms
        .get(&area)
        .cloned()
        .ok_or_else(|| Error::Missing("shelter".into()))?;
    let mut climate = game_environment::climate(tx, room.outside)?.clone();
    let heat = if room.burning_until > tx.now() {
        room.heat
    } else {
        0
    };
    for s in &mut climate.seasons {
        s.warmth = s.warmth.saturating_add(heat);
        s.rain = 0;
    }
    tx.call(game_environment::ConfigureArea { area, climate })
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Extinguish>(|tx, r| {
        let at = tx.now();
        if let Some(room) = tx.edit::<Shelters>()?.rooms.get_mut(&r.room)
            && room.burning_until <= at
        {
            room.fire = None;
            refresh(tx, r.room)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_environment::Changed>("shelter.climate", |tx, e| {
        let rooms: Vec<_> = tx
            .get::<Shelters>()?
            .rooms
            .iter()
            .filter(|(_, r)| r.outside == e.area)
            .map(|(id, _)| *id)
            .collect();
        for room in rooms {
            refresh(tx, room)?;
        }
        Ok(())
    })?;
    Ok(())
}
