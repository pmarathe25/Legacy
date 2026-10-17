use super::*;
pub type Heard = Table<PersonId, Table<PersonId, Table<Arc<str>, Opinion>>>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opinion {
    pub stance: i32,
    pub via: PersonId,
    pub at: SimTime,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coerced: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct HearOpinion {
    pub listener: PersonId,
    pub speaker: PersonId,
    pub via: PersonId,
    pub concept: String,
    pub stance: i32,
    pub coerced: bool,
}
impl Request for HearOpinion {
    const KEY: &'static str = "relationships.hear-opinion";
    type Response = Option<Opinion>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Regard {
    pub observer: PersonId,
    pub other: PersonId,
    pub trust: i32,
    pub respect: i32,
    pub admiration: i32,
}
impl Request for Regard {
    const KEY: &'static str = "relationships.regard";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ReviseOpinion {
    pub person: PersonId,
    pub concept: String,
    pub change: i32,
}
impl Request for ReviseOpinion {
    const KEY: &'static str = "relationships.revise-opinion";
    type Response = i32;
}
impl Social {
    pub fn apparent_difference(
        &self,
        listener: PersonId,
        speaker: PersonId,
        concepts: &[&str],
    ) -> i64 {
        concepts
            .iter()
            .filter_map(|concept| {
                self.heard(listener, speaker, concept)
                    .filter(|o| !o.coerced)
                    .map(|o| {
                        (i64::from(self.opinion(listener, concept)) - i64::from(o.stance)).abs()
                    })
            })
            .sum()
    }
    pub fn heard(&self, listener: PersonId, speaker: PersonId, concept: &str) -> Option<&Opinion> {
        self.heard.get(&listener)?.get(&speaker)?.get(concept)
    }
    pub fn opinion(&self, person: PersonId, concept: &str) -> i32 {
        if let Some(b) = self.beliefs.get(&person) {
            match concept {
                "sharing" => return b.sharing,
                "authority" => return b.authority,
                "honesty" => return b.honesty,
                _ => {}
            }
        }
        self.values
            .get(&person)
            .and_then(|v| v.get(concept))
            .copied()
            .unwrap_or(0)
    }
}
pub(super) fn validate(heard: &Heard, previous: &Heard) -> Result<()> {
    let speakers = Table::new();
    let topics = Table::new();
    for (listener, rows, old) in heard.changed_pairs(previous) {
        for (speaker, opinions, before) in rows.changed_pairs(old.unwrap_or(&speakers)) {
            require(listener != speaker, "self testimony")?;
            require(
                opinions
                    .changed(before.unwrap_or(&topics))
                    .all(|(concept, o)| {
                        !concept.trim().is_empty()
                            && o.via != *listener
                            && (-100..=100).contains(&o.stance)
                    }),
                "invalid heard opinion",
            )?;
        }
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<HearOpinion>(|tx, r| {
        require(
            r.listener != r.speaker
                && r.listener != r.via
                && !r.concept.trim().is_empty()
                && (-100..=100).contains(&r.stance),
            "invalid testimony",
        )?;
        let at = tx.now();
        Ok(tx
            .edit::<Social>()?
            .heard
            .entry(r.listener)
            .or_default()
            .entry(r.speaker)
            .or_default()
            .insert(
                r.concept.into(),
                Opinion {
                    stance: r.stance,
                    via: r.via,
                    at,
                    coerced: r.coerced,
                },
            ))
    })?;
    b.request::<Regard>(|tx, r| {
        require(r.observer != r.other, "self regard")?;
        let tie = tx
            .edit::<Social>()?
            .relationships
            .entry(r.observer)
            .or_default()
            .entry(r.other)
            .or_default();
        tie.trust = tie.trust.saturating_add(r.trust).clamp(-100, 100);
        tie.respect = tie.respect.saturating_add(r.respect).clamp(-100, 100);
        tie.admiration = tie.admiration.saturating_add(r.admiration).clamp(0, 100);
        Ok(())
    })?;
    b.request::<ReviseOpinion>(|tx, r| {
        require(!r.concept.trim().is_empty(), "opinion needs a concept")?;
        if !["sharing", "authority", "honesty"].contains(&r.concept.as_str()) {
            return tx.call(LearnValue {
                person: r.person,
                concept: r.concept,
                outcome: r.change,
            });
        }
        let beliefs = tx.edit::<Social>()?.beliefs.entry(r.person).or_default();
        let value = match r.concept.as_str() {
            "sharing" => &mut beliefs.sharing,
            "authority" => &mut beliefs.authority,
            _ => &mut beliefs.honesty,
        };
        *value = value.saturating_add(r.change.clamp(-5, 5)).clamp(-100, 100);
        Ok(*value)
    })?;
    b.validate_save(|h| {
        require(
            h.get::<Social>()?
                .heard
                .values()
                .flat_map(|v| v.values())
                .flat_map(|v| v.values())
                .all(|o| o.at <= h.now()),
            "testimony from the future",
        )
    });
    Ok(())
}
