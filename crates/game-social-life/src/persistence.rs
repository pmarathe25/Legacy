use super::*;
pub(super) fn validate(s: &SocialLife) -> Result<()> {
    s.rules.validate()?;
    let mut residents = Table::<AreaId, Table<PersonId, ()>>::new();
    for (id, p) in &s.people {
        require(
            !p.name.trim().is_empty()
                && p.recent != Some(*id)
                && p.recent.is_none_or(|other| s.people.contains_key(&other)),
            "invalid social person",
        )?;
        if let Some(area) = p.area {
            residents.entry(area).or_default().insert(*id, ());
        }
    }
    require(residents == s.residents, "invalid local company index")?;
    for (id, m) in &s.meetings {
        require(
            s.people.contains_key(id)
                && s.people.contains_key(&m.other)
                && *id != m.other
                && s.meetings.get(&m.other).is_some_and(|other| {
                    other.other == *id && other.job == m.job && other.until == m.until
                }),
            "invalid conversation pair",
        )?;
    }
    Ok(())
}
pub(super) fn validate_host(h: &Host) -> Result<()> {
    let state = h.get::<SocialLife>()?;
    for (id, p) in &state.people {
        let body = h
            .get::<game_physiology::Bodies>()?
            .people
            .get(id)
            .ok_or_else(|| Error::InvalidSave("social person without body".into()))?;
        let area = if body.alive {
            game_transport::area(h.get()?, *id).ok()
        } else {
            None
        };
        if p.last_contact > h.now() || p.area != area {
            return Err(Error::Rejected(format!(
                "invalid social location or memory for {id:?}: last_contact={:?}, now={:?}, social_area={:?}, actual_area={area:?}, alive={}",
                p.last_contact,
                h.now(),
                p.area,
                body.alive
            )));
        }
    }
    for (id, m) in &state.meetings {
        require(
            m.until >= h.now()
                && state.people[id].area.is_some()
                && state.people[id].area == state.people[&m.other].area
                && h.get::<Actions>()?
                    .0
                    .get(id)
                    .is_some_and(|a| a.kind == "socialize" && a.until == Some(m.until)),
            "invalid social action",
        )?;
    }
    require(
        h.get::<Actions>()?
            .0
            .iter()
            .all(|(id, a)| a.kind != "socialize" || state.meetings.contains_key(id)),
        "orphan social action",
    )
}
