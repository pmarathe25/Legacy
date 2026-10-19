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
use game_vitality::disposition;
pub fn ready(tx: &Transaction<'_>, person: PersonId) -> bool {
    let people = &tx.get::<SocialLife>().unwrap().people;
    people.get(&person).is_some_and(|p| p.area.is_some())
        && game_vitality::can_converse(tx, person)
        && game_physiology::capable(tx, person).is_ok()
        && !tx.get::<Actions>().unwrap().0.contains_key(&person)
}
pub fn affinity(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> i64 {
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
    let count = tx.get::<SocialLife>()?.people[&sender].encounters;
    let Some(facts) = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&sender)
        .filter(|f| !f.is_empty())
    else {
        return Ok(());
    };
    let key = facts
        .iter()
        .nth((count % facts.len() as u64) as usize)
        .map(|(key, _)| key.to_string());
    if let Some(key) = key {
        let _ = tx.attempt(game_testimony::Tell {
            sender,
            recipient,
            key,
        });
    }
    Ok(())
}
fn warmth(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> i32 {
    let social = tx.get::<game_relationships::Social>().unwrap();
    let difference = social.apparent_difference(person, other, &["sharing", "authority"]);
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
    b.request::<Finish>(|tx, r| {
        let now = tx.now();
        let retry_at = now;
        if game_communication::active(tx) {
            tx.call(game_communication::Practice {
                first: r.first,
                second: r.second,
            })?;
        }
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
            p.cursor = Some(other);
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
            let retry = tx.now();
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
