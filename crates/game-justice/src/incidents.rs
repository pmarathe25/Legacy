use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Prosecute {
    pub claimant: PersonId,
    pub defendant: PersonId,
}
impl Request for Prosecute {
    const KEY: &'static str = "justice.prosecute";
    type Response = CaseId;
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_property::Taking>("justice.witnessed-taking", |tx, e| {
        let Some(transfer) = &e.transfer else {
            return Ok(());
        };
        if !e.unauthorized {
            return Ok(());
        }
        let owners: Vec<_> = tx
            .get::<game_property::Property>()?
            .claims
            .get(&e.inventory)
            .into_iter()
            .flatten()
            .map(|c| c.claimant)
            .filter(|p| *p != e.actor && e.witnesses.contains(p))
            .collect();
        for claimant in owners {
            let incident = Case {
                claimant,
                defendant: e.actor,
                evidence: Evidence {
                    taking: true,
                    consent: false,
                    necessity: false,
                    credible: true,
                },
                transfer: game_cargo::Transfer {
                    from: transfer.to,
                    to: transfer.from,
                    good: transfer.good,
                    quantity: transfer.quantity,
                },
                remedy: None,
                enforced: false,
            };
            tx.edit::<Justice>()?
                .incidents
                .entry(claimant)
                .or_default()
                .insert(e.actor, incident);
            tx.notify(game_choice::Changed { person: claimant })?;
        }
        Ok(())
    })?;
    b.request::<Prosecute>(|tx, r| {
        game_physiology::capable(tx, r.claimant)?;
        let incident = tx
            .edit::<Justice>()?
            .incidents
            .get_mut(&r.claimant)
            .and_then(|row| row.remove(&r.defendant))
            .ok_or_else(|| Error::Missing("remembered incident".into()))?;
        tx.call(File(incident))
    })?;
    game_choice::provider(b, "justice.filing-opportunities", |tx, e| {
        if !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let defendants: Vec<_> = tx
            .get::<Justice>()?
            .incidents
            .get(&e.person)
            .into_iter()
            .flat_map(|r| r.keys())
            .take(8)
            .copied()
            .collect();
        let sharing = tx
            .get::<game_relationships::Social>()
            .ok()
            .and_then(|s| s.beliefs.get(&e.person))
            .map_or(0, |b| i64::from(b.sharing));
        for defendant in defendants {
            let conviction = game_vitality::disposition(tx, e.person, "conviction") / 10;
            game_choice::offer(
                tx,
                e.person,
                format!("justice:file:{}", defendant.0),
                game_choice::motive(0, 0, conviction - sharing * 5, 10, 0),
                game_choice::Undertake {
                    person: e.person,
                    kind: "file-claim".into(),
                    duration: HOUR,
                    request: Pending::immediate(Prosecute {
                        claimant: e.person,
                        defendant,
                    }),
                },
            )?;
        }
        Ok(())
    })
}
