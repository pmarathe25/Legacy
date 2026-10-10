use crate::*;

const SAVE_VERSION: u32 = 23;
#[derive(Deserialize)]
struct SaveHeader {
    version: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    version: u32,
    state: CanonicalState,
}
#[derive(Serialize)]
struct SaveRef<'a> {
    version: u32,
    state: &'a CanonicalState,
}

impl Simulation {
    pub fn save(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&SaveRef {
            version: SAVE_VERSION,
            state: &self.state,
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
        validate(&save.state)?;
        let sim = Self::from_state(save.state, execution, 0)?;
        sim.validate_ecology()?;
        sim.validate_enforcement()?;
        Ok(sim)
    }

    pub fn next_caller_sequence(&self) -> Result<u64, Error> {
        self.state
            .accepted
            .last_key_value()
            .map_or(Ok(1), |(seq, _)| seq.checked_add(1).ok_or(Error::Overflow))
    }
}

fn validate(s: &CanonicalState) -> Result<(), Error> {
    genetics::validate(s)?;
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    let playable = scenarios::PLAYABLE.contains(&s.scenario.id.as_str());
    let enforcement_fixture = matches!(
        s.scenario.id.as_str(),
        "enforcement-fixture" | "realm-fixture"
    );
    let expanded_fixture = matches!(
        s.scenario.id.as_str(),
        "ecology-fixture" | "farm-enterprise-fixture"
    );
    if (s.agriculture.is_some() && s.environment.is_none())
        || (playable && (s.realm.is_none() || s.armies.is_none() || s.agriculture.is_none()))
        || (s.scenario.id == "realm-fixture" && s.realm.is_none())
        || (s.scenario.id == "transport-fixture" && s.transport.is_none())
        || (s.scenario.id == "genetics-fixture" && s.genetics.is_none())
        || (!playable
            && !expanded_fixture
            && !enforcement_fixture
            && (!matches!(
                s.scenario.id.as_str(),
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
            ) || s.scenario.version != 1
                || !s.scenario.debug_fixture
                || (s.scenario.id != "kernel-fixture") != s.physical.is_some()
                || matches!(
                    s.scenario.id.as_str(),
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
                ) != s.decisions.is_some()
                || matches!(
                    s.scenario.id.as_str(),
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
                ) != s.development.is_some()
                || matches!(
                    s.scenario.id.as_str(),
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
                ) != s.combat.is_some()
                || (s.scenario.id == "farming-fixture") != s.environment.is_some()
                || (s.scenario.id == "farming-fixture") != s.agriculture.is_some()
                || matches!(
                    s.scenario.id.as_str(),
                    "agreement-fixture"
                        | "property-fixture"
                        | "institution-fixture"
                        | "project-fixture"
                        | "mission-fixture"
                        | "army-fixture"
                ) != s.agreements.is_some()
                || matches!(
                    s.scenario.id.as_str(),
                    "lineage-fixture"
                        | "skills-fixture"
                        | "health-fixture"
                        | "genetics-fixture"
                        | "transport-fixture"
                        | "invocation-fixture"
                        | "force-fixture"
                ) != s.lineage.is_some()
                || (s.scenario.id == "property-fixture") != s.property.is_some()
                || (s.scenario.id == "institution-fixture") != s.institutions.is_some()
                || (matches!(
                    s.scenario.id.as_str(),
                    "skills-fixture" | "health-fixture" | "genetics-fixture" | "invocation-fixture"
                ) && s.skills.is_none())
                || (s.scenario.id == "project-fixture") != s.projects.is_some()
                || (s.scenario.id == "invocation-fixture") != s.invocation.is_some()
                || (s.scenario.id == "force-fixture" && s.force.is_none())
                || (s.force.is_some() && s.combat.is_none())
                || (s.scenario.id == "mission-fixture" && s.missions.is_none())
                || (s.scenario.id == "army-fixture" && s.armies.is_none())
                || (s.armies.is_some() && s.missions.is_none())
                || (s.missions.is_some() && (s.combat.is_none() || s.agreements.is_none()))))
        || ((playable || expanded_fixture || enforcement_fixture) && s.scenario.version != 1)
        || (playable
            && (s.scenario.debug_fixture
                || s.physical.is_none()
                || s.decisions.is_none()
                || s.development.is_none()
                || s.combat.is_none()
                || s.environment.is_none()
                || s.agreements.is_none()
                || s.lineage.is_none()
                || s.property.is_none()
                || s.institutions.is_none()
                || s.skills.is_none()
                || s.projects.is_none()
                || s.invocation.is_none()))
        || (enforcement_fixture
            && (!s.scenario.debug_fixture
                || s.physical.is_none()
                || s.decisions.is_none()
                || s.development.is_none()
                || s.combat.is_none()
                || s.agreements.is_none()
                || s.property.is_none()
                || s.institutions.is_none()
                || s.missions.is_none()
                || s.enforcement.is_none()))
        || (expanded_fixture
            && (!s.scenario.debug_fixture
                || s.physical.is_none()
                || s.decisions.is_none()
                || s.development.is_none()
                || s.combat.is_none()
                || s.environment.is_none()
                || s.ecology.is_none()
                || s.agriculture.is_none()))
        || (s.scenario.id == "farm-enterprise-fixture"
            && (s.projects.is_none() || s.agreements.is_none()))
    {
        return Err(fail());
    }
    let mut ids = BTreeSet::new();
    for id in s
        .areas
        .iter()
        .map(|a| a.id.0)
        .chain(s.people.iter().map(|p| p.id.0))
        .chain(s.inventories.keys().map(|i| i.0))
    {
        if id == 0 || id >= s.next_id || !ids.insert(id) {
            return Err(fail());
        }
    }
    if let Some(world) = &s.health {
        for id in world.shelters.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.enforcement {
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
    if let Some(world) = &s.ecology {
        for id in world
            .resources
            .keys()
            .map(|id| id.0)
            .chain(world.plots.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.missions {
        for id in world
            .missions
            .keys()
            .map(|id| id.0)
            .chain(world.recruitments.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.physical {
        for id in world.sites.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.invocation {
        for id in world
            .warnings
            .keys()
            .map(|id| id.0)
            .chain(world.records.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.projects {
        for id in world
            .projects
            .keys()
            .map(|id| id.0)
            .chain(world.enterprises.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.institutions {
        for id in world
            .institutions
            .keys()
            .map(|id| id.0)
            .chain(world.proposals.keys().map(|id| id.0))
            .chain(world.cases.keys().map(|id| id.0))
            .chain(world.concepts.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.property {
        for id in world
            .claims
            .keys()
            .map(|id| id.0)
            .chain(world.incidents.keys().map(|id| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.agreements {
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
    if let Some(world) = &s.agriculture {
        for id in world.fields.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.decisions {
        for id in world.concepts.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.development {
        for id in world.pregnancies.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.armies {
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
    if let Some(world) = &s.realm {
        for id in world.kernel.commitments.keys() {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if let Some(world) = &s.combat {
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
    if !s.people.windows(2).all(|p| p[0].id < p[1].id)
        || !s.areas.windows(2).all(|a| a[0].id < a[1].id)
    {
        return Err(fail());
    }
    let people: BTreeMap<_, _> = s.people.iter().map(|p| (p.id, p)).collect();
    let areas: BTreeSet<_> = s.areas.iter().map(|a| a.id).collect();
    if !people.get(&s.ruler).is_some_and(|p| p.role == Role::Ruler)
        || s.people.iter().filter(|p| p.role == Role::Ruler).count() != 1
    {
        return Err(fail());
    }
    for a in &s.areas {
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
    for (time, events) in &s.due {
        if *time <= s.time || events.is_empty() {
            return Err(fail());
        }
        for event in events {
            match event {
                Due::Arrive(id) => {
                    let p = people.get(id).ok_or_else(fail)?;
                    let deadline = s
                        .transport
                        .as_ref()
                        .and_then(|world| world.approaches.get(id))
                        .map(|approach| approach.completes);
                    if !matches!(p.location, Location::InTransit { arrives, .. } if deadline.unwrap_or(arrives) == *time)
                        || !scheduled_arrivals.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                Due::Review => reviews.push(*time),
                Due::CombatRound(_) => {
                    if s.combat.is_none() {
                        return Err(fail());
                    }
                }
                Due::Age(_) | Due::Birth(_) => {
                    if s.development.is_none() {
                        return Err(fail());
                    }
                }
                Due::Transfers(intents) => {
                    for i in intents {
                        if i.quantity.0 == 0
                            || i.source == i.destination
                            || !people.contains_key(&i.actor)
                            || !s.inventories.contains_key(&i.source)
                            || !s.inventories.contains_key(&i.destination)
                        {
                            return Err(fail());
                        }
                    }
                }
                Due::Physical(id) => {
                    let actor = s
                        .physical
                        .as_ref()
                        .and_then(|w| w.actors.get(id))
                        .ok_or_else(fail)?;
                    if !matches!(actor.activity, Activity::RealmTalk { completes, .. } | Activity::Lift { completes, .. } | Activity::Social { completes, .. } | Activity::Instruction { completes, .. } | Activity::Practice { completes, .. } | Activity::EnforcementTalk { completes, .. } | Activity::Gather { completes, .. } | Activity::ReportAgreement { completes, .. } | Activity::Escape { completes, .. } | Activity::Restrain { completes, .. } | Activity::Eat { completes, .. } | Activity::Load { completes, .. } | Activity::Tell { completes, .. } | Activity::Care { completes, .. } | Activity::Conceive { completes, .. } | Activity::Unbind { completes, .. } | Activity::Comfort { completes, .. } | Activity::Construct { completes, .. } | Activity::Lesson { completes, .. } | Activity::Take { completes, .. } | Activity::ReportIncident { completes, .. } | Activity::Cultivate { completes, .. } | Activity::Express { completes, .. } | Activity::Play { completes, .. } if completes == *time)
                        || !physical_deadlines.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                Due::Spoil(id) => {
                    if !s
                        .inventories
                        .get(id)
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
    for p in &s.people {
        if p.work_remainder >= 60_000
            || p.hunger_exempt != (p.role == Role::Guardian)
            || !s
                .inventories
                .get(&p.inventory)
                .is_some_and(|i| i.location == InventoryLocation::Carried(p.id))
        {
            return Err(fail());
        }
        let mut origin = match p.location {
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
                    .areas
                    .iter()
                    .find(|a| a.id == from)
                    .and_then(|a| a.links.iter().find(|l| l.destination == to))
                    .ok_or_else(fail)?;
                let dead = s
                    .physical
                    .as_ref()
                    .is_some_and(|w| w.actors.get(&p.id).is_some_and(|a| !a.vitals.alive));
                if departed > s.time
                    || arrived_duration(departed, arrives)
                        != Some(match road_start {
                            None => link.duration.0,
                            Some(point) => {
                                let original = s
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
                        .transport
                        .as_ref()
                        .and_then(|world| world.groups.get(&p.id))
                        .is_some_and(|group| group.anchor == carrier)
                {
                    return Err(fail());
                }
                // No independent route remains on a body being physically borne.
                s.areas.first().ok_or_else(fail)?.id
            }
        };
        for destination in &p.route {
            if !s
                .areas
                .iter()
                .any(|a| a.id == origin && a.links.iter().any(|l| l.destination == *destination))
            {
                return Err(fail());
            }
            origin = *destination;
        }
    }
    for (id, inventory) in &s.inventories {
        if *id != inventory.id
            || inventory.quantity > inventory.capacity
            || inventory.access.iter().any(|p| !people.contains_key(p))
            || inventory
                .last_granted
                .is_some_and(|p| !people.contains_key(&p))
            || inventory.spoils_at.is_some() != spoil_deadlines.contains(id)
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
        if event.id.0 != i as u64 + 1
            || event.time > s.time
            || (i > 0 && event.time < s.events[i - 1].time)
        {
            return Err(fail());
        }
        match event.kind {
            EventKind::RealmProposed {
                commitment,
                actor,
                revision,
            } => {
                if !people.contains_key(&actor)
                    || !s
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
                    .realm
                    .as_ref()
                    .and_then(|w| w.kernel.commitments.get(&commitment))
                    .is_some_and(|c| c.revision(revision).is_some())
                    || !s.institutions.as_ref().is_some_and(|w| {
                        w.proposals
                            .get(&proposal)
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
                    .institutions
                    .as_ref()
                    .is_some_and(|w| w.institutions.contains_key(&institution))
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
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                    || !s
                        .institutions
                        .as_ref()
                        .is_some_and(|w| w.institutions.contains_key(&institution))
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
                    || !s.enforcement.as_ref().is_some_and(|w| {
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
                        .enforcement
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
                    || !s
                        .missions
                        .as_ref()
                        .is_some_and(|w| w.missions.contains_key(&mission))
                {
                    return Err(fail());
                }
            }
            EventKind::EnforcementOutcome { case, .. } => {
                if !s
                    .enforcement
                    .as_ref()
                    .is_some_and(|w| w.cases.contains_key(&case))
                {
                    return Err(fail());
                }
            }
            EventKind::Drank { actor, source, .. } | EventKind::WaterPurified { actor, source } => {
                if !people.contains_key(&actor)
                    || !s
                        .inventories
                        .get(&source)
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
                        .health
                        .as_ref()
                        .is_some_and(|w| w.shelters.contains_key(&shelter))
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
                        .health
                        .as_ref()
                        .is_some_and(|w| w.shelters.contains_key(&shelter))
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
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resources.contains_key(&resource))
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
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resources.contains_key(&resource))
                {
                    return Err(fail());
                }
            }
            EventKind::GatheringStarted {
                resource, actor, ..
            } => {
                if !people.contains_key(&actor)
                    || !s
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resources.contains_key(&resource))
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
                    || !s.inventories.contains_key(&destination)
                    || !s
                        .ecology
                        .as_ref()
                        .is_some_and(|w| w.resources.contains_key(&resource))
                {
                    return Err(fail());
                }
            }
            EventKind::MissionProposed { mission, sponsor } => {
                if !people.contains_key(&sponsor)
                    || !s
                        .missions
                        .as_ref()
                        .is_some_and(|w| w.missions.contains_key(&mission))
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
                        .missions
                        .as_ref()
                        .is_some_and(|w| w.missions.contains_key(&mission))
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
                        .missions
                        .as_ref()
                        .is_some_and(|w| w.missions.contains_key(&mission))
                {
                    return Err(fail());
                }
            }
            EventKind::MissionDeparted { mission } | EventKind::MissionFinished { mission, .. } => {
                if !s
                    .missions
                    .as_ref()
                    .is_some_and(|w| w.missions.contains_key(&mission))
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
                    || !s.inventories.contains_key(&binding)
                {
                    return Err(fail());
                }
            }
            EventKind::CombatReinforcement {
                encounter, actor, ..
            } => {
                if !people.contains_key(&actor)
                    || !s.combat.as_ref().is_some_and(|w| {
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
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.encounters.contains_key(&encounter))
                    || !people.contains_key(&actor)
                    || !people.contains_key(&target)
                    || !s.inventories.contains_key(&binding)
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
                if !s.combat.as_ref().is_some_and(|w| {
                    w.map(map)
                        .is_some_and(|m| m.obstacles.iter().any(|o| o.index == obstacle))
                }) {
                    return Err(fail());
                }
            }
            EventKind::RoundEnded { encounter, .. } | EventKind::EncounterEnded { encounter } => {
                if !s
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
                    || arrives <= event.time
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
                    || !s.inventories.contains_key(&source)
                    || !s.inventories.contains_key(&destination)
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::TransferFailed { actor, source } => {
                if !people.contains_key(&actor) || !s.inventories.contains_key(&source) {
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
                    || !s.inventories.contains_key(&source)
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::Spoiled { inventory, .. } => {
                if !s.inventories.contains_key(&inventory) {
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
                        .physical
                        .as_ref()
                        .is_some_and(|w| w.sites.contains_key(&site))
                    || !s.inventories.contains_key(&output)
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
                    || !s.inventories.contains_key(&inventory)
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
                    .development
                    .as_ref()
                    .is_some_and(|w| w.pregnancies.contains_key(&pregnancy))
                    || parents.iter().any(|p| !people.contains_key(p))
                {
                    return Err(fail());
                }
            }
            EventKind::PregnancyEnded { pregnancy } => {
                if !s
                    .development
                    .as_ref()
                    .is_some_and(|w| w.pregnancies.contains_key(&pregnancy))
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
                    .development
                    .as_ref()
                    .is_some_and(|w| w.pregnancies.contains_key(&pregnancy))
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
                    || !s.inventories.contains_key(&source)
                    || quantity.0 == 0
                {
                    return Err(fail());
                }
            }
            EventKind::InvocationStarted { actor, ref lives } => {
                if !people.contains_key(&actor)
                    || !s
                        .invocation
                        .as_ref()
                        .is_some_and(|w| lives.iter().all(|id| w.history.contains_key(id)))
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
                        .invocation
                        .as_ref()
                        .is_some_and(|w| w.warnings.contains_key(&warning))
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
                    || !s.inventories.contains_key(&inventory)
                    || !s
                        .invocation
                        .as_ref()
                        .is_some_and(|w| w.records.contains_key(&record))
                {
                    return Err(fail());
                }
            }
            EventKind::MemoryRead { record, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .invocation
                        .as_ref()
                        .is_some_and(|w| w.records.contains_key(&record))
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
                    || !s
                        .invocation
                        .as_ref()
                        .is_some_and(|w| w.history.contains_key(&life))
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectProposed { project, sponsor } => {
                if !people.contains_key(&sponsor)
                    || !s
                        .projects
                        .as_ref()
                        .is_some_and(|w| w.projects.contains_key(&project))
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
                        .projects
                        .as_ref()
                        .is_some_and(|w| w.projects.contains_key(&project))
                    || !s
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
                        .projects
                        .as_ref()
                        .is_some_and(|w| w.projects.contains_key(&project))
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectFailed { project } => {
                if !s
                    .projects
                    .as_ref()
                    .is_some_and(|w| w.projects.contains_key(&project))
                {
                    return Err(fail());
                }
            }
            EventKind::ProjectCompleted {
                project,
                enterprise,
                site,
            } => {
                if !s.projects.as_ref().is_some_and(|w| {
                    w.projects.contains_key(&project)
                        && w.enterprises
                            .get(&enterprise)
                            .is_some_and(|e| e.site == site)
                }) || !s
                    .physical
                    .as_ref()
                    .is_some_and(|w| w.sites.contains_key(&site))
                {
                    return Err(fail());
                }
            }
            EventKind::EnterpriseInputUsed {
                enterprise, site, ..
            } => {
                if !s.projects.as_ref().is_some_and(|w| {
                    w.enterprises
                        .get(&enterprise)
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
                    || !s
                        .institutions
                        .as_ref()
                        .is_some_and(|w| w.institutions.contains_key(&institution))
                {
                    return Err(fail());
                }
            }
            EventKind::InstitutionFounded { institution }
            | EventKind::InstitutionLeft { institution, .. }
            | EventKind::OfficeSupportChanged { institution, .. }
            | EventKind::InstitutionRuleApplied { institution, .. } => {
                if !s
                    .institutions
                    .as_ref()
                    .is_some_and(|w| w.institutions.contains_key(&institution))
                {
                    return Err(fail());
                }
            }
            EventKind::InstitutionChanged {
                institution,
                proposal,
            } => {
                if !s.institutions.as_ref().is_some_and(|w| {
                    w.institutions.contains_key(&institution)
                        && w.proposals
                            .get(&proposal)
                            .is_some_and(|p| p.institution == institution)
                }) {
                    return Err(fail());
                }
            }
            EventKind::SocialCaseRecorded { case, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .institutions
                        .as_ref()
                        .is_some_and(|w| w.cases.contains_key(&case))
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
                    || !s.inventories.contains_key(&inventory)
                    || !s
                        .property
                        .as_ref()
                        .is_some_and(|w| w.claims.contains_key(&claim))
                {
                    return Err(fail());
                }
            }
            EventKind::ClaimRecognized { claim, actor, .. } => {
                if !people.contains_key(&actor)
                    || !s
                        .property
                        .as_ref()
                        .is_some_and(|w| w.claims.contains_key(&claim))
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
                    || !s.inventories.contains_key(&inventory)
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
                    || !s.inventories.contains_key(&source)
                    || !s.inventories.contains_key(&destination)
                    || !s
                        .property
                        .as_ref()
                        .is_some_and(|w| w.incidents.contains_key(&incident))
                {
                    return Err(fail());
                }
            }
            EventKind::TakingInterrupted { incident, actor } => {
                if !people.contains_key(&actor)
                    || !s
                        .property
                        .as_ref()
                        .is_some_and(|w| w.incidents.contains_key(&incident))
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
                        .property
                        .as_ref()
                        .is_some_and(|w| w.incidents.contains_key(&incident))
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
                if !s.agreements.as_ref().is_some_and(|w| {
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
                        .agreements
                        .as_ref()
                        .is_some_and(|w| w.obligations.contains_key(&obligation))
                {
                    return Err(fail());
                }
            }
            EventKind::ObligationDefaulted { obligation } => {
                if !s
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
                        .agriculture
                        .as_ref()
                        .is_some_and(|w| w.fields.contains_key(&field))
                {
                    return Err(fail());
                }
            }
            EventKind::CropFailed { field } => {
                if !s
                    .agriculture
                    .as_ref()
                    .is_some_and(|w| w.fields.contains_key(&field))
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
    if s.next_event != s.events.len() as u64 + 1 || s.next_command != s.accepted.len() as u64 + 1 {
        return Err(fail());
    }
    let mut sequences = BTreeSet::new();
    for (caller, (_, receipt)) in &s.accepted {
        if *caller != receipt.caller_sequence
            || receipt.accepted_at > s.time
            || receipt.authoritative_sequence == 0
            || receipt.authoritative_sequence >= s.next_command
            || !sequences.insert(receipt.authoritative_sequence)
        {
            return Err(fail());
        }
    }
    if let Some(world) = &s.physical {
        validate_physical(s, world, &people, &areas, &physical_deadlines)?;
    }
    if let Some(world) = &s.decisions {
        if world.minds.len() != people.len()
            || !world.concepts.contains_key(&world.work_concept)
            || world.last_learning_event >= s.next_event
            || world.switching_margin < 0
        {
            return Err(fail());
        }
        for (id, mind) in &world.minds {
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
                time > s.time || !s.inventories.contains_key(&inventory)
            }) {
                return Err(fail());
            }
            for (id, fact) in &mind.knowledge {
                if *id != fact.inventory
                    || !s.inventories.contains_key(id)
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
                    || !s.physical.as_ref().unwrap().sites.contains_key(id)
                    || !areas.contains(&site.area)
                    || !s.inventories.contains_key(&site.input)
                    || !s.inventories.contains_key(&site.tool)
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
    if world.actors.len() != people.len()
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
    for (id, site) in &world.sites {
        if *id != site.id
            || !areas.contains(&site.area)
            || site.duration.0 == 0
            || site.input_quantity.0 == 0
            || site.output_quantity.0 == 0
            || site.access.iter().any(|p| !people.contains_key(p))
        {
            return Err(fail());
        }
        for inventory in [site.input, site.tool, site.output] {
            if !s
                .inventories
                .get(&inventory)
                .is_some_and(|i| i.location == InventoryLocation::Area(site.area))
            {
                return Err(fail());
            }
        }
        if s.inventories[&site.tool].good != Good::Tool {
            return Err(fail());
        }
    }
    let mut cargo = BTreeMap::<PersonId, u128>::new();
    for i in s.inventories.values() {
        if let InventoryLocation::Carried(p) = i.location {
            *cargo.entry(p).or_default() += u128::from(i.quantity.0);
        }
    }
    let locations = crate::transport::resolved_locations_for(s)?;
    let mut care_targets = BTreeSet::new();
    for (id, actor) in &world.actors {
        let p = people.get(id).ok_or_else(fail)?;
        let carried = cargo.get(id).copied().unwrap_or(0);
        if actor.health_remainder >= 1_000
            || actor.fatigue_remainder >= 1_000
            || actor.vitals.health > physical::rules_for(s, *id).max_health
            || actor.vitals.fatigue > rules.fatigue_limit
            || (carried > u128::from(physical::rules_for(s, *id).carrying_capacity.0)
                && matches!(p.location, Location::InTransit { .. }))
            || (p.hunger_exempt && (actor.vitals.hunger != 0 || !actor.vitals.alive))
            || (!actor.vitals.alive && actor.vitals.health != 0)
        {
            return Err(fail());
        }
        if !actor.vitals.alive && (actor.activity != Activity::Idle || actor.job.is_some()) {
            return Err(fail());
        }
        if let Some(job) = &actor.job
            && (job.tool.is_some_and(|tool| {
                !s.inventories
                    .get(&tool)
                    .is_some_and(|i| i.good == Good::Tool)
            }) || !matches!(job.rate.denominator, 1_000 | 1_000_000)
                || job.body_rate.denominator != 1_000
                || !(250..=2_000).contains(&job.body_rate.numerator)
                || {
                    let numerator = u128::from(job.rate.numerator)
                        * u128::from(job.body_rate.denominator)
                        * 1_000;
                    let denominator =
                        u128::from(job.rate.denominator) * u128::from(job.body_rate.numerator);
                    let maximum = s.skills.as_ref().map_or(1_000, |skills| {
                        1_000
                            + u64::from(skills.level(*id, Skill::Craft))
                                * u64::from(skills.rules.work_bonus_per_level)
                    });
                    denominator == 0
                        || numerator % denominator != 0
                        || numerator / denominator < 1_000
                        || numerator / denominator > u128::from(maximum)
                }
                || job.work_remainder >= job.rate.denominator
                || !world
                    .sites
                    .get(&job.site)
                    .is_some_and(|site| job.remaining <= site.duration.0))
        {
            return Err(fail());
        }
        let timed = matches!(
            actor.activity,
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
                actor.activity,
                Activity::Idle
                    | Activity::Rest
                    | Activity::Eat { .. }
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
                actor.activity,
                Activity::Idle | Activity::Express { .. } | Activity::Escape { .. }
            ),
            Location::Carried { .. } => false,
        };
        if !allowed {
            return Err(fail());
        }
        match actor.activity {
            Activity::ReportAgreement { recipient, .. } if !people.contains_key(&recipient) => {
                return Err(fail());
            }
            Activity::Cultivate {
                field,
                operation,
                completes,
            } if !s.agriculture.as_ref().is_some_and(|w| {
                w.jobs.get(id).is_some_and(|job| {
                    job.field == field && job.operation == operation && job.completes == completes
                })
            }) =>
            {
                return Err(fail());
            }
            Activity::Express { completes, .. } | Activity::Play { completes, .. } => {
                let life = s
                    .development
                    .as_ref()
                    .and_then(|w| w.lives.get(id))
                    .ok_or_else(fail)?;
                let duration = if matches!(actor.activity, Activity::Play { .. }) {
                    MINUTE.0
                } else {
                    6_000
                };
                let started = completes.0.checked_sub(duration).ok_or_else(fail)?;
                let age = started
                    .checked_sub(life.born.0)
                    .and_then(|v| v.checked_add(life.initial_age.0))
                    .ok_or_else(fail)?;
                let rules = &s.development.as_ref().unwrap().rules;
                if started > s.time.0
                    || age >= rules.adulthood.0
                    || matches!(actor.activity, Activity::Play { .. }) && age < rules.infancy.0
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
                if !s
                    .inventories
                    .get(&source)
                    .is_some_and(|i| i.good == Good::Food)
                {
                    return Err(fail());
                }
            }
            Activity::Load {
                source,
                destination,
                quantity,
                ..
            } => {
                if quantity.0 == 0
                    || source == destination
                    || !s.inventories.contains_key(&source)
                    || !s.inventories.contains_key(&destination)
                    || s.inventories[&source].good != s.inventories[&destination].good
                {
                    return Err(fail());
                }
            }
            Activity::Work { site } => {
                if !actor
                    .job
                    .as_ref()
                    .is_some_and(|j| j.site == site && j.remaining > 0)
                {
                    return Err(fail());
                }
            }
            Activity::Tell {
                recipient,
                ref fact,
                ..
            } if !people.contains_key(&recipient)
                || !s.inventories.contains_key(&fact.inventory)
                || fact.observed_at > s.time =>
            {
                return Err(fail());
            }
            Activity::Comfort { child, .. } if !people.contains_key(&child) => return Err(fail()),
            Activity::Care { child, source, .. }
                if !people.contains_key(&child)
                    || !s
                        .inventories
                        .get(&source)
                        .is_some_and(|i| i.good == Good::Food) =>
            {
                return Err(fail());
            }
            Activity::Conceive {
                pregnancy,
                completes,
            } if !s.development.as_ref().is_some_and(|w| {
                w.pregnancies.get(&pregnancy).is_some_and(|p| {
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
