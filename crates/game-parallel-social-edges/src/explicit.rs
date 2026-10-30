//! Validate an explicit contact against each participant's committed state.
use game_parallel_social::{Encounter, Social};

pub fn resolve(
    social: &Social,
    people: &[(u64, Option<u64>, u32, u32)],
    request: (u64, u64, bool),
    busy: &[u64],
    controlled: Option<u64>,
    tick: u64,
    allowed: impl Fn(u64, u32) -> bool,
) -> Result<Option<Encounter>, &'static str> {
    let (actor, target, pressured) = request;
    if actor == target {
        return Err("cannot talk to oneself");
    }
    let find = |person| {
        people
            .binary_search_by_key(&person, |row| row.0)
            .ok()
            .map(|i| people[i])
    };
    let actor_state = find(actor).ok_or("unknown speaker")?;
    let target_state = find(target).ok_or("unknown target")?;
    if !social.participants.iter().any(|row| row.person == actor) {
        return Err("speaker cannot participate socially");
    }
    let target_policy = social
        .participants
        .iter()
        .find(|row| row.person == target)
        .ok_or("target cannot participate socially")?;
    let region = actor_state.1.ok_or("speaker must be local")?;
    if target_state.1 != Some(region) {
        return Err("target must be in the same region");
    }
    let eligible = |row: (u64, Option<u64>, u32, u32)| {
        row.2 > 0 && row.2 >= social.rules.min_health && row.3 <= social.rules.max_hunger
    };
    if !eligible(actor_state) {
        return Err("speaker cannot participate now");
    }
    let reserved = busy.iter().filter(|&&person| person == actor).count();
    if reserved > usize::from(controlled == Some(actor)) {
        return Err("speaker is occupied");
    }
    let isolation = social
        .needs
        .as_ref()
        .and_then(|needs| needs.people.iter().find(|row| row.person == target))
        .map_or(0, |row| row.isolation);
    if !eligible(target_state)
        || busy.contains(&target)
        || (!pressured && (!target_policy.willing || !allowed(target, isolation)))
    {
        return Ok(None);
    }
    Ok(Some(Encounter {
        tick,
        region,
        a: actor.min(target),
        b: actor.max(target),
        pressured: pressured.then_some(target),
    }))
}
