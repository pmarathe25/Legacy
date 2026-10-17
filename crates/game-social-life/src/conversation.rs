use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meeting {
    pub other: PersonId,
    pub job: ScheduleId,
    pub until: SimTime,
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    first: PersonId,
    second: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "social-life.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "social-life.cancel";
    type Response = ();
}
fn disposition(tx: &Transaction<'_>, p: PersonId, key: &str) -> i64 {
    game_vitality::disposition(tx, p, key)
}
fn ready(tx: &Transaction<'_>, person: PersonId) -> bool {
    let Ok(state) = tx.get::<SocialLife>() else {
        return false;
    };
    game_vitality::can_converse(tx, person)
        && state.people.get(&person).is_some_and(|p| p.area.is_some())
        && tx.get::<game_physiology::Bodies>().is_ok_and(|s| {
            s.people
                .get(&person)
                .is_some_and(|p| p.alive && p.health > 0)
        })
        && tx
            .get::<Actions>()
            .is_ok_and(|a| !a.0.contains_key(&person))
}
fn affinity(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&person)?.get(&other))
        .map_or(0, |r| {
            i64::from(r.affection) + i64::from(r.trust) / 2
                - i64::from(r.fear)
                - i64::from(r.grievance)
        })
}
fn willing(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> bool {
    if !ready(tx, person) {
        return false;
    }
    let body = &tx.get::<game_physiology::Bodies>().unwrap().people[&person];
    let grievance = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&person)?.get(&other))
        .map_or(0, |r| i64::from(r.grievance));
    200 + affinity(tx, person, other) * 3 + company_motivation(tx, person) - grievance * 8
        > body.hunger.min(10000) as i64 * 2 + body.fatigue.min(10000) as i64 * 3
}
fn share(tx: &mut Transaction<'_>, sender: PersonId, recipient: PersonId) -> Result<()> {
    // Rotate through the speaker's memories without consulting the listener's private knowledge.
    let knowledge = tx.get::<game_knowledge::Knowledge>()?;
    let count = tx.get::<SocialLife>()?.people[&sender].encounters;
    let key = knowledge
        .0
        .get(&sender)
        .filter(|f| !f.is_empty())
        .and_then(|facts| {
            facts
                .iter()
                .nth((count % facts.len() as u64) as usize)
                .map(|(key, _)| key.to_string())
        });
    if let Some(key) = key {
        tx.call(game_knowledge::Tell {
            sender,
            recipient,
            key,
        })?;
    }
    Ok(())
}
fn warmth(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> i32 {
    let social = tx.get::<game_relationships::Social>().unwrap();
    let own = social.beliefs.get(&person).cloned().unwrap_or_default();
    let theirs = social.beliefs.get(&other).cloned().unwrap_or_default();
    // Differences in expressed attitudes can grate; empathy moderates this person's response.
    let difference = (i64::from(own.sharing) - i64::from(theirs.sharing)).abs()
        + (i64::from(own.authority) - i64::from(theirs.authority)).abs();
    (2 + disposition(tx, person, "empathy") / 250 - difference.min(60) / 10).clamp(-5, 5) as i32
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Talk>(|tx, r| {
        require(r.person != r.other, "cannot converse with oneself")?;
        require(
            ready(tx, r.person) && willing(tx, r.other, r.person),
            "company declined or unavailable",
        )?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_transport::area(tx.get()?, r.other)?,
            "company is elsewhere",
        )?;
        let until = tx.now().after(tx.get::<SocialLife>()?.rules.duration)?;
        let job = tx.schedule(
            until,
            10,
            Finish {
                first: r.person,
                second: r.other,
            },
        )?;
        for (person, other) in [(r.person, r.other), (r.other, r.person)] {
            tx.start_action(person, "socialize", Some(until), Cancel { person })?;
            tx.edit::<SocialLife>()?
                .meetings
                .insert(person, Meeting { other, job, until });
        }
        Ok(until)
    })?;
    b.request::<SeekCompany>(|tx, r| {
        if !ready(tx, r.person) {
            return Ok(false);
        }
        if company_motivation(tx, r.person) <= 0 {
            return Ok(false);
        }
        let state = tx.get::<SocialLife>()?;
        let p = &state.people[&r.person];
        if p.retry_at > tx.now() {
            return Ok(false);
        }
        let area = p.area.unwrap();
        // Rotate a bounded local window so a crowded settlement never starves later identities.
        let residents = &state.residents[&area];
        let peers: Vec<_> = residents
            .scan_after(p.cursor.as_ref())
            .chain(residents.iter())
            .take(residents.len().min(state.rules.candidates))
            .map(|(p, _)| *p)
            .collect();
        let explore = random(tx.seed(), r.person.0, p.encounters.wrapping_add(901)) % 1000
            < ((disposition(tx, r.person, "sociability")
                + disposition(tx, r.person, "risk-tolerance"))
                / 4) as u64;
        let familiar = tx
            .get::<game_relationships::Social>()?
            .relationships
            .get(&r.person);
        let chosen = peers
            .iter()
            .copied()
            .filter(|other| {
                *other != r.person
                    && Some(*other) != r.protected
                    && willing(tx, *other, r.person)
                    && affinity(tx, r.person, *other) >= -20
            })
            .max_by_key(|other| {
                (
                    affinity(tx, r.person, *other) - if p.recent == Some(*other) { 10 } else { 0 }
                        + if explore && familiar.is_none_or(|ties| !ties.contains_key(other)) {
                            200
                        } else {
                            0
                        },
                    std::cmp::Reverse(*other),
                )
            });
        let retry_at = tx.now().after(state.rules.retry)?;
        let p = tx.edit::<SocialLife>()?.people.get_mut(&r.person).unwrap();
        p.cursor = peers.last().copied();
        if let Some(other) = chosen {
            tx.call(Talk {
                person: r.person,
                other,
            })?;
            Ok(true)
        } else {
            p.retry_at = retry_at;
            Ok(false)
        }
    })?;
    b.request::<Finish>(|tx, r| {
        let now = tx.now();
        let retry_at = now.after(tx.get::<SocialLife>()?.rules.retry)?;
        for (person, other) in [(r.first, r.second), (r.second, r.first)] {
            game_physiology::capable(tx, person)?;
            let warmth = warmth(tx, person, other);
            tx.call(game_relationships::Converse {
                observer: person,
                other,
                warmth,
            })?;
            tx.call(game_relationships::LearnValue {
                person,
                concept: "fellowship".into(),
                outcome: warmth,
            })?;
            share(tx, person, other)?;
            let p = tx.edit::<SocialLife>()?.people.get_mut(&person).unwrap();
            p.last_contact = now;
            p.retry_at = retry_at;
            p.recent = Some(other);
            p.encounters = p.encounters.saturating_add(1);
            tx.edit::<SocialLife>()?.meetings.remove(&person);
            tx.finish_action(person, "socialize")?;
        }
        tx.emit(
            Encounter {
                first: r.first,
                second: r.second,
            },
            vec![r.first, r.second],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(meeting) = tx.edit::<SocialLife>()?.meetings.remove(&r.person) {
            tx.unschedule(meeting.job);
            tx.edit::<SocialLife>()?.meetings.remove(&meeting.other);
            tx.finish_action(meeting.other, "socialize")?;
            let retry = tx.now().after(tx.get::<SocialLife>()?.rules.retry)?;
            for p in [r.person, meeting.other] {
                tx.edit::<SocialLife>()?
                    .people
                    .get_mut(&p)
                    .unwrap()
                    .retry_at = retry;
            }
        }
        Ok(())
    })
}
