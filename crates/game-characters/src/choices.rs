use super::*;
use game_choice::Proposal;
use game_decision_intents::Proceed;
#[derive(Clone, Serialize, Deserialize)]
struct Execute {
    person: PersonId,
    proposal: Proposal,
}
impl Request for Execute {
    const KEY: &'static str = "characters.execute";
    type Response = game_choice::Outcome;
}
pub(super) fn review(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    reconsider(tx, p, false)
}
fn reconsider(tx: &mut Transaction<'_>, p: PersonId, fresh: bool) -> Result<()> {
    let _measurement = game_diagnostics::span("character review");
    let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let ruler = tx.get::<game_lineage::Lineage>()?.current;
    if !body.alive
        || body.health == 0
        || (ruler == Some(p) && game_invocation::controlled(tx.get()?, tx.now()))
    {
        return Ok(());
    }
    let acting = tx.get::<Actions>()?.0.get(&p).cloned();
    if acting.as_ref().is_some_and(|a| a.kind == "combat") {
        return Ok(());
    }
    if let Some(mut routine) = tx
        .get::<Minds>()?
        .people
        .get(&p)
        .and_then(|m| m.routine.clone())
    {
        let rules = tx.get::<Minds>()?.rules.clone();
        if let Some(action) = routine.resume(tx, p, rules.urgent_hunger, rules.rest_fatigue)? {
            let mind = tx.edit::<Minds>()?.people.get_mut(&p).unwrap();
            mind.engagement = Some(action);
            if mind.routine.is_some() {
                mind.routine = Some(routine);
            }
            return Ok(());
        }
        tx.edit::<Minds>()?.people.get_mut(&p).unwrap().routine = None;
    }
    let now = tx.now();
    if tx.get::<Minds>()?.people[&p].review_clock.is_some() {
        tx.edit::<Minds>()?
            .people
            .get_mut(&p)
            .unwrap()
            .review_clock
            .as_mut()
            .unwrap()
            .reviewed = Some(now);
    }
    let _preparation = game_diagnostics::span("full character review");
    if !game_choice::observed(tx, p) && game_transport::area(tx.get()?, p).is_ok() {
        tx.call(game_knowledge::ObserveSurroundings { person: p })?;
        let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        for inventory in packs {
            tx.call(game_knowledge::ObserveStore {
                person: p,
                inventory,
            })?;
        }
    }
    let prepared = game_choice::frame_decision::<game_decision_intents::Prepared>(tx, p)
        .filter(|d| d.person == p && d.at == tx.now())
        .filter(|d| {
            tx.get::<Minds>().is_ok_and(|m| {
                m.people
                    .get(&p)
                    .is_some_and(|mind| mind.plan == d.plan && mind.failures == d.failures)
            })
        });
    let snapshot = prepared.is_some();
    let (scored, reused, sources) = if let Some(prepared) = prepared {
        for effect in prepared.effects {
            tx.invoke(effect)?;
        }
        (prepared.scored, false, prepared.sources)
    } else {
        let state = tx.get::<Minds>()?;
        let rules = state.rules.clone();
        let memory = state.people[&p].considered.clone();
        let game_autonomy_policy::Options {
            offers,
            food: options,
            reused,
            remembered,
        } = game_autonomy_policy::consider(tx, p, &rules, memory.as_deref(), fresh)?;
        if let Some(memory) = remembered
            && let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&p)
        {
            mind.considered = Some(memory);
        }
        if !tx.get::<Minds>()?.people.contains_key(&p) {
            return Ok(());
        }
        (
            game_decision_intents::score(tx, p, offers, options?, acting.as_ref())?,
            reused,
            Default::default(),
        )
    };
    let game_decision_intents::Scored {
        mut offers,
        expected,
        continuing,
        margin,
    } = scored;
    let rules = tx.get::<Minds>()?.rules.clone();
    let now = tx.now();
    game_contracts::causal::candidates(
        p.0,
        offers.iter().map(|o| {
            (
                o.motive.id,
                [
                    o.motive.need,
                    o.motive.goal,
                    o.motive.value,
                    o.motive.effort,
                    o.motive.risk,
                ],
                o.continuation,
            )
        }),
    );
    for _ in 0..8 {
        let ongoing = tx.get::<Actions>()?.0.get(&p);
        if ongoing.is_none() {
            offers.retain(|o| o.continuation.is_none());
        }
        let Some(mut d) = choose(
            &offers.iter().map(|o| o.motive).collect::<Vec<_>>(),
            ongoing
                .filter(|a| acting.as_ref().is_some_and(|old| old.id == a.id))
                .and(continuing),
            if snapshot {
                margin
            } else {
                game_vitality::switching_margin(tx, p, rules.switching_margin)
            },
            false,
        ) else {
            break;
        };
        let index = offers.iter().position(|o| o.motive.id == d.id).unwrap();
        let proposal = offers.remove(index);
        if reused && proposal.key == "wait" {
            return reconsider(tx, p, true);
        }
        let continues = ongoing.is_some_and(|a| proposal.continuation == Some(a.id));
        if !continues
            && game_choice::frame_stale_provider(
                tx,
                p,
                &proposal,
                sources.get(&proposal.key).cloned(),
            )?
        {
            tx.edit::<Minds>()?.people.get_mut(&p).unwrap().frame_retry = None;
            continue;
        }
        if continues
            || tx
                .attempt(Execute {
                    person: p,
                    proposal: proposal.clone(),
                })
                .is_ok_and(|outcome| outcome == game_choice::Outcome::Performed)
        {
            let engagement = tx.get::<Actions>()?.0.get(&p).map(|a| a.id);
            let routine =
                game_autonomy_policy::Routine::capture(tx, p, &proposal, rules.routine.as_ref())?;
            let Some(m) = tx.edit::<Minds>()?.people.get_mut(&p) else {
                return Ok(());
            };
            m.retry_after.retain(|_, at| *at > now);
            m.retry_after.remove(&d.id);
            if !continues {
                m.expectation = expected.iter().find(|c| c.id == d.id).copied();
                if let Some(memory) = &mut m.considered {
                    Arc::make_mut(memory).consume(&proposal.key);
                }
            }
            d.reasons = vec![proposal.key.clone()];
            m.last = Some(d);
            m.chosen = Some(proposal.key);
            m.engagement = engagement;
            m.routine = routine.map(Box::new);
            return Ok(());
        }
        failure(tx, p, d.id)?;
        if reused {
            return reconsider(tx, p, true);
        }
    }
    Ok(())
}
fn failure(tx: &mut Transaction<'_>, person: PersonId, id: u64) -> Result<()> {
    let now = tx.now();
    let retry = now.after(tx.get::<Minds>()?.rules.retry)?;
    if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&person) {
        m.retry_after.retain(|_, at| *at > now);
        m.retry_after.insert(id, retry);
        m.failures = m.failures.saturating_add(1);
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_choice::Resolved>("characters.outcome", |tx, e| {
        let mind = tx.edit::<Minds>()?.people.get_mut(&e.person);
        let Some(mind) = mind.filter(|m| m.engagement == Some(e.action)) else {
            return Ok(());
        };
        mind.engagement = None;
        let id = mind.chosen.as_deref().map(game_choice::identity);
        if e.outcome == game_choice::Outcome::Reconsider
            && let Some(id) = id
        {
            tx.edit::<Minds>()?
                .people
                .get_mut(&e.person)
                .unwrap()
                .considered = None;
            tx.edit::<Minds>()?
                .people
                .get_mut(&e.person)
                .unwrap()
                .routine = None;
            failure(tx, e.person, id)?;
        }
        Ok(())
    })?;
    b.request::<Proceed>(|tx, r| {
        let (performed, next) = tx.call(plans::Advance {
            person: r.person,
            plan: r.plan,
        })?;
        if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&r.person) {
            m.plan = next;
        }
        Ok(performed.into())
    })?;
    b.request::<Execute>(|tx, r| {
        tx.cancel_action(r.person)?;
        if !r.proposal.key.starts_with("food:") && r.proposal.key != "reserve" {
            tx.edit::<Minds>()?.people.get_mut(&r.person).unwrap().plan = None;
        }
        tx.call(game_choice::Run(r.proposal.request))
    })?;
    b.subscribe::<game_choice::Changed>("characters.new-evidence", |tx, e| {
        scheduling::wake(tx, e.person, false, false)
    })
}
