use crate::*;

const SAVE_VERSION: u32 = 51;
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
        .chain(game_characters::persons(s.components).map(|(id, _)| id.0))
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
    if let Some(lineage) = &s.state.lineage {
        let id = lineage.history.0;
        if id == 0
            || id >= s.next_id
            || !ids.contains(&id)
            || game_characters::people::person(s.components, PersonId(id)).is_none()
        {
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
    if s.state.enforcement.is_some() {
        for id in game_society::enforcement::cases(s.components)
            .map(|(id, _)| id.0)
            .chain(
                game_society::enforcement::cases(s.components)
                    .filter_map(|(_, case)| case.judgment.as_ref().map(|judgment| judgment.id.0)),
            )
        {
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
    for id in game_agreements::agreements(s.components)
        .map(|(id, _)| id.0)
        .chain(game_agreements::obligations(s.components).map(|(id, _)| id.0))
        .chain(game_agreements::amendments(s.components).map(|(id, _)| id.0))
    {
        if id == 0 || id >= s.next_id || !ids.insert(id) {
            return Err(fail());
        }
    }
    game_property::validate_custody(
        s.components,
        game_property::CustodyContext {
            inventories: &game_cargo::inventories(s.components)
                .map(|(id, _)| id)
                .collect(),
            people: &s.people().map(|row| row.id).collect(),
        },
    )?;
    if let Some(world) = &s.state.agriculture {
        for id in world.fields(s.components).map(|(id, _)| id) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    }
    if s.state.decisions.is_none()
        && game_characters::decisions::stock_rows(s.components)
            .next()
            .is_some()
    {
        return Err(fail());
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
    if s.state.armies.is_some() {
        for (_, army) in game_armies::armies(s.components) {
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
    if s.state.realm.is_some() {
        for (id, _) in game_realm::commitments(s.components) {
            if id.0 == 0 || id.0 >= s.next_id || !ids.insert(id.0) {
                return Err(fail());
            }
        }
    } else if game_realm::commitments(s.components).next().is_some()
        || game_realm::knowledges(s.components).next().is_some()
        || game_realm::request_rows(s.components).next().is_some()
        || game_realm::talks(s.components).next().is_some()
    {
        return Err(fail());
    }
    if s.state.combat.is_some() {
        for id in game_conflict::maps(s.components)
            .map(|(id, _)| id.0)
            .chain(game_conflict::encounters(s.components).map(|(id, _)| id.0))
        {
            if id == 0 || id >= s.next_id || !ids.insert(id) {
                return Err(fail());
            }
        }
    }
    if !s.state.areas.windows(2).all(|a| a[0].id < a[1].id) {
        return Err(fail());
    }
    game_characters::people::validate_people(
        s.components,
        game_characters::people::PersonContext {
            ruler: s.state.ruler,
            next_id: s.next_id,
        },
        |inventory| {
            game_cargo::inventory(s.components, inventory).and_then(|stock| match stock.location {
                InventoryLocation::Carried(actor) => Some(actor),
                _ => None,
            })
        },
    )?;
    let people: BTreeMap<_, _> = s.people().map(|p| (p.id, p)).collect();
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
    for a in &s.state.areas {
        if !s.components.contains(game_ecs::StableId(a.id.0)) {
            return Err(fail());
        }
        game_spatial::receipt_validation::validate_link_rows(
            a.links.iter().map(|link| (link.destination, link.duration)),
            &areas,
        )?;
    }
    let mut reviews = Vec::new();
    let mut physical_deadlines = BTreeSet::new();
    let mut spoil_deadlines = BTreeSet::new();
    for (time, events) in &s.state.due {
        if *time <= s.time || events.is_empty() {
            return Err(fail());
        }
        for event in events {
            match event {
                Due::Arrive(_) => {}
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
                    for intent in intents {
                        game_cargo::receipt_validation::validate_pending_transfer(
                            intent.quantity,
                            intent.source,
                            intent.destination,
                            people.contains_key(&intent.actor),
                            game_cargo::inventory(s.components, intent.source).is_some(),
                            game_cargo::inventory(s.components, intent.destination).is_some(),
                        )?;
                    }
                }
                Due::Physical(id) => {
                    let activity = s
                        .state
                        .physical
                        .as_ref()
                        .and_then(|_| s.components.activity(*id))
                        .ok_or_else(fail)?;
                    game_physiology::save_validation::validate_deadline(
                        &saved_activity(activity),
                        *time,
                        activity_completion(activity),
                    )?;
                    if !physical_deadlines.insert(*id) {
                        return Err(fail());
                    }
                }
                Due::Spoil(id) => {
                    game_cargo::receipt_validation::validate_spoil_deadline(
                        game_cargo::inventory(s.components, *id),
                        *time,
                    )?;
                    if !spoil_deadlines.insert(*id) {
                        return Err(fail());
                    }
                }
            }
        }
    }
    game_characters::save_validation::validate_review_deadlines(s.time, &reviews)?;
    game_cargo::save_validation::validate_save_rows(
        s.components,
        &people.keys().copied().collect(),
        &areas,
        &spoil_deadlines,
    )?;
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
                if !people.contains_key(&actor) || s.state.realm.is_none() {
                    return Err(fail());
                }
                game_realm::receipt_validation::validate_revision(
                    s.components,
                    commitment,
                    revision,
                )?;
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
                        .is_some_and(|_| game_realm::commitment(s.components, commitment).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::RealmWorkRequested { key, actor } => {
                if !people.contains_key(&actor) || s.state.realm.is_none() {
                    return Err(fail());
                }
                game_realm::receipt_validation::validate_work_request(s.components, key)?;
            }
            EventKind::RealmRatified {
                commitment,
                revision,
                institution,
                proposal,
                ..
            } => {
                if s.state.realm.is_none() || s.state.institutions.is_none() {
                    return Err(fail());
                }
                game_realm::receipt_validation::validate_revision(
                    s.components,
                    commitment,
                    revision,
                )?;
                game_society::receipt_validation::validate_proposal_receipt(
                    s.components,
                    institution,
                    proposal,
                )?;
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
                    || !s.state.enforcement.as_ref().is_some_and(|_| {
                        game_society::enforcement::case(s.components, case).is_some()
                    })
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
                    || !s.state.enforcement.as_ref().is_some_and(|_| {
                        game_society::enforcement::case(s.components, case).is_some()
                    })
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
                if !people.contains_key(&proposer) || s.state.enforcement.is_none() {
                    return Err(fail());
                }
                game_society::receipt_validation::validate_judgment_receipt(
                    s.components,
                    case,
                    judgment,
                )?;
            }
            EventKind::EnforcementResponded { case, actor, .. } => {
                if !people.contains_key(&actor)
                    || !s.state.enforcement.as_ref().is_some_and(|_| {
                        game_society::enforcement::case(s.components, case).is_some()
                    })
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
                    || !s.state.enforcement.as_ref().is_some_and(|_| {
                        game_society::enforcement::case(s.components, case).is_some()
                    })
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
                    .is_some_and(|_| game_society::enforcement::case(s.components, case).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::Drank { actor, source, .. } | EventKind::WaterPurified { actor, source } => {
                game_cargo::receipt_validation::validate_water_source(
                    people.contains_key(&actor),
                    game_cargo::inventory(s.components, source).map(|stock| stock.good),
                )?;
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
                    || !s.state.agreements.as_ref().is_some_and(|_| {
                        game_agreements::obligation(s.components, obligation).is_some()
                    })
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
                    || !s.state.agreements.as_ref().is_some_and(|_| {
                        game_agreements::agreement(s.components, agreement).is_some()
                    })
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
                if !people.contains_key(&actor) || s.state.combat.is_none() {
                    return Err(fail());
                }
                game_conflict::receipt_validation::validate_reinforcement(
                    s.components,
                    encounter,
                    actor,
                )?;
            }
            EventKind::EncounterStarted { encounter, area } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|_| game_conflict::encounter(s.components, encounter).is_some())
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
                    .is_some_and(|_| game_conflict::encounter(s.components, encounter).is_some())
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
                    .is_some_and(|_| game_conflict::encounter(s.components, encounter).is_some())
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
                    .is_some_and(|_| game_conflict::encounter(s.components, encounter).is_some())
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
                if s.state.combat.is_none() {
                    return Err(fail());
                }
                game_conflict::receipt_validation::validate_obstacle_damage(
                    s.components,
                    map,
                    obstacle,
                )?;
            }
            EventKind::RoundEnded { encounter, .. } | EventKind::EncounterEnded { encounter } => {
                if !s
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|_| game_conflict::encounter(s.components, encounter).is_some())
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
                if !people.contains_key(&actor) || !areas.contains(&from) || !areas.contains(&to) {
                    return Err(fail());
                }
                game_spatial::receipt_validation::validate_departure(event.at, arrives)?;
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
                game_cargo::receipt_validation::validate_transfer(
                    quantity,
                    people.contains_key(&actor),
                    game_cargo::inventory(s.components, source).is_some(),
                    game_cargo::inventory(s.components, destination).is_some(),
                )?;
            }
            EventKind::TransferFailed { actor, source } => {
                game_cargo::receipt_validation::validate_transfer_failed(
                    people.contains_key(&actor),
                    game_cargo::inventory(s.components, source).is_some(),
                )?;
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
                game_cargo::receipt_validation::validate_consumption(
                    quantity,
                    people.contains_key(&actor),
                    game_cargo::inventory(s.components, source).is_some(),
                )?;
            }
            EventKind::Spoiled { inventory, .. } => {
                game_cargo::receipt_validation::validate_spoilage(
                    game_cargo::inventory(s.components, inventory).is_some(),
                )?;
            }
            EventKind::Produced {
                actor,
                site,
                output,
                quantity,
            } => {
                game_workshop::receipt_validation::validate_produced(
                    quantity,
                    people.contains_key(&actor),
                    s.state.physical.is_some() && game_workshop::site(s.components, site).is_some(),
                    game_cargo::inventory(s.components, output).is_some(),
                )?;
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
                game_cargo::receipt_validation::validate_care(
                    quantity,
                    people.contains_key(&caregiver),
                    people.contains_key(&child),
                    game_cargo::inventory(s.components, source).is_some(),
                )?;
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
                    || !s.state.agreements.as_ref().is_some_and(|_| {
                        game_agreements::agreement(s.components, agreement).is_some()
                    })
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
                if s.state.projects.is_none() {
                    return Err(fail());
                }
                let farm = s
                    .state
                    .agriculture
                    .as_ref()
                    .and_then(|world| world.field(s.components, site))
                    .map(
                        |field| game_projects::receipt_validation::FarmReceiptFacts {
                            id: field.id,
                            area: field.area,
                            seed: field.seed,
                            tool: field.tool,
                            water: field.water,
                            food: field.food,
                        },
                    );
                game_projects::receipt_validation::validate_project_completed_receipt(
                    s.components,
                    project,
                    enterprise,
                    site,
                    game_projects::receipt_validation::ProjectCompletionSiteFacts {
                        workshop_exists: s.state.physical.is_some()
                            && game_workshop::site(s.components, site).is_some(),
                        farm,
                    },
                )?;
            }
            EventKind::EnterpriseInputUsed {
                enterprise, site, ..
            } => {
                if s.state.projects.is_none() {
                    return Err(fail());
                }
                game_projects::receipt_validation::validate_enterprise_input_receipt(
                    s.components,
                    enterprise,
                    site,
                )?;
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
                if s.state.institutions.is_none()
                    || game_society::institution(s.components, institution).is_none()
                {
                    return Err(fail());
                }
                game_society::receipt_validation::validate_proposal_receipt(
                    s.components,
                    institution,
                    proposal,
                )?;
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
                if !people.contains_key(&teacher) || !people.contains_key(&student) {
                    return Err(fail());
                }
                game_skills::receipt_validation::validate_lesson_gain(gain)?;
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
                game_cargo::receipt_validation::validate_inventory_access(
                    people.contains_key(&actor),
                    people.contains_key(&recipient),
                    game_cargo::inventory(s.components, inventory).is_some(),
                )?;
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
                game_property::validate_taking_receipt(
                    s.components,
                    s.state.property.is_some(),
                    incident,
                    quantity,
                    people.contains_key(&actor),
                    game_cargo::inventory(s.components, source).is_some(),
                    game_cargo::inventory(s.components, destination).is_some(),
                )?;
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
                    || !s.state.agreements.as_ref().is_some_and(|_| {
                        game_agreements::agreement(s.components, agreement).is_some()
                    })
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
                    .is_some_and(|_| game_agreements::agreement(s.components, agreement).is_some())
                {
                    return Err(fail());
                }
            }
            EventKind::AgreementAmended {
                agreement,
                amendment,
            } => {
                game_agreements::validate_amended_receipt(s.components, agreement, amendment)?;
            }
            EventKind::ObligationPerformed {
                obligation,
                actor,
                quantity,
            } => {
                game_agreements::validate_performed_receipt(
                    s.components,
                    s.state.agreements.is_some(),
                    obligation,
                    quantity,
                    people.contains_key(&actor),
                )?;
            }
            EventKind::ObligationDefaulted { obligation } => {
                if !s.state.agreements.as_ref().is_some_and(|_| {
                    game_agreements::obligation(s.components, obligation).is_some()
                }) {
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
    game_characters::save_validation::validate_store::<Command>(
        s.components,
        s.state.decisions.as_ref(),
        &game_characters::save_validation::SaveFacts {
            people: people.keys().copied().collect(),
            areas: areas.clone(),
            inventories: game_cargo::inventories(s.components)
                .map(|(id, stock)| (id, stock.good))
                .collect(),
            sites: if s.state.physical.is_some() {
                game_workshop::sites(s.components)
                    .map(|(id, _)| id)
                    .collect()
            } else {
                BTreeSet::new()
            },
            projects: if s.state.projects.is_some() {
                game_projects::projects(s.components)
                    .map(|(id, _)| id)
                    .collect()
            } else {
                BTreeSet::new()
            },
            obligations: if s.state.agreements.is_some() {
                game_agreements::obligations(s.components)
                    .map(|(id, _)| id)
                    .collect()
            } else {
                BTreeSet::new()
            },
            resources: if let Some(ecology) = &s.state.ecology {
                ecology.resources(s.components).map(|(id, _)| id).collect()
            } else {
                BTreeSet::new()
            },
            time: s.time,
            next_event: s.next_event,
        },
        decisions::action_shape,
    )?;
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
    game_physiology::save_validation::validate_activity_population(
        s.components.activities().map(|(id, _)| id),
        &people.keys().copied().collect(),
    )?;
    let workshop_actors = people
        .keys()
        .map(|id| {
            let body = game_physiology::body(s.components, *id).ok_or_else(fail)?;
            let activity = s.components.activity(*id).ok_or_else(fail)?;
            let maximum_craft_rate = s.state.skills.as_ref().map_or(1_000, |skills| {
                game_skills::rate_for_level(
                    skills.level(s.components, *id, Skill::Craft),
                    &skills.rules,
                )
                .numerator
            });
            Ok((
                *id,
                game_workshop::ActorSaveFacts {
                    alive: body.vitals.alive,
                    active_site: match activity {
                        Activity::Work { site } => Some(*site),
                        _ => None,
                    },
                    maximum_craft_rate,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    game_workshop::validate_store(
        s.components,
        &s.state.workshops,
        &game_workshop::SaveFacts {
            areas: areas.clone(),
            actors: workshop_actors,
        },
    )?;
    let cargo = game_cargo::carried_quantities(s.components).map_err(|_| fail())?;
    let locations = crate::transport::resolved_locations_for(s)?;
    let body_facts = people
        .iter()
        .map(|(id, person)| {
            let activity = s.components.activity(*id).ok_or_else(fail)?;
            let location =
                game_spatial::location(s.components, game_ecs::StableId(id.0)).ok_or_else(fail)?;
            Ok(game_physiology::ValidationFacts {
                actor: *id,
                hunger_exempt: person.hunger_exempt,
                in_transit: matches!(location, Location::InTransit { .. }),
                carried: u128::from(cargo.get(id).copied().unwrap_or(Quantity(0)).0),
                idle: *activity == Activity::Idle,
                has_work: game_workshop::job(s.components, *id).is_some(),
                phenotype: genetics::phenotype(s, *id),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    game_physiology::validate_with_facts(s.components, rules, &body_facts)?;
    let rows = game_physiology::bodies(s.components)
        .map(|(id, _)| {
            let activity = s.components.activity(id).ok_or_else(fail)?;
            Ok((id, saved_activity(activity), locations[&id].clone()))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    game_physiology::save_validation::validate_activity_rows(
        &rows,
        deadlines,
        s.time,
        &PhysicalSavePort { state: s, people },
    )?;
    Ok(())
}

fn saved_activity(
    activity: &Activity,
) -> game_physiology::save_validation::SavedActivity<game_agriculture::FarmOperation> {
    use game_physiology::save_validation::SavedActivity as Saved;
    match *activity {
        Activity::ArmyApproach { .. } => Saved::ArmyApproach,
        Activity::ArmyTalk { .. } => Saved::ArmyTalk,
        Activity::RealmTalk { .. } => Saved::RealmTalk,
        Activity::Lift { .. } => Saved::Lift,
        Activity::Social { .. } => Saved::Social,
        Activity::Instruction { .. } => Saved::Instruction,
        Activity::Practice { .. } => Saved::Practice,
        Activity::EnforcementTalk { .. } => Saved::EnforcementTalk,
        Activity::HealthWork { .. } => Saved::HealthWork,
        Activity::Gather { .. } => Saved::Gather,
        Activity::MissionTalk { .. } => Saved::MissionTalk,
        Activity::Escape { .. } => Saved::Escape,
        Activity::Restrain { .. } => Saved::Restrain,
        Activity::ReportAgreement { .. } => Saved::ReportAgreement,
        Activity::WritingMemory { .. } => Saved::WritingMemory,
        Activity::ReadingMemory { .. } => Saved::ReadingMemory,
        Activity::TeachingMemory { .. } => Saved::TeachingMemory,
        Activity::MovingRecord { .. } => Saved::MovingRecord,
        Activity::Construct { .. } => Saved::Construct,
        Activity::Lesson { .. } => Saved::Lesson,
        Activity::Take { .. } => Saved::Take,
        Activity::ReportIncident { .. } => Saved::ReportIncident,
        Activity::Work { .. } => Saved::Work,
        Activity::Combat { .. } => Saved::Combat,
        Activity::Idle => Saved::Idle,
        Activity::Rest => Saved::Rest,
        Activity::Cultivate {
            field,
            operation,
            completes,
            ..
        } => Saved::Cultivate {
            field,
            operation,
            completes,
        },
        Activity::Comfort { child, .. } => Saved::Comfort { child },
        Activity::Eat { source, .. } => Saved::Eat { source },
        Activity::Load {
            source,
            destination,
            quantity,
            completes,
            ..
        } => Saved::Load {
            source,
            destination,
            quantity,
            completes,
        },
        Activity::Conceive {
            pregnancy,
            completes,
            ..
        } => Saved::Conceive {
            pregnancy,
            completes,
        },
        Activity::Care { child, source, .. } => Saved::Care { child, source },
        Activity::Unbind { target, .. } => Saved::Unbind { target },
        Activity::Express { completes, .. } => Saved::Express { completes },
        Activity::Play { completes, .. } => Saved::Play { completes },
        Activity::Tell {
            recipient,
            ref fact,
            ..
        } => Saved::Tell {
            recipient,
            inventory: fact.inventory,
            observed_at: fact.observed_at,
        },
    }
}
struct PhysicalSavePort<'a> {
    state: &'a CanonicalState<'a>,
    people: &'a BTreeMap<PersonId, &'a Person>,
}
impl game_physiology::save_validation::SavePort for PhysicalSavePort<'_> {
    type Operation = game_agriculture::FarmOperation;
    fn person_exists(&self, actor: PersonId) -> bool {
        self.people.contains_key(&actor)
    }
    fn inventory_good(&self, inventory: InventoryId) -> Option<Good> {
        game_cargo::inventory(self.state.components, inventory).map(|stock| stock.good)
    }
    fn health_available(&self) -> bool {
        self.state.state.health.is_some()
    }
    fn cultivation(&self, actor: PersonId) -> Option<(SiteId, FarmOperation, SimTime)> {
        self.state.state.agriculture.as_ref()?;
        game_agriculture::cultivation(self.state.components, actor)
            .map(|job| (job.field, job.operation, job.completes))
    }
    fn conception(&self, pregnancy: PregnancyId) -> Option<([PersonId; 2], SimTime)> {
        self.state.state.development.as_ref()?;
        let pregnancy = game_lifecycle::pregnancy(self.state.components, pregnancy)?;
        match pregnancy.state {
            development::PregnancyState::Conceiving { completes } => {
                Some(([pregnancy.gestator, pregnancy.partner], completes))
            }
            _ => None,
        }
    }
    fn validate_dependent(
        &self,
        actor: PersonId,
        activity: game_physiology::save_validation::DependentActivity,
        completes: SimTime,
    ) -> Result<(), Error> {
        self.state
            .state
            .development
            .as_ref()
            .ok_or_else(|| Error::InvalidSave("inconsistent physical state".into()))?
            .validate_dependent_activity(
                self.state.components,
                actor,
                self.state.time,
                match activity {
                    game_physiology::save_validation::DependentActivity::Express => {
                        game_lifecycle::DependentActivity::Express
                    }
                    game_physiology::save_validation::DependentActivity::Play => {
                        game_lifecycle::DependentActivity::Play
                    }
                },
                completes,
            )
    }
    fn validate_load(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("inconsistent physical state".into());
        let source_stock = game_cargo::inventory(self.state.components, source).ok_or_else(fail)?;
        let destination_stock =
            game_cargo::inventory(self.state.components, destination).ok_or_else(fail)?;
        game_cargo::LoadJob {
            actor,
            source,
            destination,
            quantity,
            completes,
        }
        .validate(self.state.time, source_stock, destination_stock)
        .map_err(|_| fail())
    }
}

fn activity_completion(activity: &Activity) -> Option<SimTime> {
    match *activity {
        Activity::ArmyApproach { completes, .. } => Some(completes),
        Activity::ArmyTalk { completes, .. } => Some(completes),
        Activity::RealmTalk { completes, .. } => Some(completes),
        Activity::Lift { completes, .. } => Some(completes),
        Activity::Social { completes, .. } => Some(completes),
        Activity::Instruction { completes, .. } => Some(completes),
        Activity::Practice { completes, .. } => Some(completes),
        Activity::EnforcementTalk { completes, .. } => Some(completes),
        Activity::HealthWork { completes, .. } => Some(completes),
        Activity::Gather { completes, .. } => Some(completes),
        Activity::MissionTalk { completes, .. } => Some(completes),
        Activity::Escape { completes, .. } => Some(completes),
        Activity::Restrain { completes, .. } => Some(completes),
        Activity::ReportAgreement { completes, .. } => Some(completes),
        Activity::WritingMemory { completes, .. } => Some(completes),
        Activity::ReadingMemory { completes, .. } => Some(completes),
        Activity::TeachingMemory { completes, .. } => Some(completes),
        Activity::MovingRecord { completes, .. } => Some(completes),
        Activity::Construct { completes, .. } => Some(completes),
        Activity::Lesson { completes, .. } => Some(completes),
        Activity::Take { completes, .. } => Some(completes),
        Activity::ReportIncident { completes, .. } => Some(completes),
        Activity::Cultivate { completes, .. } => Some(completes),
        Activity::Comfort { completes, .. } => Some(completes),
        Activity::Eat { completes, .. } => Some(completes),
        Activity::Load { completes, .. } => Some(completes),
        Activity::Tell { completes, .. } => Some(completes),
        Activity::Conceive { completes, .. } => Some(completes),
        Activity::Care { completes, .. } => Some(completes),
        Activity::Unbind { completes, .. } => Some(completes),
        Activity::Express { completes, .. } => Some(completes),
        Activity::Play { completes, .. } => Some(completes),
        Activity::Idle | Activity::Rest | Activity::Work { .. } | Activity::Combat { .. } => None,
    }
}
