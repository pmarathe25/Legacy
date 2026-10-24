//! One actor task includes all providers, food queries and final scoring.
use super::*;
use rayon::prelude::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prepared {
    pub person: PersonId,
    pub at: SimTime,
    pub scored: Scored,
    pub effects: Vec<Pending>,
    pub sources: BTreeMap<String, String>,
    pub plan: Option<Plan>,
    pub failures: u64,
}
impl Request for Prepared {
    const KEY: &'static str = "characters.frame-decision";
    type Response = ();
}
fn prepare(tx: &Transaction<'_>, person: PersonId, fanout: bool) -> Option<Pending> {
    let _span = game_diagnostics::span("actor decision preparation");
    let mut scored = None;
    let draft = tx.draft(game_choice::Offers::planning(fanout), |child| {
        let offers = child.call(game_choice::Collect { person })?;
        let count = child.get::<Minds>()?.rules.gathering_meals;
        let food = plans::options(child, person, count)?;
        let acting = child.get::<Actions>()?.0.get(&person);
        scored = Some(score(child, person, offers, food, acting)?);
        Ok(())
    });
    if !draft.accepted {
        return None;
    }
    let (effects, sources) = draft.state.finish_planning();
    Some(Pending::immediate(Prepared {
        person,
        at: tx.now(),
        scored: scored?,
        effects,
        sources,
        plan: tx.get::<Minds>().ok()?.people.get(&person)?.plan.clone(),
        failures: tx.get::<Minds>().ok()?.people.get(&person)?.failures,
    }))
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<Prepared>(|_, _| {
        Err(Error::Rejected("prepared decision is not an action".into()))
    })?;
    b.subscribe::<game_choice::PlanCohort>("characters.prepare-decisions", |tx, e| {
        if e.people.len() < game_choice::frame_tuning(tx).minimum_cohort {
            return Ok(());
        }
        let fanout = e.people.len() < tx.compute().current_num_threads() * 2;
        let rows = tx.compute().install(|| {
            e.people
                .par_iter()
                .map(|&p| (p, prepare(tx, p, fanout)))
                .collect()
        });
        tx.call(game_choice::StoreDecisions(rows))
    })
}
