use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<game_mechanics::SelectAid>(|tx, r| {
        let r = r.0;
        let area = game_transport::area(tx.get()?, r.person)?;
        let force = game_vitality::factor(game_vitality::ability(tx, r.person, "strength"));
        let openness = game_vitality::disposition(tx, r.person, "openness") / 20;
        let preference = tx.get::<game_relationships::Social>().map_or(0, |s| {
            i64::from(s.opinion(r.person, "mechanics") - s.opinion(r.person, "manual-work"))
        });
        let mut best = (0, None);
        for (id, d) in &tx.get::<Machines>()?.devices {
            if d.area != area || busy(tx, *id) {
                continue;
            }
            for outlet in 0..d.parts.len() {
                if d.parts.iter().any(|p| p.input == Some(outlet)) {
                    continue;
                }
                let Some(m) = known(tx, r.person, *id, outlet) else {
                    continue;
                };
                if u128::from(force) * u128::from(m.gain) / 1000 < u128::from(r.task.force) {
                    continue;
                }
                let benefit = if force < r.task.force { 100 } else { 0 }
                    + (m.travel.min(10000) as i64 - 1000) / 100;
                let score = benefit + preference + openness
                    - 10
                    - game_property::reluctance(tx, r.person, d.inventory) * 5
                    - d.broken.len() as i64 * 20;
                if score > best.0 {
                    best = (score, Some((*id, outlet)));
                }
            }
        }
        Ok(best.1)
    })
}
