//! Needs, bodily harm, rest, and guardian recovery.
pub use game_body::{
    Body, BodyView, CheckLocalMeal, CheckSelfMeal, ConsumeEdible, Damaged, DeathCause, Died,
    Eating, Exert, Fed, Kind, Rates, Rested, Rules, Serve, advance_body, hunger_threshold, load,
};
use game_core::*;
use game_sim::*;
pub use game_soma::{
    Bodies, Configure, Create, Damage, Express, Heal, MetabolicEnvironment, ShortenLife, Strain,
    capable, maximum_health, view,
};
use serde::{Deserialize, Serialize};
mod meal_wave;
pub use game_physiology_eating::{
    Administer, Eat, Feed, OfferOutcome, Restrained, ServeLocalMeals, TryServe,
};
pub use game_rest_activities::{
    CheckRestActivities, InterruptRest, RestActivities, RestActivity, resting_activity,
};
pub use meal_wave::{ConsumeSelfMeals, TryConsumeSelfMeals};
#[derive(Clone, Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "physiology.start";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Tick;
impl Request for Tick {
    const KEY: &'static str = "physiology.tick";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rest {
    pub person: PersonId,
}
impl Request for Rest {
    const KEY: &'static str = "physiology.rest";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Wake {
    person: PersonId,
}
impl Request for Wake {
    const KEY: &'static str = "physiology.wake";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct CancelRest {
    person: PersonId,
    job: ScheduleId,
}
impl Request for CancelRest {
    const KEY: &'static str = "physiology.cancel_rest";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_soma::plugin(b)?;
    game_digestion::plugin(b)?;
    b.plugin("physiology", 1)?;
    game_rest_activities::register(b)?;
    b.request::<game_property::Observers>(|tx, r| {
        let positions = tx.get::<game_transport::Positions>()?;
        let mut observers:Vec<_> = tx.get::<Bodies>()?.people.rows()
            .filter(|(_,b)| b.alive && b.health > 0 && !b.resting)
            .filter(|(p,_)| matches!(positions.0.get(p),Some(game_transport::Location::At(a)) if *a == r.area))
            .map(|(p,_)| *p).collect();
        observers.sort_unstable();
        Ok(observers)
    })?;

    b.validate_save(|h| {
        for person in h.get::<game_transport::Positions>()?.0.keys() {
            require(
                h.get::<Bodies>()?.people.contains_key(person),
                "position references unknown body",
            )?;
        }
        Ok(())
    });

    b.request::<Start>(|tx, _| {
        tx.call(game_soma::Begin)?;
        tx.schedule(tx.now().after(tx.get::<Bodies>()?.rules.tick)?, 0, Tick)?;
        Ok(())
    })?;
    b.request::<Tick>(|tx, _| {
        tx.call(game_soma::Advance)?;
        tx.schedule(tx.now().after(tx.get::<Bodies>()?.rules.tick)?, 0, Tick)?;
        Ok(())
    })?;
    game_physiology_eating::register(b)?;
    meal_wave::register(b)?;
    b.request::<Rest>(|tx, r| {
        capable(tx, r.person)?;
        require(
            tx.get::<game_transport::Positions>()?
                .0
                .contains_key(&r.person),
            "rest needs a physical location",
        )?;
        let until = tx.now().after(tx.get::<Bodies>()?.rules.tick.max(HOUR))?;
        let job = tx.schedule(until, 10, Wake { person: r.person })?;
        tx.start_action(
            r.person,
            "rest",
            Some(until),
            CancelRest {
                person: r.person,
                job,
            },
        )?;
        tx.call(game_soma::SetResting {
            person: r.person,
            resting: true,
        })?;
        Ok(until)
    })?;
    b.request::<Wake>(|tx, r| {
        tx.finish_action(r.person, "rest")?;
        tx.call(game_soma::SetResting {
            person: r.person,
            resting: false,
        })?;
        tx.emit(Rested { person: r.person }, vec![r.person])
    })?;
    b.batch::<Wake>(128)?;
    b.request::<CancelRest>(|tx, r| {
        tx.unschedule(r.job);
        tx.call(game_soma::SetResting {
            person: r.person,
            resting: false,
        })?;
        Ok(())
    })?;
    b.before::<game_property::Take>("physiology.taking", |tx, r| capable(tx, r.actor))?;
    b.before::<game_transport::Move>("physiology.capability", |tx, r| capable(tx, r.person))?;

    Ok(())
}
