//! Cohorts follow existing action and mind deadlines; no new character clock.
use game_core::*;
use game_sim::*;
pub fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_decision_frames::Gather>("characters.frame-observations", |tx, _| {
        let minds = tx.get::<game_characters::Minds>()?;
        if !minds.running {
            return Ok(());
        }
        require(
            minds.rules.routine.is_none() && minds.rules.option_memory.is_none(),
            "decision frames require the ordinary attention policy",
        )?;
        let now = tx.now();
        let regular = minds.schedule.grid(now, minds.rules.review)?;
        let bodies = tx.get::<game_physiology::Bodies>()?;
        let people: Vec<_> = minds
            .people
            .keys()
            .copied()
            .filter(|p| {
                bodies
                    .people
                    .get(p)
                    .is_some_and(|b| b.alive && b.health > 0)
                    && ((regular == now && minds.schedule.ready.contains_key(p))
                        || minds.schedule.waking.get(p).is_some_and(|at| *at <= now)
                        || minds.schedule.agenda.get(p).is_some_and(|at| *at <= now))
            })
            .collect();
        if game_decision_frames::demand(tx) {
            let live = |p: &PersonId| {
                minds.people.contains_key(p)
                    && bodies
                        .people
                        .get(p)
                        .is_some_and(|b| b.alive && b.health > 0)
            };
            let ready = if minds.schedule.ready.keys().any(&live) {
                Some(
                    minds
                        .schedule
                        .grid(now.after(SimDuration(1))?, minds.rules.review)?,
                )
            } else {
                None
            };
            let deadlines = minds
                .schedule
                .waking
                .iter()
                .chain(minds.schedule.agenda.iter())
                .filter(|(p, _)| live(p))
                .map(|(_, at)| *at);
            if let Some(at) = deadlines.chain(ready).min() {
                game_decision_frames::wake(tx, at)?;
            }
        }
        let tuning = game_decision_frames::tuning(tx);
        let people = if tuning.activity_epochs || tuning.mechanics_sessions {
            tx.call(game_character_activities::Epoch(people))?
        } else {
            people
        };
        if game_decision_frames::tuning(tx).batch_observations {
            tx.call(game_knowledge::ObserveFrame {
                people: people.clone(),
            })?;
        } else {
            for &person in &people {
                if game_transport::area(tx.get()?, person).is_ok() {
                    tx.call(game_knowledge::ObserveSurroundings { person })?;
                    let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(person).collect();
                    for inventory in packs {
                        tx.call(game_knowledge::ObserveStore { person, inventory })?;
                    }
                }
            }
        }
        if people.is_empty() {
            return Ok(());
        }
        tx.call(game_decision_frames::Batch { people })
    })
}
