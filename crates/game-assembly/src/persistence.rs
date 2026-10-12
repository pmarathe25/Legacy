use crate::*;

const SAVE_VERSION: u32 = 47;
#[derive(Deserialize)]
struct SaveHeader {
    version: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    version: u32,
    state: CanonicalSnapshot,
}
impl Simulation {
    pub fn save(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&Save {
            version: SAVE_VERSION,
            state: self.canonical_snapshot()?,
        })
        .map_err(|e| Error::InvalidSave(e.to_string()))
    }
    pub fn load(bytes: &[u8], execution: Execution) -> Result<Self, Error> {
        let header: SaveHeader =
            serde_json::from_slice(bytes).map_err(|e| Error::InvalidSave(e.to_string()))?;
        if header.version != SAVE_VERSION {
            return Err(Error::InvalidSave("unsupported save version".into()));
        }
        let save: Save =
            serde_json::from_slice(bytes).map_err(|e| Error::InvalidSave(e.to_string()))?;
        let host = GameHost::restore(components::schema()?, save.state, |host| {
            validate(&CanonicalState::from_host(host))
        })?;
        let sim = Self::from_host(host, execution)?;
        sim.validate_ecology()?;
        sim.validate_enforcement()?;
        Ok(sim)
    }
    pub fn next_caller_sequence(&self) -> Result<u64, Error> {
        self.host
            .state
            .accepted
            .last_key_value()
            .map_or(Ok(1), |(seq, _)| seq.checked_add(1).ok_or(Error::Overflow))
    }
}

