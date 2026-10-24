use super::*;
use game_autonomy_policy::AttentionRates;
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureAttention(pub Option<AttentionRates>);
impl Request for ConfigureAttention {
    const KEY: &'static str = "characters.configure-attention";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureBusyAttention(pub bool);
impl Request for ConfigureBusyAttention {
    const KEY: &'static str = "characters.configure-busy-attention";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<ConfigureBusyAttention>(|tx, r| {
        if tx.get::<Minds>()?.rules.busy_attention == r.0 {
            return Ok(());
        }
        std::sync::Arc::make_mut(&mut tx.edit::<Minds>()?.rules).busy_attention = r.0;
        if r.0 {
            let people: Vec<_> = tx
                .get::<Minds>()?
                .people
                .iter()
                .filter(|(p, m)| {
                    m.review_clock.is_some_and(|c| c.pending().is_some())
                        && !tx.get::<Actions>().unwrap().0.contains_key(p)
                })
                .map(|(p, _)| *p)
                .collect();
            for person in people {
                super::wake(tx, person, false, true)?;
            }
        }
        Ok(())
    })?;
    b.before::<game_choice::ConfigureFrames>("characters.attention-compatible", |tx, r| {
        require(
            r.interval.is_none()
                || (tx.get::<Minds>()?.rules.attention.is_none()
                    && !tx.get::<game_choice::Offers>()?.goal_execution),
            "disable attention and goal execution before frames",
        )
    })?;
    b.request::<ConfigureAttention>(|tx, r| {
        if tx.get::<Minds>()?.rules.attention == r.0 {
            return Ok(());
        }
        require(
            tx.get::<game_choice::Offers>()?.frame.is_none(),
            "attention rates require event-driven decisions",
        )?;
        require(
            r.0.is_some() || !tx.get::<game_choice::Offers>()?.goal_execution,
            "disable goal execution before attention",
        )?;
        require(
            tx.get::<Minds>()?.rules.option_memory.is_none(),
            "disable option memory before attention",
        )?;
        if let Some(rates) = r.0 {
            rates.validate()?;
        }
        let people: Vec<_> = tx.get::<Minds>()?.people.keys().copied().collect();
        let rules = std::sync::Arc::make_mut(&mut tx.edit::<Minds>()?.rules);
        rules.attention = r.0;
        rules.routine = None;
        for person in people {
            let mind = &tx.get::<Minds>()?.people[&person];
            let mut clock = mind.review_clock.unwrap_or_default();
            let pending = clock.pending().is_some();
            clock.consume(tx);
            if tx.get::<Minds>()?.running {
                if let Some(rates) = r.0
                    && pending
                {
                    clock.schedule(tx, rates.ordinary, false, Resume { person })?;
                } else if r.0.is_none() && pending {
                    tx.schedule(tx.now(), 50, Resume { person })?;
                }
            }
            let mind = tx.edit::<Minds>()?.people.get_mut(&person).unwrap();
            mind.routine = None;
            mind.review_clock = Some(clock);
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let minds = h.get::<Minds>()?;
        require(
            !h.get::<game_choice::Offers>()?.goal_execution
                || (minds.rules.review_floor().is_some()
                    && h.get::<game_choice::Offers>()?.frame.is_none()),
            "invalid goal execution policy",
        )
    });
    Ok(())
}
