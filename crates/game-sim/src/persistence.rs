use crate::*;

const SAVE_VERSION: u32 = 4;
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
        if ![1, 2, 3, SAVE_VERSION].contains(&header.version) {
            return Err(Error::InvalidSave("unsupported save version".into()));
        }
        let mut save: Save =
            serde_json::from_slice(bytes).map_err(|e| Error::InvalidSave(e.to_string()))?;
        // Version 1 predates development and child observations. Their empty
        // defaults preserve continuation for its three supported fixtures.
        if save.version == 1 && save.state.development.is_some() {
            return Err(Error::InvalidSave("unsupported save version".into()));
        }
        if save.version < 3
            && let Some(world) = &mut save.state.decisions
        {
            for mind in world.minds.values_mut() {
                mind.routes = decisions::fixture_routes(&save.state.areas);
            }
        }
        validate(&save.state)?;
        Self::from_state(save.state, execution, 0)
    }

    pub fn next_caller_sequence(&self) -> Result<u64, Error> {
        self.state
            .accepted
            .last_key_value()
            .map_or(Ok(1), |(seq, _)| seq.checked_add(1).ok_or(Error::Overflow))
    }
}

fn validate(s: &CanonicalState) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    if !matches!(
        s.scenario.id.as_str(),
        "kernel-fixture"
            | "survival-fixture"
            | "autonomy-fixture"
            | "life-fixture"
            | "combat-fixture"
    ) || s.scenario.version != 1
        || !s.scenario.debug_fixture
        || (s.scenario.id != "kernel-fixture") != s.physical.is_some()
        || matches!(
            s.scenario.id.as_str(),
            "autonomy-fixture" | "life-fixture" | "combat-fixture"
        ) != s.decisions.is_some()
        || matches!(s.scenario.id.as_str(), "life-fixture" | "combat-fixture")
            != s.development.is_some()
        || (s.scenario.id == "combat-fixture") != s.combat.is_some()
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
    if let Some(world) = &s.physical {
        for id in world.sites.keys() {
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
    if let Some(world) = &s.combat {
        for id in world
            .maps
            .values()
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
                    if !matches!(p.location, Location::InTransit { arrives, .. } if arrives == *time)
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
                    if !matches!(actor.activity, Activity::Eat { completes, .. } | Activity::Load { completes, .. } | Activity::Tell { completes, .. } | Activity::Care { completes, .. } | Activity::Conceive { completes, .. } | Activity::Unbind { completes, .. } if completes == *time)
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
                    || arrived_duration(departed, arrives) != Some(link.duration.0)
                    || (!dead && (arrives <= s.time || !scheduled_arrivals.contains(&p.id)))
                    || (dead && scheduled_arrivals.contains(&p.id))
                {
                    return Err(fail());
                }
                to
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
                    w.maps
                        .values()
                        .any(|m| m.id == map && m.obstacles.iter().any(|o| o.index == obstacle))
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
            EventKind::Developed { actor, .. } => {
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
                    || !areas.contains(&signal.area)
                    || signal.observed_at > s.time
                    || ![0, 3_000, 20_000].contains(&signal.urgency)
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
    development::validate(s)?;
    combat::validate(s)?;
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
    let mut care_targets = BTreeSet::new();
    for (id, actor) in &world.actors {
        let p = people.get(id).ok_or_else(fail)?;
        let carried = cargo.get(id).copied().unwrap_or(0);
        if actor.vitals.health > rules.max_health
            || actor.vitals.fatigue > rules.fatigue_limit
            || carried > u128::from(rules.carrying_capacity.0)
            || (p.hunger_exempt && (actor.vitals.hunger != 0 || !actor.vitals.alive))
            || (!actor.vitals.alive && actor.vitals.health != 0)
        {
            return Err(fail());
        }
        if !actor.vitals.alive && (actor.activity != Activity::Idle || actor.job.is_some()) {
            return Err(fail());
        }
        if let Some(job) = &actor.job
            && !world
                .sites
                .get(&job.site)
                .is_some_and(|site| job.remaining <= site.duration.0)
        {
            return Err(fail());
        }
        let timed = matches!(
            actor.activity,
            Activity::Eat { .. }
                | Activity::Load { .. }
                | Activity::Tell { .. }
                | Activity::Care { .. }
                | Activity::Conceive { .. }
                | Activity::Unbind { .. }
        );
        if timed != deadlines.contains(id) {
            return Err(fail());
        }
        if actor.activity != Activity::Idle && !matches!(p.location, Location::AtArea(_)) {
            return Err(fail());
        }
        match actor.activity {
            Activity::Unbind { target, .. } if !people.contains_key(&target) => return Err(fail()),
            Activity::Care { child, .. } if !care_targets.insert(child) => return Err(fail()),
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