fn validate(s: &CanonicalState) -> Result<(), Error> {
    s.components.validate_land()?;
    genetics::validate(s)?;
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    game_cargo::validate_storage(s.components)?;
    if s.components.ids().any(|id| id.0 == 0 || id.0 >= s.next_id) {
        return Err(fail());
    }
    s.state.workshops.validate_storage(s.components)?;
    if let Some(openings) = &s.state.openings {
        openings.validate()?;
    }
    let playable = s.state.openings.as_ref().is_some_and(|definitions| {
        definitions
            .catalog
            .openings
            .contains_key(&s.state.scenario.id)
    });
    let enforcement_fixture = matches!(
        s.state.scenario.id.as_str(),
        "enforcement-fixture" | "realm-fixture"
    );
    let expanded_fixture = matches!(
        s.state.scenario.id.as_str(),
        "ecology-fixture" | "farm-enterprise-fixture"
    );
    if (playable
        && (s.state.realm.is_none() || s.state.armies.is_none() || s.state.agriculture.is_none()))
        || (s.state.scenario.id == "realm-fixture" && s.state.realm.is_none())
        || (s.state.scenario.id == "transport-fixture" && s.state.transport.is_none())
        || (s.state.scenario.id == "genetics-fixture" && s.state.genetics.is_none())
        || (!playable
            && !expanded_fixture
            && !enforcement_fixture
            && (!matches!(
                s.state.scenario.id.as_str(),
                "kernel-fixture"
                    | "survival-fixture"
                    | "autonomy-fixture"
                    | "life-fixture"
                    | "combat-fixture"
                    | "childhood-fixture"
                    | "farming-fixture"
                    | "agreement-fixture"
                    | "lineage-fixture"
                    | "property-fixture"
                    | "institution-fixture"
                    | "skills-fixture"
                    | "health-fixture"
                    | "genetics-fixture"
                    | "transport-fixture"
                    | "project-fixture"
                    | "invocation-fixture"
                    | "force-fixture"
                    | "mission-fixture"
                    | "army-fixture"
            ) || s.state.scenario.version != 1
                || !s.state.scenario.debug_fixture
                || (s.state.scenario.id != "kernel-fixture") != s.state.physical.is_some()
                || matches!(
                    s.state.scenario.id.as_str(),
                    "autonomy-fixture"
                        | "life-fixture"
                        | "combat-fixture"
                        | "childhood-fixture"
                        | "farming-fixture"
                        | "agreement-fixture"
                        | "lineage-fixture"
                        | "property-fixture"
                        | "institution-fixture"
                        | "skills-fixture"
                        | "health-fixture"
                        | "genetics-fixture"
                        | "transport-fixture"
                        | "project-fixture"
                        | "invocation-fixture"
                        | "force-fixture"
                        | "mission-fixture"
                        | "army-fixture"
                ) != s.state.decisions.is_some()
                || matches!(
                    s.state.scenario.id.as_str(),
                    "life-fixture"
                        | "combat-fixture"
                        | "childhood-fixture"
                        | "farming-fixture"
                        | "lineage-fixture"
                        | "property-fixture"
                        | "skills-fixture"
                        | "health-fixture"
                        | "genetics-fixture"
                        | "transport-fixture"
                        | "invocation-fixture"
                        | "force-fixture"
                        | "mission-fixture"
                ) != s.state.development.is_some()
                || matches!(
                    s.state.scenario.id.as_str(),
                    "combat-fixture"
                        | "lineage-fixture"
                        | "property-fixture"
                        | "skills-fixture"
                        | "health-fixture"
                        | "genetics-fixture"
                        | "transport-fixture"
                        | "invocation-fixture"
                        | "force-fixture"
                        | "mission-fixture"
                        | "army-fixture"
                ) != s.state.combat.is_some()
                || (s.state.scenario.id == "farming-fixture") != s.state.environment.is_some()
                || (s.state.scenario.id == "farming-fixture") != s.state.agriculture.is_some()
                || matches!(
                    s.state.scenario.id.as_str(),
                    "agreement-fixture"
                        | "property-fixture"
                        | "institution-fixture"
                        | "project-fixture"
                        | "mission-fixture"
                        | "army-fixture"
                ) != s.state.agreements.is_some()
                || matches!(
                    s.state.scenario.id.as_str(),
                    "lineage-fixture"
                        | "skills-fixture"
                        | "health-fixture"
                        | "genetics-fixture"
                        | "transport-fixture"
                        | "invocation-fixture"
                        | "force-fixture"
                ) != s.state.lineage.is_some()
                || (s.state.scenario.id == "property-fixture") != s.state.property.is_some()
                || (s.state.scenario.id == "institution-fixture")
                    != s.state.institutions.is_some()
                || (matches!(
                    s.state.scenario.id.as_str(),
                    "skills-fixture" | "health-fixture" | "genetics-fixture" | "invocation-fixture"
                ) && s.state.skills.is_none())
                || (s.state.scenario.id == "project-fixture") != s.state.projects.is_some()
                || (s.state.scenario.id == "invocation-fixture") != s.state.invocation.is_some()
                || (s.state.scenario.id == "force-fixture" && s.state.force.is_none())
                || (s.state.force.is_some() && s.state.combat.is_none())
                || (s.state.scenario.id == "mission-fixture" && s.state.missions.is_none())
                || (s.state.scenario.id == "army-fixture" && s.state.armies.is_none())
                || (s.state.armies.is_some() && s.state.missions.is_none())
                || (s.state.missions.is_some()
                    && (s.state.combat.is_none() || s.state.agreements.is_none()))))
        || ((playable || expanded_fixture || enforcement_fixture) && s.state.scenario.version != 1)
        || (playable
            && (s.state.scenario.debug_fixture
                || s.state.physical.is_none()
                || s.state.decisions.is_none()
                || s.state.development.is_none()
                || s.state.combat.is_none()
                || s.state.environment.is_none()
                || s.state.agreements.is_none()
                || s.state.lineage.is_none()
                || s.state.property.is_none()
                || s.state.institutions.is_none()
                || s.state.skills.is_none()
                || s.state.projects.is_none()
                || s.state.invocation.is_none()))
        || (enforcement_fixture
            && (!s.state.scenario.debug_fixture
                || s.state.physical.is_none()
                || s.state.decisions.is_none()
                || s.state.development.is_none()
                || s.state.combat.is_none()
                || s.state.agreements.is_none()
                || s.state.property.is_none()
                || s.state.institutions.is_none()
                || s.state.missions.is_none()
                || s.state.enforcement.is_none()))
        || (expanded_fixture
            && (!s.state.scenario.debug_fixture
                || s.state.physical.is_none()
                || s.state.decisions.is_none()
                || s.state.development.is_none()
                || s.state.combat.is_none()
                || s.state.environment.is_none()
                || s.state.ecology.is_none()
                || s.state.agriculture.is_none()))
        || (s.state.scenario.id == "farm-enterprise-fixture"
            && (s.state.projects.is_none() || s.state.agreements.is_none()))
    {
        return Err(fail());
    }
    let mut ids = BTreeSet::new();
    for id in s
        .state
        .areas
        .iter()
        .map(|a| a.id.0)
        .chain(s.state.people.iter().map(|p| p.id.0))
        .chain(
            game_cargo::inventories(s.components)
                .map(|(id, _)| id)
                .map(|i| i.0),
        )
    {
        if id == 0 || id >= s.next_id || !ids.insert(id) {
            return Err(fail());
        }
    }
    if s.state.health.is_some() {
        for (id, _) in game_health::shelters(s.components) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.enforcement {
        for id in world.cases.keys().map(|id| id.0).chain(
            world
                .cases
                .values()
                .filter_map(|case| case.judgment.as_ref().map(|judgment| judgment.id.0)),
        ) {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.ecology {
        for id in world
            .resources(s.components)
            .map(|(id, _)| id)
            .map(|id| id.0)
            .chain(world.plots(s.components).map(|(id, _)| id).map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if s.state.missions.is_some() {
        for id in game_missions::missions(s.components)
            .map(|(id, _)| id.0)
            .chain(game_missions::recruitments(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if s.state.physical.is_some() {
        for (id, _) in game_workshop::sites(s.components) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if s.state.invocation.is_some() {
        for id in game_invocation::warnings(s.components)
            .map(|(id, _)| id.0)
            .chain(game_invocation::records(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if s.state.projects.is_some() {
        for id in game_projects::projects(s.components)
            .map(|(id, _)| id.0)
            .chain(game_projects::enterprises(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if s.state.institutions.is_some() {
        for id in game_society::institutions(s.components)
            .map(|(id, _)| id.0)
            .chain(game_society::proposals(s.components).map(|(id, _)| id.0))
            .chain(game_society::cases(s.components).map(|(id, _)| id.0))
            .chain(game_society::concepts(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if s.state.property.is_some() {
        for id in game_property::claims(s.components)
            .map(|(id, _)| id.0)
            .chain(game_property::incidents(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.agreements {
        for id in world
            .agreements
            .keys()
            .map(|id| id.0)
            .chain(world.obligations.keys().map(|id| id.0))
            .chain(world.amendments.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.agriculture {
        for id in world.fields(s.components).map(|(id, _)| id) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.decisions {
        for id in world.concepts.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if s.state.development.is_some() {
        for (id, _) in game_lifecycle::all_pregnancies(s.components) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.armies {
        for army in world.kernel.armies.values() {
            for id in std::iter::once(army.id.0)
                .chain(army.orders.keys().map(|id| id.0))
                .chain(army.reports.keys().map(|id| id.0))
            {
                if id == 0 || id >= s.next_id || !ids.insert(id) {
                    return Err(fail());
                }
            }
        }
    }
    if let Some(world) = &s.state.realm {
        for id in world.kernel.commitments.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.state.combat {
        for id in world
            .maps
            .values()
            .chain(world.road_maps.values())
            .map(|m| m.id.0)
            .chain(world.encounters.keys().map(|e| e.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if !s.state.people.windows(2).all(|p| p[0].id < p[1].id)
        || !s.state.areas.windows(2).all(|a| a[0].id < a[1].id)
    {
        return Err(fail());
    }
    let people: BTreeMap<_, _> = s.state.people.iter().map(|p| (p.id, p)).collect();
    if game_spatial::locations(s.components)
        .map(|(id, location)| (game_core::PersonId(id.0), location))
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>()
        != people.keys().copied().collect()
    {
        return Err(fail());
    }
    game_spatial::resolve_locations(
        game_spatial::locations(s.components)
            .map(|(id, location)| (game_core::PersonId(id.0), location))
            .map(|(id, location)| (game_ecs::StableId(id.0), location)),
    )?;

    let areas: BTreeSet<_> = s.state.areas.iter().map(|a| a.id).collect();
    if !people
        .get(&s.state.ruler)
        .is_some_and(|p| p.role == Role::Ruler)
        || s.state
            .people
            .iter()
            .filter(|p| p.role == Role::Ruler)
            .count()
            != 1
    {
        return Err(fail());
    }
    for a in &s.state.areas {
        let mut destinations = BTreeSet::new();
        for l in &a.links {
            if l.duration.0 == 0
                || !areas.contains(&l.destination)
                || !destinations.insert(l.destination)
            {
                return Err(fail());
            }
        }
    }
    let mut scheduled_arrivals = BTreeSet::new();
    let mut reviews = Vec::new();
    let mut physical_deadlines = BTreeSet::new();
    let mut spoil_deadlines = BTreeSet::new();
    for (time, events) in &s.state.due {
        if *time <= s.time || events.is_empty() {
            return Err(fail());
        }
        for event in events {
            match event {
                Due::Arrive(id) => {
                    let p = people.get(id).ok_or_else(fail)?;
                    let deadline = s
                        .state
                        .transport
                        .as_ref()
                        .and_then(|_| game_transport::approach_job(s.components, *id))
                        .map(|approach| approach.completes);
                    if !matches!(*game_spatial::location(s.components,game_ecs::StableId((p.id).0)).expect("location exists"), Location::InTransit { arrives, .. } if deadline.unwrap_or(arrives) == *time)
                        || !scheduled_arrivals.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                Due::Review => reviews.push(*time),
                Due::CombatRound(_) => {
                    if s.state.combat.is_none() {
                        return Err(fail());
                    }
                }
                Due::Age(_) | Due::Birth(_) => {
                    if s.state.development.is_none() {
                        return Err(fail());
                    }
                }
                Due::Transfers(intents) => {
                    for i in intents {
                        if i.quantity.0 == 0
                            || i.source == i.destination
                            || !people.contains_key(&i.actor)
                            || !game_cargo::inventory(s.components, i.source).is_some()
                            || !game_cargo::inventory(s.components, i.destination).is_some()
                        {
                            return Err(fail());
                        }
                    }
                }
                Due::Physical(id) => {
                    let activity = s
                        .state
                        .physical
                        .as_ref()
                        .and_then(|_| s.components.activity(*id))
                        .ok_or_else(fail)?;
                    if !matches!(*activity, Activity::RealmTalk { completes, .. } | Activity::Lift { completes, .. } | Activity::Social { completes, .. } | Activity::Instruction { completes, .. } | Activity::Practice { completes, .. } | Activity::EnforcementTalk { completes, .. } | Activity::Gather { completes, .. } | Activity::ReportAgreement { completes, .. } | Activity::Escape { completes, .. } | Activity::Restrain { completes, .. } | Activity::Eat { completes, .. } | Activity::Load { completes, .. } | Activity::Tell { completes, .. } | Activity::Care { completes, .. } | Activity::Conceive { completes, .. } | Activity::Unbind { completes, .. } | Activity::Comfort { completes, .. } | Activity::Construct { completes, .. } | Activity::Lesson { completes, .. } | Activity::Take { completes, .. } | Activity::ReportIncident { completes, .. } | Activity::Cultivate { completes, .. } | Activity::Express { completes, .. } | Activity::Play { completes, .. } if completes == *time)
                        || !physical_deadlines.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                Due::Spoil(id) => {
                    if !game_cargo::inventory(s.components, *(id))
                        .is_some_and(|i| i.good == Good::Food && i.spoils_at == Some(*time))
                        || !spoil_deadlines.insert(*id)
                    {
                        return Err(fail());
                    }
                }
            }
        }
    }
    if reviews.len() != 1 || reviews[0].0 % 60_000 != 0 || reviews[0].0 - s.time.0 > 60_000 {
        return Err(fail());
    }
    for p in &s.state.people {
        if p.work_remainder >= 60_000
            || p.hunger_exempt != (p.role == Role::Guardian)
            || !game_cargo::inventory(s.components, p.inventory)
                .is_some_and(|i| i.location == InventoryLocation::Carried(p.id))
        {
            return Err(fail());
        }
        let mut origin = match *game_spatial::location(s.components, game_ecs::StableId((p.id).0))
            .expect("location exists")
        {
            Location::AtArea(a) => {
                if !p.route.is_empty() || !areas.contains(&a) {
                    return Err(fail());
                }
                a
            }
            Location::InTransit {
                from,
                to,
                departed,
                arrives,
                road_start,
            } => {
                let link = s
                    .state
                    .areas
                    .iter()
                    .find(|a| a.id == from)
                    .and_then(|a| a.links.iter().find(|l| l.destination == to))
                    .ok_or_else(fail)?;
                let dead = s.state.physical.as_ref().is_some_and(|_| {
                    game_physiology::body(s.components, p.id).is_some_and(|a| !a.vitals.alive)
                });
                if departed > s.time
                    || arrived_duration(departed, arrives)
                        != Some(match road_start {
                            None => link.duration.0,
                            Some(point) => {
                                let original = s
                                    .state
                                    .areas
                                    .iter()
                                    .find(|area| area.id == point.from)
                                    .and_then(|area| {
                                        area.links.iter().find(|edge| edge.destination == point.to)
                                    })
                                    .ok_or_else(fail)?;
                                if point.duration != original.duration {
                                    return Err(fail());
                                }
                                let leg =
                                    game_transport::retreat(point, to, link.duration, SimTime(0))
                                        .map_err(|_| fail())?;
                                if leg.from != from {
                                    return Err(fail());
                                }
                                leg.arrives.0
                            }
                        })
                    || (!dead && (arrives <= s.time || !scheduled_arrivals.contains(&p.id)))
                    || (dead && scheduled_arrivals.contains(&p.id))
                {
                    return Err(fail());
                }
                to
            }
            Location::StoppedOnRoad {
                from,
                to,
                progress,
                duration,
            } => {
                let link = s
                    .state
                    .areas
                    .iter()
                    .find(|area| area.id == from)
                    .and_then(|area| area.links.iter().find(|link| link.destination == to))
                    .ok_or_else(fail)?;
                if duration != link.duration
                    || progress > duration
                    || !p.route.is_empty()
                    || scheduled_arrivals.contains(&p.id)
                {
                    return Err(fail());
                }
                from
            }
            Location::Carried { carrier } => {
                if !people.contains_key(&carrier)
                    || carrier == p.id
                    || !p.route.is_empty()
                    || scheduled_arrivals.contains(&p.id)
                    || !s
                        .state
                        .transport
                        .as_ref()
                        .and_then(|_| game_transport::group(s.components, p.id))
                        .is_some_and(|group| group.anchor == carrier)
                {
                    return Err(fail());
                }
                // No independent route remains on a body being physically borne.
                s.state.areas.first().ok_or_else(fail)?.id
            }
        };
        for destination in &p.route {
            if !s
                .state
                .areas
                .iter()
                .any(|a| a.id == origin && a.links.iter().any(|l| l.destination == *destination))
            {
                return Err(fail());
            }
            origin = *destination;
        }
    }
    for (id, inventory) in game_cargo::inventories(s.components) {
        if id != inventory.id
            || inventory.quantity > inventory.capacity
            || inventory.access.iter().any(|p| !people.contains_key(p))
            || inventory
                .last_granted
                .is_some_and(|p| !people.contains_key(&p))
            || inventory.spoils_at.is_some() != spoil_deadlines.contains(&id)
        {
            return Err(fail());
        }
        match inventory.location {
            InventoryLocation::Area(a) if !areas.contains(&a) => return Err(fail()),
            InventoryLocation::Carried(p) if !people.contains_key(&p) => return Err(fail()),
            _ => {}
        }
    }
    for (i, event) in s.events.iter().enumerate() {
        if event.sequence != i as u64 + 1
            || event.at > s.time
            || (i > 0 && event.at < s.events[i - 1].at)
        {
            return Err(fail());
        }
        match event.event {
            EventKind::RealmProposed {
                commitment,
                actor,
                revision,
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .realm
                        .as_ref()
                        .and_then(|w| w.kernel.commitments.get(&commitment))
                        .is_some_and(|c| c.revision(revision).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::RealmReported {
                commitment,
                sender,
                recipient,
            } => {
                if !people.contains_key(&sender)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .realm
                        .as_ref()
                        .is_some_and(|w| w.kernel.commitments.contains_key(&commitment))
                {
                    return Err(fail());
                }
            }
            EventKind::RealmWorkRequested { key, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .realm
                        .as_ref()
                        .and_then(|w| w.kernel.commitments.get(&key.commitment))
                        .and_then(|c| c.revision(key.revision))
                        .is_some_and(|r| usize::from(key.index) < r.proposal.terms.len())
                {
                    return Err(fail());
                }
            }
            EventKind::RealmRatified {
                commitment,
                revision,
                institution,
                proposal,
                ..
            } => {
                if !s
                    .state
                    .realm
                    .as_ref()
                    .and_then(|w| w.kernel.commitments.get(&commitment))
                    .is_some_and(|c| c.revision(revision).is_some())
                    || !s.state.institutions.as_ref().is_some_and(|_| {
                        game_society::proposal(s.components, proposal)
                            .is_some_and(|p| p.institution == institution)
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::Socialized { first, second, .. }
            | EventKind::InstructionCompleted {
                teacher: first,
                student: second,
                ..
            }
            | EventKind::SocialRefused {
                actor: first,
                recipient: second,
                ..
            } => {
                if !people.contains_key(&first) || !people.contains_key(&second) {
                    return Err(fail());
                }
            }
            EventKind::PracticeCompleted { actor, .. } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
            EventKind::EnforcementPolicyChanged { institution, .. } => {
                if !s
                    .state
                    .institutions
                    .as_ref()
                    .is_some_and(|_| game_society::institution(s.components, institution).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementFiling {
                case,
                actor,
                recipient,
            }
            | EventKind::EnforcementReported {
                case,
                sender: actor,
                recipient,
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementFiled {
                case,
                actor,
                institution,
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                    || !s.state.institutions.as_ref().is_some_and(|_| {
                        game_society::institution(s.components, institution).is_some()
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementDecided {
                case,
                judgment,
                proposer,
            } => {
                if !people.contains_key(&proposer)
                    || !s.state.enforcement.as_ref().is_some_and(|w| {
                        w.cases
                            .get(&case)
                            .is_some_and(|c| c.judgment.as_ref().is_some_and(|j| j.id == judgment))
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementResponded { case, actor, .. } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementDispatched {
                case,
                actor,
                mission,
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                    || !s
                        .state
                        .missions
                        .as_ref()
                        .is_some_and(|_| game_missions::mission(s.components, mission).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementOutcome { case, .. } => {
                if !s
                    .state
                    .enforcement
                    .as_ref()
                    .is_some_and(|w| w.cases.contains_key(&case))
                {
                    return Err(fail());
                }
            }
            EventKind::Drank { actor, source, .. } | EventKind::WaterPurified { actor, source } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, source)
                        .is_some_and(|i| i.good == Good::Water)
                {
                    return Err(fail());
                }
            }
            EventKind::ShelterBuilt {
                shelter,
                builder: actor,
            }
            | EventKind::ShelterEntered { shelter, actor }
            | EventKind::ShelterLeft { shelter, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .health
                        .as_ref()
                        .is_some_and(|_| game_health::shelter(s.components, shelter).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ShelterAccessChanged {
                shelter,
                actor,
                person,
                ..
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&person)
                    || !s
                        .state
                        .health
                        .as_ref()
                        .is_some_and(|_| game_health::shelter(s.components, shelter).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::TreatmentOffered { caregiver, patient }
            | EventKind::TreatmentAccepted { caregiver, patient }
            | EventKind::TreatmentRefused { caregiver, patient }
            | EventKind::TreatmentCompleted { caregiver, patient } => {
                if !people.contains_key(&caregiver) || !people.contains_key(&patient) {
                    return Err(fail());
                }
            }
            EventKind::IllnessStarted { actor, .. } | EventKind::IllnessRecovered { actor, .. } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
            EventKind::ObligationAccrued {
                obligation, actor, ..
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.obligations.contains_key(&obligation))
                {
                    return Err(fail());
                }
            }
            EventKind::ResourceAccessChanged {
                resource,
                actor,
                recipient,
                ..
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resource(s.components, resource).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ResourceAccessRequested {
                resource,
                actor,
                custodian,
            }
            | EventKind::ResourceAccessRefused {
                resource,
                actor,
                custodian,
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&custodian)
                    || !s
                        .state
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resource(s.components, resource).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::GatheringStarted {
                resource, actor, ..
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resource(s.components, resource).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ResourceCollected {
                resource,
                actor,
                destination,
                ..
            } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, destination).is_some()
                    || !s
                        .state
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resource(s.components, resource).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MissionProposed { mission, sponsor } => {
                if !people.contains_key(&sponsor)
                    || !s
                        .state
                        .missions
                        .as_ref()
                        .is_some_and(|_| game_missions::mission(s.components, mission).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MissionRecruitmentChanged {
                mission, person, ..
            }
            | EventKind::MissionObjectiveChanged {
                mission,
                leader: person,
            } => {
                if !people.contains_key(&person)
                    || !s
                        .state
                        .missions
                        .as_ref()
                        .is_some_and(|_| game_missions::mission(s.components, mission).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MissionReported {
                mission,
                speaker,
                recipient,
            } => {
                if !people.contains_key(&speaker)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .missions
                        .as_ref()
                        .is_some_and(|_| game_missions::mission(s.components, mission).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MissionDeparted { mission } | EventKind::MissionFinished { mission, .. } => {
                if !s
                    .state
                    .missions
                    .as_ref()
                    .is_some_and(|_| game_missions::mission(s.components, mission).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::AgreementReported {
                sender,
                recipient,
                agreement,
            } => {
                if !people.contains_key(&sender)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.agreements.contains_key(&agreement))
                {
                    return Err(fail());
                }
            }
            EventKind::GuardianAccepted {
                guardian: actor,
                requester: captor,
            }
            | EventKind::Yielded { actor, captor } => {
                if !people.contains_key(&actor) || !people.contains_key(&captor) {
                    return Err(fail());
                }
            }
            EventKind::YieldWithdrawn { actor }
            | EventKind::EscapeAttempted { actor }
            | EventKind::EscapeInterrupted { actor } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
            EventKind::Restrained {
                actor,
                target,
                binding,
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&target)
                    || !game_cargo::inventory(s.components, binding).is_some()
                {
                    return Err(fail());
                }
            }
            EventKind::CombatReinforcement {
                encounter, actor, ..
            } => {
                if !people.contains_key(&actor)
                    || !s.state.combat.as_ref().is_some_and(|w| {
                        w.encounters
                            .get(&encounter)
                            .is_some_and(|e| e.fighters.contains_key(&actor))
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::EncounterStarted { encounter, area } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                    || !areas.contains(&area)
                {
                    return Err(fail());
                }
            }
            EventKind::CombatMoved {
                encounter, actor, ..
            }
            | EventKind::Surrendered { encounter, actor } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                    || !people.contains_key(&actor)
                {
                    return Err(fail());
                }
            }
            EventKind::Struck {
                encounter,
                actor,
                target,
                ..
            } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                    || !people.contains_key(&actor)
                    || !people.contains_key(&target)
                {
                    return Err(fail());
                }
            }
            EventKind::Captured {
                encounter,
                actor,
                target,
                binding,
            } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                    || !people.contains_key(&actor)
                    || !people.contains_key(&target)
                    || !game_cargo::inventory(s.components, binding).is_some()
                {
                    return Err(fail());
                }
            }
            EventKind::Released { actor, target } => {
                if !people.contains_key(&actor) || !people.contains_key(&target) {
                    return Err(fail());
                }
            }
            EventKind::ObstacleDamaged { map, obstacle, .. } => {
                if !s.state.combat.as_ref().is_some_and(|w| {
                    w.map(map)
                        .is_some_and(|m| m.obstacles.iter().any(|o| o.index == obstacle))
                }) {
                    return Err(fail());
                }
            }
            EventKind::RoundEnded { encounter, .. } | EventKind::EncounterEnded { encounter } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                {
                    return Err(fail());
                }
            }

            EventKind::Departed {
                actor,
                from,
                to,
                arrives,
            } => {
                if !people.contains_key(&actor)
                    || !areas.contains(&from)
                    || !areas.contains(&to)
                    || arrives <= event.at
                {
                    return Err(fail());
                }
            }
            EventKind::Arrived { actor, area } => {
                if !people.contains_key(&actor) || !areas.contains(&area) {
                    return Err(fail());
                }
            }
            EventKind::Transferred {
                actor,
                source,
                destination,
                quantity,
            } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, source).is_some()
                    || !game_cargo::inventory(s.components, destination).is_some()
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::TransferFailed { actor, source } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, source).is_some()
                {
                    return Err(fail());
                }
            }
            EventKind::Consumed {
                actor,
                source,
                quantity,
            }
            | EventKind::Ate {
                actor,
                source,
                quantity,
            } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, source).is_some()
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::Spoiled { inventory, .. } => {
                if !game_cargo::inventory(s.components, inventory).is_some() {
                    return Err(fail());
                }
            }
            EventKind::Produced {
                actor,
                site,
                output,
                quantity,
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .physical
                        .as_ref()
                        .is_some_and(|_| game_workshop::site(s.components, site).is_some())
                    || !game_cargo::inventory(s.components, output).is_some()
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::Died { actor }
            | EventKind::Interrupted { actor }
            | EventKind::Recovered { actor } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
            EventKind::Reported {
                sender,
                recipient,
                inventory,
            } => {
                if !people.contains_key(&sender)
                    || !people.contains_key(&recipient)
                    || !game_cargo::inventory(s.components, inventory).is_some()
                {
                    return Err(fail());
                }
            }
            EventKind::ConceptionRefused { actor, partner } => {
                if !people.contains_key(&actor) || !people.contains_key(&partner) {
                    return Err(fail());
                }
            }
            EventKind::Conceived { pregnancy, parents } => {
                if !s
                    .state
                    .development
                    .as_ref()
                    .is_some_and(|_| game_lifecycle::pregnancy(s.components, pregnancy).is_some())
                    || parents.iter().any(|p| !people.contains_key(p))
                {
                    return Err(fail());
                }
            }
            EventKind::PregnancyEnded { pregnancy } => {
                if !s
                    .state
                    .development
                    .as_ref()
                    .is_some_and(|_| game_lifecycle::pregnancy(s.components, pregnancy).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::Born {
                pregnancy,
                child,
                parents,
            } => {
                if !s
                    .state
                    .development
                    .as_ref()
                    .is_some_and(|_| game_lifecycle::pregnancy(s.components, pregnancy).is_some())
                    || !people.contains_key(&child)
                    || parents.iter().any(|p| !people.contains_key(p))
                {
                    return Err(fail());
                }
            }
            EventKind::Cared {
                caregiver,
                child,
                source,
                quantity,
            } => {
                if !people.contains_key(&caregiver)
                    || !people.contains_key(&child)
                    || !game_cargo::inventory(s.components, source).is_some()
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::InvocationStarted { actor, ref lives } => {
                if !people.contains_key(&actor)
                    || !s.state.invocation.as_ref().is_some_and(|_| {
                        lives.iter().all(|id| {
                            game_invocation::archive::<game_api::Command>(s.components, *id)
                                .is_some()
                        })
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::InvocationDismissed { actor }
            | EventKind::InvocationCostApplied { actor, .. }
            | EventKind::InvocationControlChanged { actor, .. } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
            EventKind::InvocationWarningRaised { warning, actor }
            | EventKind::InvocationCostAccepted { warning, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .invocation
                        .as_ref()
                        .is_some_and(|_| game_invocation::warning(s.components, warning).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MemoryRecorded {
                record,
                actor,
                inventory,
            } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, inventory).is_some()
                    || !s
                        .state
                        .invocation
                        .as_ref()
                        .is_some_and(|_| game_invocation::record(s.components, record).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MemoryRead { record, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .invocation
                        .as_ref()
                        .is_some_and(|_| game_invocation::record(s.components, record).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::MemoryTaught {
                actor,
                recipient,
                life,
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&recipient)
                    || !s.state.invocation.as_ref().is_some_and(|_| {
                        game_invocation::archive::<game_api::Command>(s.components, life).is_some()
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectProposed { project, sponsor } => {
                if !people.contains_key(&sponsor)
                    || !s
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|_| game_projects::project(s.components, project).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectSupported {
                project,
                actor,
                agreement,
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|_| game_projects::project(s.components, project).is_some())
                    || !s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.agreements.contains_key(&agreement))
                {
                    return Err(fail());
                }
            }
            EventKind::ConstructionStarted { project, actor }
            | EventKind::ProjectCancelled { project, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|_| game_projects::project(s.components, project).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectFailed { project } => {
                if !s
                    .state
                    .projects
                    .as_ref()
                    .is_some_and(|_| game_projects::project(s.components, project).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectCompleted {
                project,
                enterprise,
                site,
            } => {
                let valid = s.state.projects.as_ref().is_some_and(|_| {
                    let Some(plan) = game_projects::project(s.components, project) else {
                        return false;
                    };
                    game_projects::enterprise(s.components, enterprise).is_some_and(|record| {
                        record.project == project
                            && record.site == site
                            && match plan.kind {
                                ProjectKind::Workshop { .. } => {
                                    s.state.physical.as_ref().is_some_and(|_| {
                                        game_workshop::site(s.components, site).is_some()
                                    })
                                }
                                ProjectKind::Farm { plot, water } => {
                                    site == plot
                                        && s.state
                                            .agriculture
                                            .as_ref()
                                            .and_then(|world| world.field(s.components, site))
                                            .is_some_and(|field| {
                                                field.id == plot
                                                    && field.area == plan.area
                                                    && field.seed == plan.input
                                                    && field.tool == plan.tool
                                                    && field.water == water
                                                    && field.food == plan.output
                                            })
                                }
                            }
                    })
                });
                if !valid {
                    return Err(fail());
                }
            }
            EventKind::EnterpriseInputUsed {
                enterprise, site, ..
            } => {
                if !s.state.projects.as_ref().is_some_and(|_| {
                    game_projects::enterprise(s.components, enterprise)
                        .is_some_and(|e| e.site == site)
                }) {
                    return Err(fail());
                }
            }
            EventKind::InstitutionProposed {
                institution,
                proposer,
            } => {
                if !people.contains_key(&proposer)
                    || !s.state.institutions.as_ref().is_some_and(|_| {
                        game_society::institution(s.components, institution).is_some()
                    })
                {
                    return Err(fail());
                }
            }
            EventKind::InstitutionFounded { institution }
            | EventKind::InstitutionLeft { institution, .. }
            | EventKind::OfficeSupportChanged { institution, .. }
            | EventKind::InstitutionRuleApplied { institution, .. } => {
                if !s
                    .state
                    .institutions
                    .as_ref()
                    .is_some_and(|_| game_society::institution(s.components, institution).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::InstitutionChanged {
                institution,
                proposal,
            } => {
                if !s.state.institutions.as_ref().is_some_and(|_| {
                    game_society::institution(s.components, institution).is_some()
                        && game_society::proposal(s.components, proposal)
                            .is_some_and(|p| p.institution == institution)
                }) {
                    return Err(fail());
                }
            }
            EventKind::SocialCaseRecorded { case, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .institutions
                        .as_ref()
                        .is_some_and(|_| game_society::case(s.components, case).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::LessonCompleted {
                teacher,
                student,
                gain,
                ..
            } => {
                if !people.contains_key(&teacher) || !people.contains_key(&student) || gain > 1_000
                {
                    return Err(fail());
                }
            }
            EventKind::GuardianRefused {
                guardian,
                requester,
                ..
            } => {
                if !people.contains_key(&guardian) || !people.contains_key(&requester) {
                    return Err(fail());
                }
            }
            EventKind::PropertyClaimed {
                claim,
                actor,
                inventory,
            } => {
                if !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, inventory).is_some()
                    || !s
                        .state
                        .property
                        .as_ref()
                        .is_some_and(|_| game_property::claim(s.components, claim).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::ClaimRecognized { claim, actor, .. } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .property
                        .as_ref()
                        .is_some_and(|_| game_property::claim(s.components, claim).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::PermissionShared {
                actor,
                recipient,
                inventory,
                ..
            }
            | EventKind::AccessRequested {
                actor,
                custodian: recipient,
                inventory,
            }
            | EventKind::AccessRefused {
                actor,
                custodian: recipient,
                inventory,
            } => {
                if !people.contains_key(&actor)
                    || !people.contains_key(&recipient)
                    || !game_cargo::inventory(s.components, inventory).is_some()
                {
                    return Err(fail());
                }
            }
            EventKind::TakingAttempted {
                incident,
                actor,
                source,
                destination,
                quantity,
            }
            | EventKind::Taken {
                incident,
                actor,
                source,
                destination,
                quantity,
            } => {
                if quantity.0 == 0
                    || !people.contains_key(&actor)
                    || !game_cargo::inventory(s.components, source).is_some()
                    || !game_cargo::inventory(s.components, destination).is_some()
                    || !s
                        .state
                        .property
                        .as_ref()
                        .is_some_and(|_| game_property::incident(s.components, incident).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::TakingInterrupted { incident, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .property
                        .as_ref()
                        .is_some_and(|_| game_property::incident(s.components, incident).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::IncidentReported {
                incident,
                sender,
                recipient,
            } => {
                if !people.contains_key(&sender)
                    || !people.contains_key(&recipient)
                    || !s
                        .state
                        .property
                        .as_ref()
                        .is_some_and(|_| game_property::incident(s.components, incident).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::AgreementProposed {
                agreement,
                proposer,
            } => {
                if !people.contains_key(&proposer)
                    || !s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.agreements.contains_key(&agreement))
                {
                    return Err(fail());
                }
            }
            EventKind::AgreementActivated { agreement }
            | EventKind::AgreementRejected { agreement, .. } => {
                if !s
                    .state
                    .agreements
                    .as_ref()
                    .is_some_and(|w| w.agreements.contains_key(&agreement))
                {
                    return Err(fail());
                }
            }
            EventKind::AgreementAmended {
                agreement,
                amendment,
            } => {
                if !s.state.agreements.as_ref().is_some_and(|w| {
                    w.agreements.contains_key(&agreement)
                        && w.amendments
                            .get(&amendment)
                            .is_some_and(|a| a.agreement == agreement)
                }) {
                    return Err(fail());
                }
            }
            EventKind::ObligationPerformed {
                obligation,
                actor,
                quantity,
            } => {
                if !people.contains_key(&actor)
                    || quantity.0 == 0
                    || !s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.obligations.contains_key(&obligation))
                {
                    return Err(fail());
                }
            }
            EventKind::ObligationDefaulted { obligation } => {
                if !s
                    .state
                    .agreements
                    .as_ref()
                    .is_some_and(|w| w.obligations.contains_key(&obligation))
                {
                    return Err(fail());
                }
            }
            EventKind::Cultivated { actor, field, .. }
            | EventKind::CropHarvested { actor, field, .. } => {
                if !people.contains_key(&actor)
                    || !s
                        .state
                        .agriculture
                        .as_ref()
                        .is_some_and(|w| w.field(s.components, field).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::CropFailed { field } => {
                if !s
                    .state
                    .agriculture
                    .as_ref()
                    .is_some_and(|w| w.field(s.components, field).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::Comforted { caregiver, child } => {
                if !people.contains_key(&caregiver) || !people.contains_key(&child) {
                    return Err(fail());
                }
            }
            EventKind::Developed { actor, .. }
            | EventKind::Expressed { actor, .. }
            | EventKind::Practiced { actor, .. } => {
                if !people.contains_key(&actor) {
                    return Err(fail());
                }
            }
        }
    }
    if s.next_event != s.events.len() as u64 + 1
        || s.state.next_command != s.state.accepted.len() as u64 + 1
    {
        return Err(fail());
    }
    let mut sequences = BTreeSet::new();
    for (caller, (_, receipt)) in &s.state.accepted {
        if *caller != receipt.caller_sequence
            || receipt.accepted_at > s.time
            || receipt.authoritative_sequence == 0
            || receipt.authoritative_sequence >= s.state.next_command
            || !sequences.insert(receipt.authoritative_sequence)
        {
            return Err(fail());
        }
    }
    if let Some(world) = &s.state.physical {
        validate_physical(s, world, &people, &areas, &physical_deadlines)?;
    } else if game_physiology::bodies(s.components).next().is_some()
        || s.components.activities().next().is_some()
        || game_workshop::jobs(s.components).next().is_some()
        || game_workshop::sites(s.components).next().is_some()
    {
        return Err(fail());
    }
    if let Some(world) = &s.state.decisions {
        if game_characters::decisions::minds::<game_api::Command>(s.components)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != people.keys().copied().collect::<BTreeSet<_>>()
            || !world.concepts.contains_key(&world.work_concept)
            || world.last_learning_event >= s.next_event
            || world.switching_margin < 0
        {
            return Err(fail());
        }
        for (id, mind) in game_characters::decisions::minds::<game_api::Command>(s.components) {
            let id = &id;
            if !people.contains_key(id)
                || mind.interests.values().any(|v| *v > 1_000)
                || mind
                    .plan
                    .as_ref()
                    .is_some_and(|plan| plan.shape != decisions::action_shape(&plan.action))
                || [
                    mind.personality.discipline,
                    mind.personality.ambition,
                    mind.personality.sociability,
                    mind.personality.risk_tolerance,
                    mind.personality.hedonism,
                    mind.personality.empathy,
                    mind.personality.aggression,
                    mind.personality.honesty,
                    mind.personality.patience,
                ]
                .iter()
                .any(|v| *v > 1_000)
            {
                return Err(fail());
            }
            if let Some(plan) = &mind.plan {
                use game_characters::decisions::{CommitmentRef, IntentionKey, Motive, Subject};
                let commitment_exists = |commitment: CommitmentRef| match commitment {
                    CommitmentRef::Obligation(id) => s
                        .state
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.obligations.contains_key(&id)),
                    CommitmentRef::Project(id) => s
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|_| game_projects::project(s.components, id).is_some()),
                };
                let subject_exists = |subject: Subject| match subject {
                    Subject::Inventory(id) => game_cargo::inventory(s.components, id).is_some(),
                    Subject::Site(id) => s
                        .state
                        .physical
                        .as_ref()
                        .is_some_and(|_| game_workshop::site(s.components, id).is_some()),
                    Subject::Project(id) => s
                        .state
                        .projects
                        .as_ref()
                        .is_some_and(|_| game_projects::project(s.components, id).is_some()),
                    Subject::Resource(id) => s
                        .state
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resource(s.components, id).is_some()),
                };
                let key_valid = match plan.key {
                    IntentionKey::Relief { beneficiary, .. } => people.contains_key(&beneficiary),
                    IntentionKey::Development {
                        beneficiary,
                        subject,
                        ..
                    } => people.contains_key(&beneficiary) && subject.is_none_or(subject_exists),
                    IntentionKey::Provision {
                        good,
                        beneficiary,
                        source,
                    } => {
                        people.contains_key(&beneficiary)
                            && source.is_none_or(|id| {
                                game_cargo::inventory(s.components, id)
                                    .is_some_and(|stock| stock.good == good)
                            })
                    }
                    IntentionKey::Commitment(commitment) => commitment_exists(commitment),
                };
                let motives_valid = plan.motives.iter().all(|motive| match motive {
                    Motive::Relieve { beneficiary, .. } | Motive::Develop { beneficiary, .. } => {
                        people.contains_key(beneficiary)
                    }
                    Motive::Provide {
                        requirement,
                        beneficiary,
                        ..
                    } => {
                        people.contains_key(beneficiary)
                            && requirement.quantity.0 > 0
                            && requirement.source.is_none_or(|id| {
                                game_cargo::inventory(s.components, id)
                                    .is_some_and(|stock| stock.good == requirement.good)
                            })
                    }
                    Motive::Fulfil { commitment } => commitment_exists(*commitment),
                });
                if !key_valid
                    || !motives_valid
                    || plan
                        .prospects
                        .source
                        .is_some_and(|id| !game_cargo::inventory(s.components, id).is_some())
                    || plan.prospects.site.is_some_and(|id| {
                        !s.state
                            .physical
                            .as_ref()
                            .is_some_and(|_| game_workshop::site(s.components, id).is_some())
                    })
                    || plan
                        .prospects
                        .sponsor
                        .is_some_and(|id| !people.contains_key(&id))
                {
                    return Err(fail());
                }
            }
            if let Some(plan) = &mind.plan
                && let Some(delivery) = &plan.delivery
            {
                let target_exists = match delivery.destination {
                    game_logistics::Destination::Inventory(target) => {
                        game_cargo::inventory(s.components, target).is_some()
                    }
                    game_logistics::Destination::Contact(target) => people.contains_key(&target),
                };
                let matches_motive = plan.motives.iter().all(|motive| match motive {
                    game_characters::decisions::Motive::Provide {
                        requirement,
                        beneficiary,
                        ..
                    } => {
                        delivery.destination == game_logistics::Destination::Contact(*beneficiary)
                            && delivery.requirement.good == requirement.good
                            && delivery.requirement.source == requirement.source
                    }
                    _ => true,
                });
                if !delivery.valid()
                    || !target_exists
                    || !matches_motive
                    || delivery.last_receipt >= s.next_event
                    || delivery.requirement.source.is_some_and(|source| {
                        !game_cargo::inventory(s.components, source)
                            .is_some_and(|stock| stock.good == delivery.requirement.good)
                    })
                {
                    return Err(fail());
                }
            }
            for (from, links) in &mind.routes {
                if !areas.contains(from)
                    || links
                        .iter()
                        .any(|(to, duration)| !areas.contains(to) || duration.0 == 0)
                {
                    return Err(fail());
                }
            }
            if mind.last_failure.is_some_and(|(time, inventory)| {
                time > s.time || !game_cargo::inventory(s.components, inventory).is_some()
            }) {
                return Err(fail());
            }
            for (id, fact) in &mind.knowledge {
                if *id != fact.inventory
                    || !game_cargo::inventory(s.components, *(id)).is_some()
                    || !areas.contains(&fact.area)
                    || fact.observed_at > s.time
                    || fact.learned_at > s.time
                {
                    return Err(fail());
                }
            }
            for (child, signal) in &mind.child_signals {
                if !people.contains_key(child)
                    || !match signal.location {
                        Location::AtArea(area) => areas.contains(&area),
                        Location::StoppedOnRoad {
                            from,
                            to,
                            progress,
                            duration,
                        } => {
                            areas.contains(&from)
                                && areas.contains(&to)
                                && from != to
                                && duration.0 > 0
                                && progress <= duration
                        }
                        Location::InTransit {
                            from,
                            to,
                            departed,
                            arrives,
                            ..
                        } => {
                            areas.contains(&from)
                                && areas.contains(&to)
                                && from != to
                                && departed <= signal.observed_at
                                && signal.observed_at <= arrives
                                && departed < arrives
                        }
                        Location::Carried { .. } => false,
                    }
                    || signal.observed_at > s.time
                    || ![0, 3_000, 20_000].contains(&signal.urgency)
                    || ![0, 3_000, 20_000].contains(&signal.hydration_urgency)
                {
                    return Err(fail());
                }
            }
            for id in mind.relationships.keys() {
                if !people.contains_key(id) {
                    return Err(fail());
                }
            }
            for (id, value) in &mind.values {
                if !world.concepts.contains_key(id) || !(0..=1_000).contains(&value.weight) {
                    return Err(fail());
                }
            }
            for (id, site) in &mind.sites {
                if *id != site.site
                    || !game_workshop::site(s.components, *id).is_some()
                    || !areas.contains(&site.area)
                    || !game_cargo::inventory(s.components, site.input).is_some()
                    || !game_cargo::inventory(s.components, site.tool).is_some()
                {
                    return Err(fail());
                }
            }
        }
    }
    agreements::validate(s)?;
    development::validate(s)?;
    combat::validate(s)?;
    transport::validate(s)?;
    lineage::validate(s)?;
    property::validate(s)?;
    institutions::validate(s)?;
    skills::validate(s)?;
    projects::validate(s)?;
    invocation::validate(s)?;
    force::validate(s)?;
    health::validate(s)?;
    social::validate(s)?;
    missions::validate(s)?;
    Ok(())
}

fn validate_physical(
    s: &CanonicalState,
    world: &physical::World,
    people: &BTreeMap<PersonId, &Person>,
    areas: &BTreeSet<AreaId>,
    deadlines: &BTreeSet<PersonId>,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent physical state".into());
    let rules = &world.rules;
    if game_physiology::bodies(s.components).count() != people.len()
        || s.components
            .activities()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            != people.keys().copied().collect()
        || game_workshop::jobs(s.components).any(|(id, _)| !people.contains_key(&id))
        || rules.hunger_harm == 0
        || rules.fatigue_limit == 0
        || rules.max_health == 0
        || rules.meal_relief == 0
        || rules.meal_size.0 == 0
        || rules.eating_time.0 == 0
        || rules.loading_time.0 == 0
    {
        return Err(fail());
    }
    for (id, site) in game_workshop::sites(s.components) {
        let id = &id;
        if *id != site.id
            || !areas.contains(&site.area)
            || s.state.workshops.resolve_site(site).is_err()
            || site.access.iter().any(|p| !people.contains_key(p))
        {
            return Err(fail());
        }
        for inventory in [site.input, site.tool, site.output] {
            if !game_cargo::inventory(s.components, inventory)
                .is_some_and(|i| i.location == InventoryLocation::Area(site.area))
            {
                return Err(fail());
            }
        }
        if game_cargo::inventory(s.components, site.tool)
            .expect("inventory exists")
            .good
            != Good::Tool
        {
            return Err(fail());
        }
    }
    let mut cargo = BTreeMap::<PersonId, u128>::new();
    for i in game_cargo::inventories(s.components).map(|(_, row)| row) {
        if let InventoryLocation::Carried(p) = i.location {
            *cargo.entry(p).or_default() += u128::from(i.quantity.0);
        }
    }
    let locations = crate::transport::resolved_locations_for(s)?;
    let mut care_targets = BTreeSet::new();
    for (id, actor) in game_physiology::bodies(s.components) {
        let id = &id;
        let activity = s.components.activity(*id).ok_or_else(fail)?;
        let job = game_workshop::job(s.components, *id);
        let p = people.get(id).ok_or_else(fail)?;
        let carried = cargo.get(id).copied().unwrap_or(0);
        if actor.health_remainder >= 1_000
            || actor.fatigue_remainder >= 1_000
            || actor.vitals.health > physical::rules_for(s, *id).max_health
            || actor.vitals.fatigue > rules.fatigue_limit
            || (carried > u128::from(physical::rules_for(s, *id).carrying_capacity.0)
                && matches!(
                    *game_spatial::location(s.components, game_ecs::StableId((p.id).0))
                        .expect("location exists"),
                    Location::InTransit { .. }
                ))
            || (p.hunger_exempt && (actor.vitals.hunger != 0 || !actor.vitals.alive))
            || (!actor.vitals.alive && actor.vitals.health != 0)
        {
            return Err(fail());
        }
        if !actor.vitals.alive && (*activity != Activity::Idle || job.is_some()) {
            return Err(fail());
        }
        if let Some(job) = job
            && (job.tool.is_some_and(|tool| {
                !game_cargo::inventory(s.components, tool).is_some_and(|i| i.good == Good::Tool)
            }) || !matches!(job.rate.denominator, 1_000 | 1_000_000)
                || job.body_rate.denominator != 1_000
                || !(250..=2_000).contains(&job.body_rate.numerator)
                || {
                    let numerator = u128::from(job.rate.numerator)
                        * u128::from(job.body_rate.denominator)
                        * 1_000;
                    let denominator =
                        u128::from(job.rate.denominator) * u128::from(job.body_rate.numerator);
                    let maximum = s.state.skills.as_ref().map_or(1_000, |skills| {
                        1_000
                            + u64::from(skills.level(s.components, *id, Skill::Craft))
                                * u64::from(skills.rules.work_bonus_per_level)
                    });
                    denominator == 0
                        || numerator % denominator != 0
                        || numerator / denominator < 1_000
                        || numerator / denominator > u128::from(maximum)
                }
                || job.work_remainder >= job.rate.denominator
                || !game_workshop::site(s.components, job.site).is_some_and(|site| {
                    s.state
                        .workshops
                        .resolve_site(site)
                        .is_ok_and(|view| job.remaining <= view.recipe.duration.0)
                }))
        {
            return Err(fail());
        }
        let timed = matches!(
            *activity,
            Activity::RealmTalk { .. }
                | Activity::Lift { .. }
                | Activity::Social { .. }
                | Activity::Instruction { .. }
                | Activity::Practice { .. }
                | Activity::EnforcementTalk { .. }
                | Activity::Gather { .. }
                | Activity::ReportAgreement { .. }
                | Activity::Escape { .. }
                | Activity::Restrain { .. }
                | Activity::Eat { .. }
                | Activity::Load { .. }
                | Activity::Tell { .. }
                | Activity::Care { .. }
                | Activity::Conceive { .. }
                | Activity::Unbind { .. }
                | Activity::Comfort { .. }
                | Activity::Express { .. }
                | Activity::Play { .. }
                | Activity::Construct { .. }
                | Activity::Lesson { .. }
                | Activity::Take { .. }
                | Activity::ReportIncident { .. }
                | Activity::Cultivate { .. }
        );
        if timed != deadlines.contains(id) {
            return Err(fail());
        }
        let location = &locations[id];
        let allowed = match location {
            Location::AtArea(_) => true,
            Location::StoppedOnRoad { .. } => matches!(
                *activity,
                Activity::Idle
                    | Activity::Rest
                    | Activity::Eat { .. }
                    | Activity::Load { .. }
                    | Activity::Care { .. }
                    | Activity::Comfort { .. }
                    | Activity::Express { .. }
                    | Activity::Lift { .. }
                    | Activity::HealthWork { .. }
                    | Activity::Unbind { .. }
                    | Activity::Escape { .. }
                    | Activity::Restrain { .. }
                    | Activity::Combat { .. }
            ),
            Location::InTransit { .. } => matches!(
                *activity,
                Activity::Idle | Activity::Express { .. } | Activity::Escape { .. }
            ),
            Location::Carried { .. } => false,
        };
        if !allowed {
            return Err(fail());
        }
        match *activity {
            Activity::ReportAgreement { recipient, .. } if !people.contains_key(&recipient) => {
                return Err(fail());
            }
            Activity::Cultivate {
                field,
                operation,
                completes,
            } if !s.state.agriculture.as_ref().is_some_and(|_| {
                game_agriculture::cultivation(s.components, *id).is_some_and(|job| {
                    job.field == field && job.operation == operation && job.completes == completes
                })
            }) =>
            {
                return Err(fail());
            }
            Activity::Express { completes, .. } | Activity::Play { completes, .. } => {
                let life = s
                    .state
                    .development
                    .as_ref()
                    .and_then(|_| game_lifecycle::life(s.components, *id))
                    .ok_or_else(fail)?;
                let duration = if matches!(*activity, Activity::Play { .. }) {
                    MINUTE.0
                } else {
                    6_000
                };
                let started = completes.0.checked_sub(duration).ok_or_else(fail)?;
                let age = started
                    .checked_sub(life.born.0)
                    .and_then(|v| v.checked_add(life.initial_age.0))
                    .ok_or_else(fail)?;
                let rules = &s.state.development.as_ref().unwrap().rules;
                if started > s.time.0
                    || age >= rules.adulthood.0
                    || matches!(*activity, Activity::Play { .. }) && age < rules.infancy.0
                {
                    return Err(fail());
                }
            }
            Activity::Unbind { target, .. } if !people.contains_key(&target) => return Err(fail()),
            Activity::Care { child, .. } | Activity::Comfort { child, .. }
                if !care_targets.insert(child) =>
            {
                return Err(fail());
            }
            Activity::Eat { source, .. } => {
                if !game_cargo::inventory(s.components, source)
                    .is_some_and(|i| i.good == Good::Food)
                {
                    return Err(fail());
                }
            }
            Activity::Load {
                source,
                destination,
                quantity,
                completes,
            } => {
                let source_stock = game_cargo::inventory(s.components, source).ok_or_else(fail)?;
                let destination_stock =
                    game_cargo::inventory(s.components, destination).ok_or_else(fail)?;
                game_cargo::LoadJob {
                    actor: *id,
                    source,
                    destination,
                    quantity,
                    completes,
                }
                .validate(s.time, source_stock, destination_stock)
                .map_err(|_| fail())?;
            }
            Activity::Work { site } => {
                if !job.is_some_and(|j| j.site == site && j.remaining > 0) {
                    return Err(fail());
                }
            }
            Activity::Tell {
                recipient,
                ref fact,
                ..
            } if !people.contains_key(&recipient)
                || !game_cargo::inventory(s.components, fact.inventory).is_some()
                || fact.observed_at > s.time =>
            {
                return Err(fail());
            }
            Activity::Comfort { child, .. } if !people.contains_key(&child) => return Err(fail()),
            Activity::Care { child, source, .. }
                if !people.contains_key(&child)
                    || !game_cargo::inventory(s.components, source).is_some_and(|i| {
                        i.good == Good::Food || i.good == Good::Water && s.state.health.is_some()
                    }) =>
            {
                return Err(fail());
            }
            Activity::Conceive {
                pregnancy,
                completes,
            } if !s.state.development.as_ref().is_some_and(|_| {
                game_lifecycle::pregnancy(s.components, pregnancy).is_some_and(|p| {
                    [p.gestator, p.partner].contains(id)
                        && p.state == development::PregnancyState::Conceiving { completes }
                })
            }) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    Ok(())
}

fn arrived_duration(departed: SimTime, arrives: SimTime) -> Option<u64> {
    arrives.0.checked_sub(departed.0)
}
