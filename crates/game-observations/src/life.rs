use super::*;
fn practice(tx: &mut Transaction<'_>, person: PersonId, effort: u64) -> Result<()> {
    if tx.registered::<game_skills::Practice>() {
        let attribution = tx.allocate()?;
        tx.call(game_skills::Practice {
            person,
            skill: "child-care".into(),
            effort: effort.max(1),
            attribution,
        })?;
    }
    Ok(())
}
fn witness(
    tx: &mut Transaction<'_>,
    p: PersonId,
    subject: String,
    outcome: &str,
    context: BTreeMap<String, i64>,
    value: i64,
) -> Result<()> {
    if game_physiology::capable(tx, p).is_ok() {
        tx.call(Witness {
            observer: p,
            observation: Observation {
                subject,
                outcome: outcome.into(),
                context,
                value,
                trial: 0,
            },
        })?;
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.before::<game_physiology::Serve>("experience.before-care", |tx, r| {
        if r.caregiver == r.patient {
            return Ok(());
        }
        let perceived = tx
            .attempt(game_signals::Notice {
                observer: r.caregiver,
                person: r.patient,
            })
            .ok()
            .map(|m| m.estimated_need)
            .unwrap_or(0);
        let hunger = tx
            .get::<game_physiology::Bodies>()?
            .people
            .get(&r.patient)
            .map_or(0, |b| b.hunger);
        tx.edit::<Observations>()?
            .care
            .insert(r.patient, (r.caregiver, perceived, hunger));
        Ok(())
    })?;
    b.subscribe::<game_physiology::Fed>("experience.care", |tx, e| {
        let Some((giver, before, own_before)) = tx.edit::<Observations>()?.care.remove(&e.patient)
        else {
            return Ok(());
        };
        if giver != e.caregiver {
            return Ok(());
        }
        let context = BTreeMap::from([
            ("dose".into(), e.quantity.min(i64::MAX as u64) as i64),
            ("food-id".into(), e.good.0.min(i64::MAX as u64) as i64),
        ]);
        let own_after = tx.get::<game_physiology::Bodies>()?.people[&e.patient].hunger;
        witness(
            tx,
            e.patient,
            "child-care".into(),
            "relief",
            context.clone(),
            own_before.saturating_sub(own_after).min(i64::MAX as u64) as i64,
        )?;
        let after = if tx.registered::<game_signals::Notice>() {
            tx.attempt(game_signals::Notice {
                observer: giver,
                person: e.patient,
            })
            .ok()
            .map(|p| p.estimated_need)
        } else {
            None
        };
        if let Some(after) = after {
            witness(
                tx,
                giver,
                "child-care".into(),
                "relief",
                context,
                (before as i128 - after as i128).clamp(i64::MIN as i128, i64::MAX as i128) as i64,
            )?;
        }
        practice(tx, giver, 2 + e.quantity.min(20))
    })?;
    b.before::<game_knowledge::Receive>("experience.care-lesson", |tx, r| {
        if !r.key.starts_with("theory:child-care:") {
            return Ok(());
        }
        let old = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&r.person)
            .and_then(|f| f.get(&r.key));
        if old.is_some_and(|f| {
            f.value.as_ref() == r.fact.value.as_ref()
                || (f.observed, f.confidence) > (r.fact.observed, r.fact.confidence)
        }) {
            return Ok(());
        }
        if r.fact
            .value
            .decode::<game_reasoning::Claim>()
            .is_some_and(|c| c.subject == "child-care")
        {
            practice(tx, r.person, u64::from(r.fact.confidence) / 4 + 1)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_conflict::Struck>("experience.conflict", |tx, e| {
        let bodies = tx.get::<game_physiology::Bodies>()?;
        let body = &bodies.people[&e.target];
        let max = body.maximum(&bodies.rules).max(1);
        let wounds = 100
            - body
                .health
                .saturating_add(e.amount)
                .saturating_mul(100)
                .checked_div(max)
                .unwrap_or(100)
                .min(100);
        let own = game_vitality::ability(tx, e.target, "size").max(100);
        let apparent = (game_vitality::ability(tx, e.actor, "size") / 100 * 100).max(100);
        witness(
            tx,
            e.target,
            format!("danger:{}", e.actor.0),
            "harm",
            BTreeMap::from([
                (
                    "relative-build".into(),
                    apparent
                        .saturating_mul(100)
                        .checked_div(own)
                        .unwrap_or(100)
                        .min(10000) as i64,
                ),
                ("wounds".into(), wounds as i64),
            ]),
            e.amount.min(i64::MAX as u64) as i64,
        )
    })?;
    b.subscribe::<game_physiology::Damaged>("experience.danger", |tx, e| {
        witness(
            tx,
            e.person,
            "bodily-danger".into(),
            "harm",
            weather(tx, e.person),
            e.amount.min(i64::MAX as u64) as i64,
        )
    })?;
    b.before::<game_relationships::Converse>("experience.social", |tx, e| {
        if !game_transport::together(tx.get()?, e.observer, e.other) {
            return Ok(());
        }
        witness(
            tx,
            e.observer,
            "social-contact".into(),
            "warmth",
            BTreeMap::from([("partner-id".into(), e.other.0.min(i64::MAX as u64) as i64)]),
            i64::from(e.warmth.clamp(-5, 5)),
        )
    })
}
