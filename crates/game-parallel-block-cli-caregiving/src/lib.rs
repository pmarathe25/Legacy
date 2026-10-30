//! Personal caregiver preferences and observations.
use game_parallel::CaregivingPolicy;
use game_parallel_fused_physical::Kernel;
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    let command = words.next();
    if !matches!(
        command,
        Some("care-policy" | "caregiving" | "farm-policy" | "farm-status")
    ) {
        return Ok(false);
    }
    let person = words
        .next()
        .ok_or("missing person")?
        .parse()
        .map_err(|_| "invalid person")?;
    if command == Some("farm-status") {
        if words.next().is_some() {
            return Err("unexpected argument".into());
        }
        let state = engine.export();
        let agriculture = state
            .agriculture
            .as_ref()
            .ok_or("agriculture is disabled")?;
        match &agriculture.volunteers {
            None => println!("farm person={person} policy=unrestricted"),
            Some(volunteers) => println!(
                "farm person={person} volunteering={} refused={}",
                volunteers.contains(&person),
                agriculture.refusals.contains(&person)
            ),
        }
    } else if command == Some("farm-policy") {
        let willing = match words.next() {
            Some("enable") => true,
            Some("disable") => false,
            _ => return Err("farm policy must be enable or disable".into()),
        };
        if words.next().is_some() {
            return Err("unexpected argument".into());
        }
        engine.set_farm_volunteer(person, willing)?;
    } else if command == Some("care-policy") {
        let willing = match words.next() {
            Some("enable") => true,
            Some("disable") => false,
            _ => return Err("care policy must be enable or disable".into()),
        };
        if words.next().is_some() {
            return Err("unexpected argument".into());
        }
        engine.set_caregiver_policy(CaregivingPolicy { person, willing })?;
    } else {
        if words.next().is_some() {
            return Err("unexpected argument".into());
        }
        let state = engine.export();
        let care = state.caregiving.as_ref().ok_or("caregiving is disabled")?;
        for event in care
            .events
            .iter()
            .filter(|event| event.child == person || event.caregiver == Some(person))
        {
            println!("care child={} caregiver={:?}", event.child, event.caregiver);
        }
    }
    Ok(true)
}
