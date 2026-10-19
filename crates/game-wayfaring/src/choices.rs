use super::*;
use game_choice::{Proposal, motive};
#[derive(Clone, Serialize, Deserialize)]
struct Go {
    person: PersonId,
    target: AreaId,
}
impl Request for Go {
    const KEY: &'static str = "wayfaring.go";
    type Response = ();
}
fn position(tx: &Transaction<'_>, p: PersonId) -> Result<game_transport::Location> {
    let position = tx
        .get::<game_transport::Positions>()?
        .0
        .get(&p)
        .cloned()
        .ok_or_else(|| Error::Missing("position".into()))?;
    Ok(match position {
        game_transport::Location::Transit {
            from,
            to,
            departed,
            arrives,
            ..
        } => game_transport::Location::Road {
            from,
            to,
            elapsed: SimDuration(tx.now().0.saturating_sub(departed.0)),
            duration: SimDuration(arrives.0.saturating_sub(departed.0)),
        },
        other => other,
    })
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Go>(|tx, r| {
        let location = position(tx, r.person)?;
        let area = match location {
            game_transport::Location::At(a) => a,
            game_transport::Location::Road { from, .. } => from,
            _ => unreachable!(),
        };
        if let game_transport::Location::At(here) = location {
            let now = tx.now();
            tx.edit::<Wayfaring>()?
                .visits
                .entry(r.person)
                .or_default()
                .entry(here)
                .or_insert_with(|| now);
        }
        let facts = &tx.get::<game_knowledge::Knowledge>()?.0[&r.person];
        let (next, _) = game_food_actions::next_step(tx, r.person, facts, &location, r.target)?;
        let next = next.ok_or_else(|| Error::Rejected("already at destination".into()))?;
        let mut trip = tx
            .get::<Wayfaring>()?
            .trips
            .get(&r.person)
            .cloned()
            .unwrap_or(Trip {
                home: area,
                destination: r.target,
                returning: false,
                leave_at: None,
            });
        trip.returning = r.target == trip.home;
        if !trip.returning {
            trip.destination = r.target;
        }
        tx.edit::<Wayfaring>()?.trips.insert(r.person, trip);
        tx.call(game_transport::Move {
            person: r.person,
            destination: next,
        })?;
        Ok(())
    })
}
pub(super) fn options(tx: &Transaction<'_>, p: PersonId) -> Result<Vec<Proposal>> {
    let mut offers = vec![];
    if !tx.get::<Wayfaring>()?.enabled
        || Some(p) == tx.get::<game_lineage::Lineage>()?.current
        || !game_vitality::can_walk(tx, p)
    {
        return Ok(offers);
    }
    let location = position(tx, p)?;
    let area = if let game_transport::Location::At(a) = location {
        Some(a)
    } else {
        None
    };
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&p) else {
        return Ok(offers);
    };
    let state = tx.get::<Wayfaring>()?;
    let mut targets: std::collections::BTreeSet<_> =
        game_provisioning::frontiers(facts).into_iter().collect();
    targets.extend(
        game_knowledge::with_prefix(facts, "roads:")
            .take(128)
            .filter_map(|(k, _)| k.strip_prefix("roads:")?.parse::<u64>().ok().map(AreaId)),
    );
    if let Some(t) = state.trips.get(&p) {
        targets.extend([t.home, t.destination]);
    }
    let risk = game_vitality::disposition(tx, p, "risk-tolerance");
    let foresight = game_vitality::ability(tx, p, "foresight").min(16000) as i64;
    let desire = tx
        .get::<game_relationships::Social>()?
        .values
        .get(&p)
        .and_then(|v| v.get("discovery"))
        .copied()
        .unwrap_or(0);
    let hunger = tx.get::<game_physiology::Bodies>()?.people[&p]
        .hunger
        .min(1000) as i64;
    let satisfaction = state.next.get(&p).map_or(0, |at| {
        (60 * u128::from(DAY.0) / u128::from(DAY.0 + tx.now().0.saturating_sub(at.0))) as i64
    });
    for target in targets.into_iter().filter(|to| Some(*to) != area).take(64) {
        let Ok((_, duration)) = game_food_actions::next_step(tx, p, facts, &location, target)
        else {
            continue;
        };
        let travel = (duration.0 / MINUTE.0).min(10000) as i64;
        let novelty = state
            .visits
            .get(&p)
            .and_then(|r| r.get(&target))
            .map_or(30, |at| {
                (tx.now().0.saturating_sub(at.0) / DAY.0).min(30) as i64
            });
        let home = state.trips.get(&p).is_some_and(|t| t.home == target);
        let attachment = if home {
            game_vitality::disposition(tx, p, "empathy") / 25
        } else {
            0
        };
        let perceived_risk =
            (travel / 10 + hunger / 4) * (1000 - risk) / 1000 * (1000 + foresight / 2) / 1000;
        offers.push(Proposal::new(
            format!("journey:{}", target.0),
            motive(
                0,
                risk / 20 + novelty + i64::from(desire) * 2 - satisfaction,
                attachment,
                travel / 5,
                perceived_risk,
            ),
            Go { person: p, target },
        )?);
    }
    Ok(offers)
}
