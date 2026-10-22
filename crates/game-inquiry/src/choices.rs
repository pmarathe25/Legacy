use super::*;
pub(super) fn register(b: &mut Builder) -> Result<()> {
    game_choice::provider(b, "inquiry.opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let p = e.person;
        let curiosity = game_vitality::disposition(tx, p, "openness") / 10
            + game_vitality::disposition(tx, p, "contemplation") / 10;
        let (count, signature) = evidence(tx, p);
        let changed = count >= 2
            && tx
                .get::<Inquiry>()?
                .people
                .get(&p)
                .is_none_or(|m| m.reasoning_signature != signature);
        if changed
            || tx.get::<Inquiry>()?.people.get(&p).is_some_and(|m| {
                remembered(tx, p).len() >= 2
                    && (m.count > m.considered && m.contrasts
                        || m.considered > 0
                            && game_vitality::ability(tx, p, "intelligence") / 250 > m.capacity)
            })
        {
            game_choice::offer(
                tx,
                p,
                "inquiry:reflect",
                game_choice::motive(0, 0, curiosity, 10, 0),
                game_choice::Undertake {
                    person: p,
                    kind: "reason".into(),
                    duration: HOUR,
                    request: Pending::immediate(Reflect { person: p }),
                },
            )?;
        }
        Ok(())
    })?;
    b.subscribe::<game_social_life::Encounter>("inquiry.explanation", |tx, e| {
        for (teacher, listener) in [(e.first, e.second), (e.second, e.first)] {
            let interest = game_vitality::disposition(tx, teacher, "altruism")
                + game_vitality::disposition(tx, teacher, "sociability");
            if interest < 900 {
                continue;
            }
            let key = {
                let known: Vec<_> = perceived_claims(tx, teacher)
                    .map(|(claim, _)| claim)
                    .collect();
                let Some(claim) = known.get(
                    (random(tx.seed(), teacher.0, tx.now().0) % known.len().max(1) as u64) as usize,
                ) else {
                    continue;
                };
                claim.key()
            };
            let form = if game_vitality::disposition(tx, teacher, "patience") > 500 {
                Explanation::Practical
            } else {
                Explanation::Detailed
            };
            let _ = tx.attempt(Explain {
                teacher,
                listener,
                key,
                form,
            });
        }
        Ok(())
    })
}
