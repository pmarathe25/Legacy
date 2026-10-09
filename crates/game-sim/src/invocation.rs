//! Invocation retains a separate memory source and a person-bound consent ledger.
//! Fractional burden and physiology use opening-state integer arithmetic.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub activation_burden: u64,
    pub activation_fatigue: u64,
    pub strain_thresholds: [u64; 3],
    pub emotional_thresholds: [u64; 3],
    pub identity_thresholds: [u64; 3],
    pub strain_per_mille: [u64; 4],
    pub health_harm_per_mille: [u64; 4],
    pub loss_threshold: u64,
    pub loss_gap: u64,
    pub loss_duration: SimDuration,
    pub permanent_burden: u64,
    pub permanent_exposure: SimDuration,
    pub permanent_interval: SimDuration,
    pub permanent_identity_per_band: u16,
    pub lifespan_loss_per_band: SimDuration,
    pub burden_recovery_per_mille: u64,
    pub exposure_recovery_per_mille: u64,
    pub controller_quantum: SimDuration,
    pub writing_time: SimDuration,
    pub reading_time: SimDuration,
    pub teaching_time: SimDuration,
    pub moving_time: SimDuration,
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.invocation else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid invocation state".into());
    let lineage = s.lineage.as_ref().ok_or_else(fail)?;
    let physical = s.physical.as_ref().ok_or_else(fail)?;
    if s.decisions.is_none() || s.skills.is_none() {
        return Err(fail());
    }
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let lives: BTreeSet<_> = lineage
        .incarnations
        .iter()
        .map(|life| life.person)
        .collect();
    let areas: BTreeSet<_> = s.areas.iter().map(|a| a.id).collect();
    let r = &w.rules;
    let increasing =
        |values: [u64; 3]| values[0] > 0 && values[0] < values[1] && values[1] < values[2];
    if !increasing(r.strain_thresholds)
        || !increasing(r.emotional_thresholds)
        || !increasing(r.identity_thresholds)
        || r.strain_per_mille
            .iter()
            .chain(r.health_harm_per_mille.iter())
            .any(|v| *v > 1_000_000)
        || r.activation_fatigue == 0
        || r.activation_burden == 0
        || r.loss_threshold == 0
        || r.loss_gap == 0
        || r.loss_duration.0 == 0
        || r.permanent_burden < r.loss_threshold
        || r.permanent_exposure.0 == 0
        || r.permanent_interval.0 == 0
        || r.permanent_identity_per_band == 0
        || r.permanent_identity_per_band > 100
        || r.lifespan_loss_per_band.0 == 0
        || r.lifespan_loss_per_band.0.checked_mul(252).is_none()
        || r.burden_recovery_per_mille == 0
        || r.burden_recovery_per_mille > 1_000_000
        || r.exposure_recovery_per_mille == 0
        || r.exposure_recovery_per_mille > 1_000
        || r.controller_quantum.0 == 0
        || r.controller_quantum > r.loss_duration
        || [
            r.writing_time,
            r.reading_time,
            r.teaching_time,
            r.moving_time,
        ]
        .iter()
        .any(|d| d.0 == 0)
    {
        return Err(fail());
    }
    let valid_lives = |selected: &BTreeSet<PersonId>| {
        !selected.is_empty()
            && selected.len() <= 8
            && selected.iter().all(|id| w.history.contains_key(id))
    };
    let source_valid = |source: &KnowledgeSource| match source {
        KnowledgeSource::Observed(actor) | KnowledgeSource::Reported(actor) => {
            people.contains(actor)
        }
        KnowledgeSource::Recorded(record) => w.records.contains_key(record),
    };
    let fact_valid = |fact: &StockKnowledge| {
        s.inventories
            .get(&fact.inventory)
            .is_some_and(|inventory| inventory.good == fact.good)
            && areas.contains(&fact.area)
            && fact.observed_at <= fact.learned_at
            && fact.learned_at <= s.time
            && source_valid(&fact.source)
    };
    for (id, past) in &w.history {
        if *id != past.person
            || !lineage
                .incarnations
                .iter()
                .any(|life| life.person == *id && life.died_at == Some(past.died_at))
            || past.died_at > s.time
            || physical.actors[id].vitals.alive
            || past
                .mind
                .knowledge
                .values()
                .any(|fact| !fact_valid(fact) || fact.observed_at > past.died_at)
            || past
                .mind
                .relationships
                .keys()
                .any(|id| !people.contains(id))
            || past.mind.routes.iter().any(|(from, links)| {
                !areas.contains(from)
                    || links
                        .iter()
                        .any(|(to, duration)| !areas.contains(to) || duration.0 == 0)
            })
        {
            return Err(fail());
        }
        let mut skills = BTreeSet::new();
        if past
            .skills
            .iter()
            .any(|skill| !skills.insert(skill.skill) || skill.level > 100)
        {
            return Err(fail());
        }
    }
    for (actor, b) in &w.people {
        if !lives.contains(actor)
            || b.burden_remainder >= 1_000
            || b.exposure_remainder >= 1_000
            || b.fatigue_remainder >= 1_000
            || b.health_remainder >= 1_000
            || b.temporary_identity > 15
            || b.emotional_disruption > 300
            || b.permanent_identity > 252 * r.permanent_identity_per_band
            || b.lifespan_loss > r.lifespan_loss_per_band.0 * 252
            || b.uninterrupted > b.exposure
        {
            return Err(fail());
        }
    }
    let consent_valid = |accepted: &BTreeMap<InvocationCost, Consent>| {
        accepted.iter().all(|(kind, c)| {
            c.permanent <= 252
                && c.temporary
                    <= if *kind == InvocationCost::Strain {
                        4
                    } else if *kind == InvocationCost::LossOfControl {
                        1
                    } else {
                        3
                    }
                && (c.permanent == 0
                    || matches!(kind, InvocationCost::Identity | InvocationCost::Lifespan))
        })
    };
    if let Some(a) = &w.active {
        if lineage.current() != Some(a.actor)
            || !valid_lives(&a.lives)
            || !w.people.contains_key(&a.actor)
            || a.started > s.time
            || a.next_loss_burden == 0
            || !consent_valid(&a.accepted)
            || !a.accepted.contains_key(&InvocationCost::Strain)
            || a.dominant.is_some_and(|id| !a.lives.contains(&id))
        {
            return Err(fail());
        }
        if let Some(e) = &a.episode
            && (e.actor != a.actor
                || e.starts > s.time
                || e.ends <= s.time
                || e.ends.0 - e.starts.0 != r.loss_duration.0
                || a.next_controller < s.time
                || a.next_controller > e.ends
                || w.warning.is_some()
                || w.activation.is_some())
        {
            return Err(fail());
        }
    }
    if let Some(plan) = &w.activation
        && (lineage.current() != Some(plan.actor)
            || !valid_lives(&plan.lives)
            || !consent_valid(&plan.accepted)
            || !w.warning.as_ref().is_some_and(|warning| {
                warning.activation.as_ref() == Some(&plan.lives) && warning.view.actor == plan.actor
            }))
    {
        return Err(fail());
    }
    for (id, warning) in &w.warnings {
        if id.0 == 0
            || id.0 >= s.next_id
            || *id != warning.id
            || !lives.contains(&warning.actor)
            || !valid_lives(&warning.lives)
            || warning.severity > 252
            || warning.fatigue_per_mille > 64_000_000
            || warning.health_harm_per_mille > 64_000_000
            || warning.description.len() > 1_024
        {
            return Err(fail());
        }
    }
    if let Some(p) = &w.warning
        && (w.warnings.get(&p.view.id) != Some(&p.view)
            || lineage.current() != Some(p.view.actor)
            || p.activation
                .as_ref()
                .is_some_and(|lives| !valid_lives(lives)))
    {
        return Err(fail());
    }
    let content_valid = |content: &RecordedMemory| match content {
        RecordedMemory::Stock { life, fact } => w.history.contains_key(life) && fact_valid(fact),
        RecordedMemory::Route {
            life,
            origin,
            destination,
            duration,
        } => {
            w.history.contains_key(life)
                && areas.contains(origin)
                && areas.contains(destination)
                && origin != destination
                && duration.0 > 0
        }
    };
    let mut totals: BTreeMap<InventoryId, u64> = BTreeMap::new();
    for (id, record) in &w.records {
        let stock = s.inventories.get(&record.inventory).ok_or_else(fail)?;
        if id.0 == 0
            || id.0 >= s.next_id
            || *id != record.id
            || !people.contains(&record.author)
            || !record.readers.is_subset(&people)
            || record.written_at > s.time
            || !content_valid(&record.content)
            || stock.good != Good::Record
        {
            return Err(fail());
        }
        if record.intact {
            *totals.entry(record.inventory).or_default() += 1_000;
        }
    }
    for stock in s
        .inventories
        .values()
        .filter(|stock| stock.good == Good::Record)
    {
        if stock.quantity.0 != totals.get(&stock.id).copied().unwrap_or(0) {
            return Err(fail());
        }
    }
    for (actor, action) in &w.actions {
        if *actor != action.actor
            || !people.contains(actor)
            || !areas.contains(&action.area)
            || action.completes <= s.time
            || !physical.actors[actor].vitals.alive
        {
            return Err(fail());
        }
        let activity = &physical.actors[actor].activity;
        let valid = match &action.work {
            MemoryWork::Write {
                content,
                material,
                tool,
            } => {
                content_valid(content)
                    && s.inventories
                        .get(material)
                        .is_some_and(|i| i.good == Good::Timber)
                    && s.inventories
                        .get(tool)
                        .is_some_and(|i| i.good == Good::Tool)
                    && matches!(activity,Activity::WritingMemory {completes}if *completes==action.completes)
            }
            MemoryWork::Read { record } => {
                w.records.contains_key(record)
                    && matches!(activity,Activity::ReadingMemory {completes}if *completes==action.completes)
            }
            MemoryWork::Move {
                record,
                destination,
            } => {
                w.records.contains_key(record)
                    && match destination {
                        StockPosition::Area(area) => areas.contains(area),
                        StockPosition::Carried(actor) => people.contains(actor),
                    }
                    && matches!(activity,Activity::MovingRecord {completes}if *completes==action.completes)
            }
            MemoryWork::Teach { content, recipient } => {
                content_valid(content)
                    && people.contains(recipient)
                    && recipient != actor
                    && matches!(activity,Activity::TeachingMemory {completes}if *completes==action.completes)
                    && matches!(physical.actors[recipient].activity,Activity::TeachingMemory {completes}if completes==action.completes)
            }
        };
        if !valid {
            return Err(fail());
        }
    }
    Ok(())
}
impl Rules {
    pub fn human() -> Self {
        let hour = 3_600_000;
        let day = 24 * hour;
        Self {
            activation_burden: 60_000,
            activation_fatigue: 10_000,
            strain_thresholds: [hour, 8 * hour, 2 * day],
            emotional_thresholds: [2 * hour, 12 * hour, 3 * day],
            identity_thresholds: [4 * hour, day, 4 * day],
            strain_per_mille: [0, 50, 250, 2_000],
            health_harm_per_mille: [0, 0, 0, 500],
            loss_threshold: day,
            loss_gap: 6 * hour,
            loss_duration: SimDuration(6 * hour),
            permanent_burden: 30 * day,
            permanent_exposure: SimDuration(90 * day),
            permanent_interval: SimDuration(30 * day),
            permanent_identity_per_band: 10,
            lifespan_loss_per_band: SimDuration(365 * day),
            burden_recovery_per_mille: 1_000,
            exposure_recovery_per_mille: 100,
            controller_quantum: MINUTE,
            writing_time: MINUTE,
            reading_time: MINUTE,
            teaching_time: MINUTE,
            moving_time: MINUTE,
        }
    }
    fn fixture() -> Self {
        Self {
            activation_burden: 6_000,
            activation_fatigue: 1_000,
            strain_thresholds: [120_000, 360_000, 720_000],
            emotional_thresholds: [180_000, 420_000, 780_000],
            identity_thresholds: [240_000, 480_000, 840_000],
            strain_per_mille: [0, 50, 250, 2_000],
            health_harm_per_mille: [0, 0, 0, 500],
            loss_threshold: 300_000,
            loss_gap: 180_000,
            loss_duration: SimDuration(420_000),
            permanent_burden: 600_000,
            permanent_exposure: SimDuration(600_000),
            permanent_interval: SimDuration(300_000),
            permanent_identity_per_band: 10,
            lifespan_loss_per_band: MINUTE,
            burden_recovery_per_mille: 1_000,
            exposure_recovery_per_mille: 100,
            controller_quantum: SimDuration(6_000),
            writing_time: MINUTE,
            reading_time: MINUTE,
            teaching_time: MINUTE,
            moving_time: MINUTE,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Burden {
    pub burden: u64,
    pub exposure: u64,
    pub uninterrupted: u64,
    pub burden_remainder: u16,
    pub exposure_remainder: u16,
    pub fatigue_remainder: u16,
    pub health_remainder: u16,
    pub temporary_identity: u16,
    pub emotional_disruption: u16,
    pub permanent_identity: u16,
    pub lifespan_loss: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastLife {
    pub person: PersonId,
    pub died_at: SimTime,
    pub mind: decisions::Mind,
    pub skills: Vec<SkillView>,
    pub institutions: Vec<InstitutionView>,
    pub cases: Vec<SocialCaseView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub actor: PersonId,
    pub starts: SimTime,
    pub ends: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    pub temporary: u8,
    pub permanent: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Active {
    pub actor: PersonId,
    pub lives: BTreeSet<PersonId>,
    pub started: SimTime,
    pub accepted: BTreeMap<InvocationCost, Consent>,
    pub episode: Option<Episode>,
    pub next_loss_burden: u64,
    pub next_controller: SimTime,
    pub dominant: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub view: InvocationWarning,
    pub activation: Option<BTreeSet<PersonId>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    pub actor: PersonId,
    pub lives: BTreeSet<PersonId>,
    pub accepted: BTreeMap<InvocationCost, Consent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: RecordId,
    pub inventory: InventoryId,
    pub content: RecordedMemory,
    pub author: PersonId,
    pub written_at: SimTime,
    pub readers: BTreeSet<PersonId>,
    pub intact: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryWork {
    Write {
        content: RecordedMemory,
        material: InventoryId,
        tool: InventoryId,
    },
    Read {
        record: RecordId,
    },
    Move {
        record: RecordId,
        destination: StockPosition,
    },
    Teach {
        content: RecordedMemory,
        recipient: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAction {
    pub actor: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub work: MemoryWork,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub history: BTreeMap<PersonId, PastLife>,
    pub people: BTreeMap<PersonId, Burden>,
    pub active: Option<Active>,
    pub activation: Option<Activation>,
    pub warning: Option<Warning>,
    pub warnings: BTreeMap<WarningId, InvocationWarning>,
    pub records: BTreeMap<RecordId, Record>,
    pub actions: BTreeMap<PersonId, MemoryAction>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingInvocation {
    at: SimTime,
    updates: Vec<(PersonId, Burden)>,
    reached_cost: bool,
    end_episode: Option<PersonId>,
}

fn severity(burden: u64, thresholds: [u64; 3]) -> u8 {
    thresholds.iter().filter(|v| burden >= **v).count() as u8
}
fn signed_accumulator(
    value: u64,
    remainder: u16,
    elapsed: u64,
    rate: i128,
    maximum: u64,
) -> (u64, u16) {
    let total = (i128::from(value) * 1_000 + i128::from(remainder) + i128::from(elapsed) * rate)
        .clamp(0, i128::from(maximum) * 1_000);
    ((total / 1_000) as u64, (total % 1_000) as u16)
}

impl Simulation {
    pub fn invocation_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "invocation-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.state.invocation = Some(World {
            rules: Rules::fixture(),
            history: BTreeMap::new(),
            people: BTreeMap::new(),
            active: None,
            activation: None,
            warning: None,
            warnings: BTreeMap::new(),
            records: BTreeMap::new(),
            actions: BTreeMap::new(),
        });
        // Separate compressed fixture rates leave room for sustained-use tests.
        let development = sim.state.development.as_mut().unwrap();
        development.rules.elderhood = SimDuration(3_600_000);
        development.rules.lifespan = SimDuration(5_400_000);
        for due in sim.state.due.values_mut() {
            due.retain(|event| !matches!(event, Due::Age(_)));
        }
        sim.state.due.retain(|_, due| !due.is_empty());
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            sim.schedule_age(actor);
        }
        // This fixture preserves a life that actually traveled, practiced, and
        // died before an independently conceived child's actual birth.
        let old = sim.state.ruler;
        sim.debug_move_actor(old, AreaId(3))?;
        sim.advance(SimDuration(180_000))?;
        let site = sim
            .state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|s| s.area == AreaId(3))
            .unwrap()
            .id;
        sim.debug_physical_command(old, Command::Work { site })?;
        sim.advance(MINUTE)?;
        let damage = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(old, damage)?;
        sim.observe_invocation_deaths()?;
        sim.advance(MINUTE)?;
        Ok(sim)
    }
    pub(crate) fn observe_invocation_deaths(&mut self) -> Result<(), Error> {
        let Some(w) = &self.state.invocation else {
            return Ok(());
        };
        let Some(lineage) = &self.state.lineage else {
            return Ok(());
        };
        let ended: Vec<_> = lineage
            .incarnations
            .iter()
            .filter_map(|life| life.died_at.map(|time| (life.person, time)))
            .filter(|(id, _)| !w.history.contains_key(id))
            .collect();
        let fresh: Vec<_> = lineage
            .incarnations
            .iter()
            .filter(|life| {
                life.died_at == Some(self.state.time) && w.history.contains_key(&life.person)
            })
            .map(|life| life.person)
            .collect();
        for person in fresh {
            let skills = self.own_skill_view(person);
            self.state
                .invocation
                .as_mut()
                .unwrap()
                .history
                .get_mut(&person)
                .unwrap()
                .skills = skills;
        }
        for (person, died_at) in ended {
            let snapshot = PastLife {
                person,
                died_at,
                mind: self.state.decisions.as_ref().unwrap().minds[&person].clone(),
                skills: self.own_skill_view(person),
                institutions: self.institution_view(person),
                cases: self.social_case_view(person),
            };
            self.state
                .invocation
                .as_mut()
                .unwrap()
                .history
                .insert(person, snapshot);
        }
        if self
            .state
            .invocation
            .as_ref()
            .unwrap()
            .warning
            .as_ref()
            .is_some_and(|pending| {
                !self.state.physical.as_ref().unwrap().actors[&pending.view.actor]
                    .vitals
                    .alive
            })
        {
            let w = self.state.invocation.as_mut().unwrap();
            w.warning = None;
            w.activation = None;
        }
        let dead = self
            .state
            .invocation
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .map(|a| a.actor)
            .filter(|actor| {
                !self.state.physical.as_ref().unwrap().actors[actor]
                    .vitals
                    .alive
            });
        if let Some(actor) = dead {
            self.cancel_invocation_action(actor);
            let w = self.state.invocation.as_mut().unwrap();
            w.active = None;
            w.warning = None;
            if let Some(b) = w.people.get_mut(&actor) {
                b.uninterrupted = 0;
            }
        }
        Ok(())
    }
    fn invocation_ready(&self, actor: PersonId) -> bool {
        self.state.lineage.as_ref().is_some_and(|lineage| {
            lineage.current() == Some(actor)
                && lineage
                    .invocation_learning
                    .get(&actor)
                    .copied()
                    .unwrap_or(0)
                    >= lineage.rules.invocation_access_threshold
        }) && self.independent(actor)
            && self
                .state
                .physical
                .as_ref()
                .is_some_and(|w| w.actors[&actor].vitals.alive)
    }
    pub(crate) fn controls_actor(&self, actor: PersonId) -> bool {
        self.state
            .invocation
            .as_ref()
            .and_then(|w| w.active.as_ref())
            .and_then(|a| a.episode.as_ref())
            .is_some_and(|e| {
                e.actor == actor
                    && self.state.time < e.ends
                    && self.state.physical.as_ref().unwrap().actors[&actor]
                        .vitals
                        .alive
            })
    }
    pub(crate) fn required_invocation_input(&self) -> Option<WarningId> {
        self.state
            .invocation
            .as_ref()?
            .warning
            .as_ref()
            .map(|w| w.view.id)
    }
    fn required_bands(
        &self,
        lives: &BTreeSet<PersonId>,
        b: &Burden,
        next_loss: u64,
    ) -> Result<Vec<(InvocationCost, u8, bool)>, Error> {
        let r = &self.state.invocation.as_ref().unwrap().rules;
        let strain = if b.burden >= r.permanent_burden && b.exposure >= r.permanent_exposure.0 {
            4
        } else {
            severity(b.burden, r.strain_thresholds)
        };
        let mut bands = Vec::new();
        if strain > 0 || r.strain_per_mille[0] > 0 {
            bands.push((InvocationCost::Strain, strain, false));
        }
        let emotional = severity(b.burden, r.emotional_thresholds);
        if emotional > 0 {
            bands.push((InvocationCost::Emotional, emotional, false));
        }
        let identity = severity(b.burden, r.identity_thresholds);
        if identity > 0 {
            bands.push((InvocationCost::Identity, identity, false));
        }
        if b.burden >= r.permanent_burden && b.exposure >= r.permanent_exposure.0 {
            let count = 1 + (b.exposure - r.permanent_exposure.0) / r.permanent_interval.0;
            let count = count.min(252) as u8;
            bands.push((InvocationCost::Identity, count, true));
            bands.push((InvocationCost::Lifespan, count, true));
        }
        if b.burden >= next_loss && !lives.is_empty() {
            bands.push((InvocationCost::LossOfControl, 1, false));
        }
        Ok(bands)
    }
    fn warning_spec(
        &self,
        actor: PersonId,
        lives: &BTreeSet<PersonId>,
        cost: InvocationCost,
        band: u8,
        permanent: bool,
    ) -> Result<InvocationWarning, Error> {
        let r = &self.state.invocation.as_ref().unwrap().rules;
        let count = (lives.len() as u64).pow(2);
        let (fatigue, harm) = if cost == InvocationCost::Strain {
            (
                r.strain_per_mille[usize::from(band.min(3))]
                    .checked_mul(count)
                    .ok_or(Error::Overflow)?,
                (if band > 3 {
                    r.health_harm_per_mille[3]
                } else {
                    0
                })
                .checked_mul(count)
                .ok_or(Error::Overflow)?,
            )
        } else {
            (0, 0)
        };
        let identity = if cost == InvocationCost::Identity {
            if permanent {
                u16::from(band) * r.permanent_identity_per_band
            } else {
                u16::from(band) * 5
            }
        } else {
            0
        };
        let lifespan = if cost == InvocationCost::Lifespan {
            r.lifespan_loss_per_band
                .0
                .checked_mul(u64::from(band))
                .ok_or(Error::Overflow)?
        } else {
            0
        };
        Ok(InvocationWarning {id:WarningId(0),actor,lives:lives.clone(),cost,severity:band,activation_burden:0,instant_fatigue:0,fatigue_per_mille:fatigue,health_harm_per_mille:harm,identity_skill_penalty:identity,emotion_disruption:if cost==InvocationCost::Emotional {u16::from(band)*100}else{0},lifespan_loss:SimDuration(lifespan),control_loss_duration:if cost==InvocationCost::LossOfControl {r.loss_duration}else{SimDuration(0)},permanent,description:match cost {InvocationCost::Strain=>"Invocation adds the shown fatigue and injury rates while these lives remain active; ordinary rest can recover strain.".into(),InvocationCost::Emotional=>"Emotional interference changes judgment and impulsiveness; it recovers as burden falls.".into(),InvocationCost::Identity if !permanent=>"Temporary identity interference reduces effective mastery until burden recovers.".into(),InvocationCost::Identity=>"Very protracted exposure permanently impairs this person's identity and effective capabilities. A successor does not inherit this direct penalty.".into(),InvocationCost::Lifespan=>"Very protracted exposure permanently reduces this person's remaining lifespan; already advanced age can make the reduction fatal.".into(),InvocationCost::LossOfControl=>"Past lives direct this body for the exact shown duration. Actions and dismissal are unavailable; all ensuing costs, irreversible harm, and death are authorized during it. Pause, observation, and saving remain available.".into()}})
    }
    fn activation_charge(
        &self,
        actor: PersonId,
        lives: &BTreeSet<PersonId>,
    ) -> Result<(u64, u64), Error> {
        let w = self.state.invocation.as_ref().unwrap();
        let count = (lives.len() as u64).pow(2);
        let burden = w
            .rules
            .activation_burden
            .checked_mul(count)
            .ok_or(Error::Overflow)?;
        let fatigue = w
            .rules
            .activation_fatigue
            .checked_mul(count)
            .ok_or(Error::Overflow)?;
        w.people
            .get(&actor)
            .cloned()
            .unwrap_or_default()
            .burden
            .checked_add(burden)
            .ok_or(Error::Overflow)?;
        if self.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .fatigue
            .checked_add(fatigue)
            .is_none_or(|v| v > self.state.physical.as_ref().unwrap().rules.fatigue_limit)
        {
            return Err(Error::Unavailable);
        }
        Ok((burden, fatigue))
    }
    fn prepare_invocation_warning(
        &mut self,
        activation: Option<BTreeSet<PersonId>>,
    ) -> Result<(), Error> {
        if self.state.invocation.as_ref().unwrap().warning.is_some() {
            return Ok(());
        }
        if let Some(lives) = activation {
            self.state
                .invocation
                .as_mut()
                .unwrap()
                .activation
                .get_or_insert(Activation {
                    actor: self.state.ruler,
                    lives,
                    accepted: BTreeMap::new(),
                });
        }
        let w = self.state.invocation.as_ref().unwrap();
        let planned = w.activation.clone();
        let (actor, lives, accepted, next_loss) = if let Some(plan) = &planned {
            (
                plan.actor,
                plan.lives.clone(),
                plan.accepted.clone(),
                w.rules.loss_threshold,
            )
        } else {
            let Some(a) = &w.active else {
                return Ok(());
            };
            if a.episode.as_ref().is_some_and(|e| self.state.time < e.ends) {
                return Ok(());
            }
            (
                a.actor,
                a.lives.clone(),
                a.accepted.clone(),
                a.next_loss_burden,
            )
        };
        let mut b = w.people.get(&actor).cloned().unwrap_or_default();
        if planned.is_some() {
            b.burden = b
                .burden
                .checked_add(self.activation_charge(actor, &lives)?.0)
                .ok_or(Error::Overflow)?;
        }
        let upfront = planned.is_some() && !accepted.contains_key(&InvocationCost::Strain);
        let next = if upfront {
            Some((InvocationCost::Strain, 0, false))
        } else {
            self.required_bands(&lives, &b, next_loss)?
                .into_iter()
                .find(|(kind, band, permanent)| {
                    accepted.get(kind).is_none_or(|old| {
                        if *permanent {
                            old.permanent < *band
                        } else {
                            old.temporary < *band
                        }
                    }) || planned.is_none() && *kind == InvocationCost::LossOfControl
                })
        };
        if let Some((cost, band, permanent)) = next {
            self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
            self.ensure_event_capacity(1)?;
            let mut view = self.warning_spec(actor, &lives, cost, band, permanent)?;
            if upfront {
                let (burden, fatigue) = self.activation_charge(actor, &lives)?;
                view.activation_burden = burden;
                view.instant_fatigue = fatigue;
                view.description="Activation pays the shown fatigue and adds persistent burden before any borrowed benefit begins. Full payment is required; dismissal does not refund it.".into();
            }
            view.id = WarningId(allocate(&mut self.state)?);
            let w = self.state.invocation.as_mut().unwrap();
            w.warnings.insert(view.id, view.clone());
            w.warning = Some(Warning {
                view: view.clone(),
                activation: planned.map(|p| p.lives),
            });
            self.emit(EventKind::InvocationWarningRaised {
                warning: view.id,
                actor,
            });
        } else if let Some(plan) = planned {
            self.finalize_invocation_activation(plan)?;
        }
        Ok(())
    }
    pub(crate) fn invocation_command(
        &mut self,
        actor: PersonId,
        action: &InvocationAction,
    ) -> Result<(), Error> {
        let w = self.state.invocation.as_ref().ok_or(Error::Unavailable)?;
        if self.controls_actor(actor)
            || !self
                .state
                .physical
                .as_ref()
                .unwrap()
                .actors
                .get(&actor)
                .is_some_and(|a| a.vitals.alive && a.vitals.health > 0)
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(8)?;
        self.state.next_id.checked_add(4).ok_or(Error::Overflow)?;
        match action {
            InvocationAction::Activate { lives } => {
                if !self.invocation_ready(actor)
                    || actor != self.state.ruler
                    || lives.is_empty()
                    || lives.len() > 8
                    || lives.iter().any(|id| !w.history.contains_key(id))
                    || w.warning.is_some()
                    || w.active.as_ref().is_some_and(|a| a.lives == *lives)
                {
                    return Err(Error::Unavailable);
                }
                self.activation_charge(actor, lives)?;
                self.prepare_invocation_warning(Some(lives.clone()))?;
            }
            InvocationAction::Dismiss => {
                if !w.active.as_ref().is_some_and(|a| a.actor == actor)
                    && !w.warning.as_ref().is_some_and(|a| a.view.actor == actor)
                {
                    return Err(Error::Unavailable);
                }
                self.cancel_recollection_actions(actor);
                self.dismiss_recalled_route(actor);
                let w = self.state.invocation.as_mut().unwrap();
                w.active = None;
                w.activation = None;
                w.warning = None;
                if let Some(b) = w.people.get_mut(&actor) {
                    b.uninterrupted = 0;
                }
                self.emit(EventKind::InvocationDismissed { actor });
            }
            InvocationAction::AcceptWarning { warning } => {
                let pending = w
                    .warning
                    .as_ref()
                    .filter(|p| p.view.id == *warning && p.view.actor == actor)
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                if pending.view.cost == InvocationCost::LossOfControl {
                    self.state
                        .time
                        .0
                        .checked_add(w.rules.loss_duration.0)
                        .ok_or(Error::Overflow)?;
                    w.people
                        .get(&actor)
                        .cloned()
                        .unwrap_or_default()
                        .burden
                        .checked_add(w.rules.loss_gap)
                        .ok_or(Error::Overflow)?;
                }
                if pending.activation.is_some() {
                    let plan = self
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .activation
                        .as_ref()
                        .ok_or(Error::Unavailable)?
                        .clone();
                    self.activation_charge(actor, &plan.lives)?;
                    let consent = self
                        .state
                        .invocation
                        .as_mut()
                        .unwrap()
                        .activation
                        .as_mut()
                        .unwrap()
                        .accepted
                        .entry(pending.view.cost)
                        .or_default();
                    if pending.view.permanent {
                        consent.permanent = pending.view.severity;
                    } else {
                        consent.temporary = pending.view.severity;
                    }
                    self.state.invocation.as_mut().unwrap().warning = None;
                    self.emit(EventKind::InvocationCostAccepted {
                        warning: *warning,
                        actor,
                    });
                    self.prepare_invocation_warning(None)?;
                    return Ok(());
                }
                self.state.invocation.as_mut().unwrap().warning = None;
                let consent = self
                    .state
                    .invocation
                    .as_mut()
                    .unwrap()
                    .active
                    .as_mut()
                    .unwrap()
                    .accepted
                    .entry(pending.view.cost)
                    .or_default();
                if pending.view.permanent {
                    consent.permanent = pending.view.severity;
                } else {
                    consent.temporary = pending.view.severity;
                }
                self.refresh_temporary_invocation_costs();
                self.emit(EventKind::InvocationCostAccepted {
                    warning: *warning,
                    actor,
                });
                if pending.view.cost == InvocationCost::LossOfControl {
                    self.begin_control_episode(actor)?;
                } else {
                    if pending.view.permanent {
                        self.apply_accepted_permanent_cost(
                            actor,
                            pending.view.cost,
                            pending.view.severity,
                        )?;
                    }
                    self.prepare_invocation_warning(None)?;
                }
            }
            _ => self.start_memory_action(actor, action)?,
        }
        Ok(())
    }
    fn finalize_invocation_activation(&mut self, plan: Activation) -> Result<(), Error> {
        let actor = plan.actor;
        let (charge, fatigue) = self.activation_charge(actor, &plan.lives)?;
        if self
            .state
            .invocation
            .as_ref()
            .unwrap()
            .actions
            .get(&actor)
            .is_some_and(|action| match &action.work {
                MemoryWork::Write { content, .. } | MemoryWork::Teach { content, .. } => {
                    !plan.lives.contains(&Self::memory_life(content))
                }
                _ => false,
            })
        {
            self.cancel_recollection_actions(actor);
        }
        let next_loss = self.state.invocation.as_ref().unwrap().rules.loss_threshold;
        let w = self.state.invocation.as_mut().unwrap();
        w.people.entry(actor).or_default().burden += charge;
        w.activation = None;
        w.active = Some(Active {
            actor,
            lives: plan.lives.clone(),
            started: self.state.time,
            accepted: plan.accepted.clone(),
            episode: None,
            next_loss_burden: next_loss,
            next_controller: self.state.time,
            dominant: None,
        });
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .vitals
            .fatigue += fatigue;
        self.emit(EventKind::InvocationStarted {
            actor,
            lives: plan.lives,
        });
        self.emit(EventKind::InvocationCostApplied {
            actor,
            cost: InvocationCost::Strain,
            severity: 0,
            permanent: false,
        });
        self.refresh_temporary_invocation_costs();
        for (cost, consent) in &plan.accepted {
            if consent.permanent > 0 {
                self.apply_accepted_permanent_cost(actor, *cost, consent.permanent)?;
            }
        }
        if plan
            .accepted
            .get(&InvocationCost::LossOfControl)
            .is_some_and(|c| c.temporary > 0)
            && self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .alive
        {
            self.begin_control_episode(actor)?;
        }
        Ok(())
    }
    fn begin_control_episode(&mut self, actor: PersonId) -> Result<(), Error> {
        let r = self.state.invocation.as_ref().unwrap().rules.clone();
        let ends = SimTime(
            self.state
                .time
                .0
                .checked_add(r.loss_duration.0)
                .ok_or(Error::Overflow)?,
        );
        let w = self.state.invocation.as_mut().unwrap();
        let b = w.people.entry(actor).or_default();
        let a = w.active.as_mut().unwrap();
        a.next_loss_burden = b.burden.checked_add(r.loss_gap).ok_or(Error::Overflow)?;
        a.episode = Some(Episode {
            actor,
            starts: self.state.time,
            ends,
        });
        a.next_controller = self.state.time;
        self.emit(EventKind::InvocationControlChanged {
            actor,
            controlled: false,
            until: Some(ends),
        });
        self.review_invocation()?;
        Ok(())
    }
    fn apply_accepted_permanent_cost(
        &mut self,
        actor: PersonId,
        cost: InvocationCost,
        band: u8,
    ) -> Result<(), Error> {
        let r = self.state.invocation.as_ref().unwrap().rules.clone();
        let w = self.state.invocation.as_mut().unwrap();
        let b = w.people.entry(actor).or_default();
        let changed = if cost == InvocationCost::Identity {
            let target = u16::from(band) * r.permanent_identity_per_band;
            let changed = target > b.permanent_identity;
            b.permanent_identity = b.permanent_identity.max(target);
            changed
        } else if cost == InvocationCost::Lifespan {
            let target = r
                .lifespan_loss_per_band
                .0
                .checked_mul(u64::from(band))
                .ok_or(Error::Overflow)?;
            let changed = target > b.lifespan_loss;
            b.lifespan_loss = b.lifespan_loss.max(target);
            changed
        } else {
            false
        };
        if changed {
            self.emit(EventKind::InvocationCostApplied {
                actor,
                cost,
                severity: band,
                permanent: true,
            });
            if cost == InvocationCost::Lifespan {
                self.reschedule_invocation_age(actor);
            }
        }
        Ok(())
    }
    fn reschedule_invocation_age(&mut self, actor: PersonId) {
        let Some(development) = &self.state.development else {
            return;
        };
        let life = &development.lives[&actor];
        let lifespan = development
            .rules
            .lifespan
            .0
            .saturating_sub(self.invocation_lifespan_loss(actor));
        let age = self.state.time.0 - life.born.0 + life.initial_age.0;
        if age >= lifespan {
            self.end_life(actor);
            return;
        }
        for due in self.state.due.values_mut() {
            due.retain(|d| !matches!(d,Due::Age(id)if *id==actor));
        }
        self.state.due.retain(|_, due| !due.is_empty());
        self.schedule_age(actor);
    }
    pub(crate) fn invocation_lifespan_loss(&self, actor: PersonId) -> u64 {
        self.state
            .invocation
            .as_ref()
            .and_then(|w| w.people.get(&actor))
            .map_or(0, |b| b.lifespan_loss)
    }

    fn effective_band(&self, active: &Active, kind: InvocationCost, required: u8) -> u8 {
        if self.controls_actor(active.actor) {
            required
        } else {
            active
                .accepted
                .get(&kind)
                .map_or(0, |consent| consent.temporary.min(required))
        }
    }
    fn invocation_rates(&self, actor: PersonId) -> (u64, u64) {
        let Some(w) = &self.state.invocation else {
            return (0, 0);
        };
        let Some(active) = w.active.as_ref().filter(|a| a.actor == actor) else {
            return (0, 0);
        };
        let b = w.people.get(&actor).cloned().unwrap_or_default();
        let required =
            if b.burden >= w.rules.permanent_burden && b.exposure >= w.rules.permanent_exposure.0 {
                4
            } else {
                severity(b.burden, w.rules.strain_thresholds)
            };
        if !self.controls_actor(actor) && !active.accepted.contains_key(&InvocationCost::Strain) {
            return (0, 0);
        }
        let band = self.effective_band(active, InvocationCost::Strain, required);
        let count = (active.lives.len() as u64).pow(2);
        (
            w.rules.strain_per_mille[usize::from(band.min(3))] * count,
            (if band > 3 {
                w.rules.health_harm_per_mille[3]
            } else {
                0
            }) * count,
        )
    }
    fn joint_vitals_and_remainders(
        &self,
        actor: PersonId,
        elapsed: u64,
    ) -> Result<Option<(Vitals, u16, u16)>, Error> {
        let Some(w) = &self.state.invocation else {
            return Ok(None);
        };
        let state = w.people.get(&actor).cloned().unwrap_or_default();
        let (strain, injury) = self.invocation_rates(actor);
        if strain == 0 && injury == 0 && state.fatigue_remainder == 0 && state.health_remainder == 0
        {
            return Ok(None);
        }
        let physical = self.state.physical.as_ref().unwrap();
        let body = &physical.actors[&actor];
        let person = &self.state.people[self.person_index[&actor]];
        if !body.vitals.alive {
            return Ok(None);
        }
        let mut vitals = body.vitals.clone();
        let hunger_before = vitals.hunger;
        let hunger_harm = if person.hunger_exempt {
            0
        } else {
            vitals.hunger = vitals.hunger.checked_add(elapsed).ok_or(Error::Overflow)?;
            vitals.hunger.saturating_sub(physical.rules.hunger_harm)
                - hunger_before.saturating_sub(physical.rules.hunger_harm)
        };
        let exerting = matches!(
            body.activity,
            Activity::Work { .. }
                | Activity::Gather { .. }
                | Activity::Construct { .. }
                | Activity::Load { .. }
                | Activity::Take { .. }
                | Activity::Combat { .. }
                | Activity::Play { .. }
                | Activity::Cultivate { .. }
        ) || matches!(person.location, Location::InTransit { .. });
        let base_fatigue = if exerting {
            1_000
        } else if body.activity == Activity::Rest {
            -2_000
        } else {
            0
        };
        let (fatigue, fraction) = signed_accumulator(
            vitals.fatigue,
            state.fatigue_remainder,
            elapsed,
            base_fatigue + i128::from(strain),
            physical.rules.fatigue_limit,
        );
        vitals.fatigue = fatigue;
        let healing = if person.hunger_exempt {
            elapsed
        } else if body.activity == Activity::Rest {
            elapsed.min(physical.rules.hunger_harm.saturating_sub(hunger_before))
        } else {
            0
        };
        let total = i128::from(vitals.health) * 1_000
            + i128::from(state.health_remainder)
            + i128::from(healing) * 1_000
            - i128::from(hunger_harm) * 1_000
            - i128::from(elapsed) * i128::from(injury);
        let maximum = i128::from(physical.rules.max_health) * 1_000;
        let total = total.clamp(0, maximum);
        vitals.health = (total / 1_000) as u64;
        let health_fraction = (total % 1_000) as u16;
        if vitals.health == 0 && !person.hunger_exempt {
            vitals.alive = false;
        }
        Ok(Some((vitals, fraction, health_fraction)))
    }
    pub(crate) fn joint_invocation_vitals(
        &self,
        actor: PersonId,
        elapsed: u64,
    ) -> Result<Option<Vitals>, Error> {
        Ok(self
            .joint_vitals_and_remainders(actor, elapsed)?
            .map(|(vitals, _, _)| vitals))
    }

    pub(crate) fn evaluate_invocation(&self, next: SimTime) -> Result<PendingInvocation, Error> {
        let elapsed = next
            .0
            .checked_sub(self.state.time.0)
            .ok_or(Error::Overflow)?;
        let Some(w) = &self.state.invocation else {
            return Ok(PendingInvocation {
                at: next,
                updates: Vec::new(),
                reached_cost: false,
                end_episode: None,
            });
        };
        let mut updates = Vec::new();
        for (actor, before) in &w.people {
            if !self.state.physical.as_ref().unwrap().actors[actor]
                .vitals
                .alive
            {
                continue;
            }
            let mut b = before.clone();
            if let Some(active) = w.active.as_ref().filter(|a| a.actor == *actor) {
                let rate = (active.lives.len() as u64).pow(2);
                b.burden = b
                    .burden
                    .checked_add(elapsed.checked_mul(rate).ok_or(Error::Overflow)?)
                    .ok_or(Error::Overflow)?;
                b.exposure = b.exposure.checked_add(elapsed).ok_or(Error::Overflow)?;
                b.uninterrupted = b
                    .uninterrupted
                    .checked_add(elapsed)
                    .ok_or(Error::Overflow)?;
                // Effects apply at the opening band. Reaching a new band raises
                // a warning before a subsequent interval can apply its cost.
                b.temporary_identity = u16::from(self.effective_band(
                    active,
                    InvocationCost::Identity,
                    severity(before.burden, w.rules.identity_thresholds),
                )) * 5;
                b.emotional_disruption = u16::from(self.effective_band(
                    active,
                    InvocationCost::Emotional,
                    severity(before.burden, w.rules.emotional_thresholds),
                )) * 100;
            } else {
                let (burden, fraction) = signed_accumulator(
                    b.burden,
                    b.burden_remainder,
                    elapsed,
                    -i128::from(w.rules.burden_recovery_per_mille),
                    u64::MAX,
                );
                b.burden = burden;
                b.burden_remainder = fraction;
                let (exposure, fraction) = signed_accumulator(
                    b.exposure,
                    b.exposure_remainder,
                    elapsed,
                    -i128::from(w.rules.exposure_recovery_per_mille),
                    u64::MAX,
                );
                b.exposure = exposure;
                b.exposure_remainder = fraction;
                b.uninterrupted = 0;
                b.temporary_identity = b
                    .temporary_identity
                    .min(u16::from(severity(b.burden, w.rules.identity_thresholds)) * 5);
                b.emotional_disruption = b
                    .emotional_disruption
                    .min(u16::from(severity(b.burden, w.rules.emotional_thresholds)) * 100);
            }
            if let Some((_, fatigue, health)) = self.joint_vitals_and_remainders(*actor, elapsed)? {
                b.fatigue_remainder = fatigue;
                b.health_remainder = health;
            }
            updates.push((*actor, b));
        }
        let end_episode = w
            .active
            .as_ref()
            .and_then(|a| a.episode.as_ref())
            .filter(|e| next >= e.ends)
            .map(|e| e.actor);
        self.state
            .next_id
            .checked_add(self.invocation_allocation_bound(next))
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(16 + w.actions.len() * 3)?;
        Ok(PendingInvocation {
            at: next,
            updates,
            reached_cost: true,
            end_episode,
        })
    }
    pub(crate) fn invocation_allocation_bound(&self, next: SimTime) -> u64 {
        let Some(w) = &self.state.invocation else {
            return 0;
        };
        let actions = w
            .actions
            .values()
            .filter(|action| action.completes <= next)
            .map(|action| match action.work {
                MemoryWork::Write { .. } => 2,
                MemoryWork::Move { .. } => 1,
                _ => 0,
            })
            .sum::<u64>();
        let warning = u64::from(w.active.is_some() || w.activation.is_some());
        let controller = if w
            .active
            .as_ref()
            .is_some_and(|a| a.episode.is_some() && a.next_controller <= next)
        {
            2
        } else {
            0
        };
        actions + warning + controller
    }
    pub(crate) fn commit_invocation(&mut self, pending: PendingInvocation) -> Result<(), Error> {
        if self.state.invocation.is_none() {
            return Ok(());
        }
        debug_assert_eq!(pending.at, self.state.time);
        for (actor, b) in pending.updates {
            self.state
                .invocation
                .as_mut()
                .unwrap()
                .people
                .insert(actor, b);
        }
        self.refresh_temporary_invocation_costs();
        let active = self.state.invocation.as_ref().unwrap().active.clone();
        if let Some(a) = active
            && self.controls_actor(a.actor)
        {
            let b = self.state.invocation.as_ref().unwrap().people[&a.actor].clone();
            for (cost, band, permanent) in self.required_bands(&a.lives, &b, a.next_loss_burden)? {
                if permanent {
                    self.apply_accepted_permanent_cost(a.actor, cost, band)?;
                }
            }
        }
        if let Some(actor) = pending.end_episode {
            if let Some(a) = self
                .state
                .invocation
                .as_mut()
                .unwrap()
                .active
                .as_mut()
                .filter(|a| a.actor == actor)
            {
                a.episode = None;
                a.dominant = None;
            }
            self.emit(EventKind::InvocationControlChanged {
                actor,
                controlled: true,
                until: None,
            });
        }
        self.finish_memory_actions()?;
        self.observe_invocation_deaths()?;
        if pending.reached_cost {
            self.prepare_invocation_warning(None)?;
        }
        Ok(())
    }
    fn refresh_temporary_invocation_costs(&mut self) {
        let Some(w) = &self.state.invocation else {
            return;
        };
        let Some(a) = w.active.clone() else { return };
        let b = w.people.get(&a.actor).cloned().unwrap_or_default();
        let identity = u16::from(self.effective_band(
            &a,
            InvocationCost::Identity,
            severity(b.burden, w.rules.identity_thresholds),
        )) * 5;
        let emotion = u16::from(self.effective_band(
            &a,
            InvocationCost::Emotional,
            severity(b.burden, w.rules.emotional_thresholds),
        )) * 100;
        let b = self
            .state
            .invocation
            .as_mut()
            .unwrap()
            .people
            .entry(a.actor)
            .or_default();
        b.temporary_identity = b.temporary_identity.max(identity);
        b.emotional_disruption = b.emotional_disruption.max(emotion);
    }
    fn next_cost_after(&self, actor: PersonId, lives: &BTreeSet<PersonId>) -> Option<SimDuration> {
        let w = self.state.invocation.as_ref()?;
        let b = w.people.get(&actor).cloned().unwrap_or_default();
        let rate = (lives.len() as u64).pow(2);
        if rate == 0 {
            return None;
        }
        let mut candidates = Vec::new();
        for threshold in w
            .rules
            .strain_thresholds
            .into_iter()
            .chain(w.rules.emotional_thresholds)
            .chain(w.rules.identity_thresholds)
            .chain([w.rules.permanent_burden])
        {
            if threshold > b.burden {
                candidates.push((threshold - b.burden).div_ceil(rate));
            }
        }
        let next_loss = w
            .active
            .as_ref()
            .filter(|a| a.actor == actor)
            .map_or(w.rules.loss_threshold, |a| a.next_loss_burden);
        if next_loss > b.burden {
            candidates.push((next_loss - b.burden).div_ceil(rate));
        }
        if b.exposure < w.rules.permanent_exposure.0 {
            candidates.push(w.rules.permanent_exposure.0 - b.exposure);
        } else {
            let next = w.rules.permanent_exposure.0.checked_add(
                ((b.exposure - w.rules.permanent_exposure.0) / w.rules.permanent_interval.0 + 1)
                    .checked_mul(w.rules.permanent_interval.0)?,
            )?;
            candidates.push(next - b.exposure);
        }
        candidates
            .into_iter()
            .filter(|v| *v > 0)
            .min()
            .map(SimDuration)
    }
    pub(crate) fn invocation_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.state.invocation else {
            return limit;
        };
        let mut next = limit;
        for (actor, b) in &w.people {
            if w.active.as_ref().is_some_and(|a| a.actor == *actor)
                || !self.state.physical.as_ref().unwrap().actors[actor]
                    .vitals
                    .alive
                || w.rules.burden_recovery_per_mille == 0
            {
                continue;
            }
            for threshold in w
                .rules
                .identity_thresholds
                .into_iter()
                .chain(w.rules.emotional_thresholds)
            {
                if b.burden >= threshold {
                    let total = u128::from(b.burden) * 1_000 + u128::from(b.burden_remainder);
                    let remaining = total - u128::from(threshold) * 1_000 + 1;
                    let dt = remaining.div_ceil(u128::from(w.rules.burden_recovery_per_mille));
                    if let Ok(dt) = u64::try_from(dt)
                        && let Some(time) = self.state.time.0.checked_add(dt.max(1))
                    {
                        next = next.min(SimTime(time));
                    }
                }
            }
        }
        if let Some(a) = &w.active {
            if let Some(cost) = self.next_cost_after(a.actor, &a.lives)
                && let Some(time) = self.state.time.0.checked_add(cost.0)
            {
                next = next.min(SimTime(time));
            }
            if let Some(e) = &a.episode {
                if e.ends > self.state.time {
                    next = next.min(e.ends);
                }
                if a.next_controller > self.state.time {
                    next = next.min(a.next_controller);
                }
            }
            let body = &self.state.physical.as_ref().unwrap().actors[&a.actor];
            let person = &self.state.people[self.person_index[&a.actor]];
            let (strain, injury) = self.invocation_rates(a.actor);
            let state = w.people.get(&a.actor).cloned().unwrap_or_default();
            let exerting = matches!(
                body.activity,
                Activity::Work { .. }
                    | Activity::Load { .. }
                    | Activity::Take { .. }
                    | Activity::Combat { .. }
                    | Activity::Play { .. }
                    | Activity::Cultivate { .. }
            ) || matches!(person.location, Location::InTransit { .. });
            let fatigue_rate = if exerting {
                1_000_i128
            } else if body.activity == Activity::Rest {
                -2_000
            } else {
                0
            } + i128::from(strain);
            if fatigue_rate > 0
                && body.vitals.fatigue < self.state.physical.as_ref().unwrap().rules.fatigue_limit
            {
                let remaining = u128::from(
                    self.state.physical.as_ref().unwrap().rules.fatigue_limit - body.vitals.fatigue,
                ) * 1_000
                    - u128::from(state.fatigue_remainder);
                let dt = remaining.div_ceil(fatigue_rate as u128);
                if let Ok(dt) = u64::try_from(dt)
                    && let Some(time) = self.state.time.0.checked_add(dt.max(1))
                {
                    next = next.min(SimTime(time));
                }
            }
            let healing = if person.hunger_exempt
                || body.activity == Activity::Rest
                    && body.vitals.hunger < self.state.physical.as_ref().unwrap().rules.hunger_harm
            {
                1_000_i128
            } else {
                0
            };
            let hunger_harm = if !person.hunger_exempt
                && body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.hunger_harm
            {
                1_000_i128
            } else {
                0
            };
            let damage = i128::from(injury) + hunger_harm - healing;
            if damage > 0 && body.vitals.health > 0 {
                let remaining =
                    u128::from(body.vitals.health) * 1_000 + u128::from(state.health_remainder);
                let dt = remaining.div_ceil(damage as u128);
                if let Ok(dt) = u64::try_from(dt)
                    && let Some(time) = self.state.time.0.checked_add(dt.max(1))
                {
                    next = next.min(SimTime(time));
                }
            }
        }
        for action in w.actions.values() {
            if action.completes > self.state.time {
                next = next.min(action.completes);
            }
        }
        next
    }

    pub(crate) fn borrowed_skill_level(&self, actor: PersonId, skill: Skill) -> u16 {
        let Some(w) = &self.state.invocation else {
            return 0;
        };
        let Some(a) = w.active.as_ref().filter(|a| a.actor == actor) else {
            return 0;
        };
        let levels: Vec<_> = a
            .lives
            .iter()
            .map(|id| {
                w.history[id]
                    .skills
                    .iter()
                    .find(|s| s.skill == skill)
                    .map_or(0, |s| s.level)
            })
            .collect();
        let maximum = levels.iter().copied().max().unwrap_or(0);
        (u32::from(maximum)
            + (levels.iter().map(|v| u32::from(*v)).sum::<u32>() - u32::from(maximum)) / 2)
            .min(200) as u16
    }
    pub(crate) fn invocation_work_rate(&self, actor: PersonId, own: SkillRate) -> SkillRate {
        self.invocation_work_rate_for(actor, Skill::Craft, own)
    }
    pub(crate) fn invocation_work_rate_for(
        &self,
        actor: PersonId,
        skill: Skill,
        own: SkillRate,
    ) -> SkillRate {
        let Some(w) = &self.state.invocation else {
            return own;
        };
        let state = w.people.get(&actor).cloned().unwrap_or_default();
        let bonus = self
            .state
            .skills
            .as_ref()
            .map_or(10, |s| s.rules.work_bonus_per_level);
        let borrowed =
            1_000 + u64::from(self.borrowed_skill_level(actor, skill)) * u64::from(bonus);
        let borrowing = w.active.as_ref().is_some_and(|a| a.actor == actor);
        let numerator = if borrowing {
            own.numerator.max(borrowed)
        } else {
            own.numerator
        };
        let penalty = u64::from(state.temporary_identity) + u64::from(state.permanent_identity);
        SkillRate {
            numerator: numerator.saturating_sub(penalty * 5).max(1),
            denominator: own.denominator,
        }
    }
    pub(crate) fn invocation_combat_damage(
        &self,
        actor: PersonId,
        base: u64,
    ) -> Result<u64, Error> {
        let Some(w) = &self.state.invocation else {
            return Ok(base);
        };
        let state = w.people.get(&actor).cloned().unwrap_or_default();
        let own = self.own_skill_level(actor, Skill::Combat);
        let borrowed = self.borrowed_skill_level(actor, Skill::Combat);
        let bonus = self
            .state
            .skills
            .as_ref()
            .map_or(5, |s| s.rules.damage_bonus_per_level);
        let penalty = u64::from(state.temporary_identity) + u64::from(state.permanent_identity);
        let denominator = 1_000 + u64::from(own) * u64::from(bonus);
        let multiplier = (1_000 + u64::from(borrowed.max(own)) * u64::from(bonus))
            .saturating_sub(penalty * 5)
            .max(1);
        u64::try_from(u128::from(base) * u128::from(multiplier) / u128::from(denominator))
            .map_err(|_| Error::Overflow)
    }
    pub(crate) fn invocation_routes(
        &self,
        actor: PersonId,
    ) -> Option<BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>> {
        let w = self.state.invocation.as_ref()?;
        let a = w.active.as_ref().filter(|a| a.actor == actor)?;
        let mut routes = self
            .state
            .decisions
            .as_ref()?
            .minds
            .get(&actor)?
            .routes
            .clone();
        for id in &a.lives {
            for (from, links) in &w.history[id].mind.routes {
                for (to, duration) in links {
                    routes
                        .entry(*from)
                        .or_default()
                        .entry(*to)
                        .or_insert(*duration);
                }
            }
        }
        Some(routes)
    }
    fn dismiss_recalled_route(&mut self, actor: PersonId) {
        let person = &self.state.people[self.person_index[&actor]];
        let Some(destination) = person.route.last().copied() else {
            return;
        };
        let origin = match person.location {
            Location::AtArea(area) => area,
            Location::InTransit { to, .. } => to,
        };
        let owned = &self.state.decisions.as_ref().unwrap().minds[&actor].routes;
        let route = decisions::known_path(owned, origin, destination)
            .map_or_else(|_| Vec::new(), |(path, _)| path);
        self.state.people[self.person_index[&actor]].route = route;
    }
    pub(crate) fn invocation_personality(&self, actor: PersonId) -> Option<decisions::Personality> {
        let w = self.state.invocation.as_ref()?;
        let a = w.active.as_ref().filter(|a| a.actor == actor)?;
        let life = a.dominant.or_else(|| a.lives.iter().next().copied())?;
        let mut p = w.history[&life].mind.personality.clone();
        let b = w.people.get(&actor).cloned().unwrap_or_default();
        p.discipline = p.discipline.saturating_sub(
            b.temporary_identity
                .saturating_add(b.permanent_identity)
                .saturating_mul(5),
        );
        p.aggression = p
            .aggression
            .saturating_add(b.emotional_disruption)
            .min(1_000);
        p.patience = p.patience.saturating_sub(b.emotional_disruption);
        Some(p)
    }
    pub(crate) fn invocation_social_penalty(&self, actor: PersonId) -> i64 {
        self.state
            .invocation
            .as_ref()
            .and_then(|w| w.people.get(&actor))
            .map_or(0, |b| {
                i64::from(b.emotional_disruption)
                    + i64::from(b.temporary_identity)
                    + i64::from(b.permanent_identity)
            })
    }
    pub(crate) fn review_invocation(&mut self) -> Result<(), Error> {
        let Some(a) = self
            .state
            .invocation
            .as_ref()
            .and_then(|w| w.active.as_ref())
            .cloned()
        else {
            return Ok(());
        };
        if !self.controls_actor(a.actor) || a.next_controller > self.state.time {
            return Ok(());
        }
        let w = self.state.invocation.as_ref().unwrap();
        let episode = a.episode.as_ref().unwrap();
        let epoch = (self.state.time.0 - episode.starts.0) / w.rules.controller_quantum.0;
        let index = epoch as usize % a.lives.len();
        let life = *a.lives.iter().nth(index).unwrap();
        let mind = w.history[&life].mind.clone();
        let quantum = w.rules.controller_quantum.0;
        let next = SimTime(
            self.state
                .time
                .0
                .checked_add(quantum)
                .ok_or(Error::Overflow)?
                .min(episode.ends.0),
        );
        let changed = a.dominant.is_some_and(|old| old != life);
        let active = self
            .state
            .invocation
            .as_mut()
            .unwrap()
            .active
            .as_mut()
            .unwrap();
        active.dominant = Some(life);
        active.next_controller = next;
        if self.in_combat(a.actor) {
            return Ok(());
        }
        let person = &self.state.people[self.person_index[&a.actor]];
        if !self.independent(a.actor) {
            return Ok(());
        }
        if !matches!(person.location, Location::AtArea(_)) {
            if changed && mind.dream == decisions::Dream::Comfort {
                let _ = self.debug_physical_command(a.actor, Command::Cancel);
            }
            return Ok(());
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&a.actor];
        let traits = self.invocation_personality(a.actor).unwrap();
        if !changed
            && matches!(
                body.activity,
                Activity::Eat { .. }
                    | Activity::Load { .. }
                    | Activity::Care { .. }
                    | Activity::Tell { .. }
                    | Activity::Lesson { .. }
                    | Activity::WritingMemory { .. }
                    | Activity::ReadingMemory { .. }
                    | Activity::TeachingMemory { .. }
                    | Activity::MovingRecord { .. }
            )
        {
            return Ok(());
        }
        let area = if let Location::AtArea(area) = person.location {
            area
        } else {
            unreachable!()
        };
        let current = &self.state.decisions.as_ref().unwrap().minds[&a.actor];
        let mut knowledge = mind.knowledge.clone();
        knowledge.extend(
            current
                .knowledge
                .iter()
                .map(|(id, fact)| (*id, fact.clone())),
        );
        let mut sites = mind.sites.clone();
        sites.extend(current.sites.iter().map(|(id, fact)| (*id, fact.clone())));
        let command = if body.vitals.hunger
            >= self.state.physical.as_ref().unwrap().rules.meal_relief
            || mind.goal == decisions::Goal::Survival
                && body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.meal_relief / 3
        {
            knowledge
                .values()
                .filter(|k| k.good == Good::Food && k.accessible && k.quantity.0 >= 1_000)
                .filter_map(|k| {
                    if k.area == area {
                        Some((
                            0,
                            Command::Eat {
                                source: k.inventory,
                            },
                        ))
                    } else {
                        self.invocation_routes(a.actor)
                            .and_then(|routes| decisions::known_path(&routes, area, k.area).ok())
                            .map(|(_, duration)| {
                                (
                                    duration.0,
                                    Command::Move {
                                        destination: k.area,
                                    },
                                )
                            })
                    }
                })
                .min_by_key(|(cost, _)| *cost)
                .map(|(_, command)| command)
                .unwrap_or(Command::Rest)
        } else if body.vitals.fatigue
            > self.state.physical.as_ref().unwrap().rules.fatigue_limit / 2
            || mind.dream == decisions::Dream::Comfort
            || traits.hedonism > traits.discipline
        {
            Command::Rest
        } else if traits.aggression >= 800 && traits.risk_tolerance >= 600 {
            self.state
                .people
                .iter()
                .filter(|p| {
                    p.id != a.actor
                        && p.location == Location::AtArea(area)
                        && self.state.physical.as_ref().unwrap().actors[&p.id]
                            .vitals
                            .alive
                })
                .filter(|p| mind.relationships.get(&p.id).is_some_and(|r| r.trust < 0))
                .take(64)
                .map(|p| Command::Engage { target: p.id })
                .next()
                .unwrap_or(Command::Rest)
        } else if (mind.dream == decisions::Dream::SupportOthers
            || mind.goal == decisions::Goal::Care)
            && traits.empathy >= 500
        {
            current
                .child_signals
                .iter()
                .filter(|(_, signal)| signal.area == area && signal.urgency > 0)
                .find_map(|(child, _)| {
                    knowledge
                        .values()
                        .find(|k| {
                            k.area == area
                                && k.good == Good::Food
                                && k.accessible
                                && k.quantity.0 >= 1_000
                        })
                        .map(|k| Command::Care {
                            child: *child,
                            source: k.inventory,
                        })
                })
                .unwrap_or(Command::Rest)
        } else {
            sites
                .values()
                .filter(|s| s.access)
                .filter(|s| {
                    knowledge
                        .get(&s.input)
                        .is_some_and(|k| k.accessible && k.quantity >= s.input_quantity)
                        && knowledge
                            .get(&s.tool)
                            .is_some_and(|k| k.accessible && k.quantity.0 >= 1_000)
                })
                .map(|s| {
                    if s.area == area {
                        Command::Work { site: s.site }
                    } else {
                        Command::Move {
                            destination: s.area,
                        }
                    }
                })
                .next()
                .unwrap_or(Command::Rest)
        };
        if !changed
            && matches!((&body.activity,&command),(Activity::Work {site:a},Command::Work {site:b})if a==b)
        {
            return Ok(());
        }
        let result = match command {
            Command::Move { destination } => self.start_movement(a.actor, destination),
            Command::Engage { target } => self
                .debug_start_encounter(vec![(a.actor, 0), (target, 1)])
                .map(|_| ()),
            command => self.debug_physical_command(a.actor, command),
        };
        if result.is_err() {
            let _ = self.debug_physical_command(a.actor, Command::Rest);
        }
        Ok(())
    }
    pub fn invocation_preview(
        &self,
        lives: BTreeSet<PersonId>,
    ) -> Result<InvocationPreview, Error> {
        let w = self.state.invocation.as_ref().ok_or(Error::Unavailable)?;
        if !self.invocation_ready(self.state.ruler)
            || lives.is_empty()
            || lives.len() > 8
            || lives.iter().any(|id| !w.history.contains_key(id))
        {
            return Err(Error::Unavailable);
        }
        let levels = Skill::ALL
            .into_iter()
            .map(|skill| {
                let values: Vec<_> = lives
                    .iter()
                    .map(|id| {
                        w.history[id]
                            .skills
                            .iter()
                            .find(|s| s.skill == skill)
                            .map_or(0, |s| s.level)
                    })
                    .collect();
                let max = values.iter().copied().max().unwrap_or(0);
                (
                    skill,
                    (u32::from(max)
                        + (values.iter().map(|v| u32::from(*v)).sum::<u32>() - u32::from(max)) / 2)
                        .min(200) as u16,
                )
            })
            .collect();
        Ok(InvocationPreview {
            burden_rate: (lives.len() as u64).pow(2),
            activation_burden: w
                .rules
                .activation_burden
                .checked_mul((lives.len() as u64).pow(2))
                .ok_or(Error::Overflow)?,
            activation_fatigue: w
                .rules
                .activation_fatigue
                .checked_mul((lives.len() as u64).pow(2))
                .ok_or(Error::Overflow)?,
            next_cost_in: self.next_cost_after(self.state.ruler, &lives),
            lives,
            borrowed_skills: levels,
        })
    }
    pub(crate) fn invocation_view(&self) -> Option<InvocationView> {
        let w = self.state.invocation.as_ref()?;
        let actor = self.state.ruler;
        let b = w.people.get(&actor).cloned().unwrap_or_default();
        let active = w.active.as_ref().filter(|a| a.actor == actor);
        let lives = active.map_or_else(BTreeSet::new, |a| a.lives.clone());
        let memories = lives
            .iter()
            .map(|id| {
                let life = &w.history[id];
                InvokedLifeView {
                    person: *id,
                    died_at: life.died_at,
                    skills: life.skills.clone(),
                    stocks: life.mind.knowledge.values().take(64).cloned().collect(),
                    routes: life
                        .mind
                        .routes
                        .iter()
                        .flat_map(|(from, links)| {
                            links.iter().map(|(to, duration)| (*from, *to, *duration))
                        })
                        .take(128)
                        .collect(),
                    institutions: life.institutions.clone(),
                    cases: life.cases.clone(),
                }
            })
            .collect();
        let records=w.records.values().filter_map(|record|{let inventory=self.state.inventories.get(&record.inventory)?;let local=&self.state.people[self.person_index[&actor]].location;let visible=matches!((&inventory.location,local),(InventoryLocation::Carried(id),_)if *id==actor)||matches!((&inventory.location,local),(InventoryLocation::Area(a),Location::AtArea(b))if a==b);if !visible||!record.intact{return None}Some(MemoryRecordView {id:record.id,inventory:record.inventory,content:record.readers.contains(&actor).then(||record.content.clone()),written_at:record.written_at,author:record.author,position:match inventory.location {InventoryLocation::Area(area)=>StockPosition::Area(area),InventoryLocation::Carried(person)=>StockPosition::Carried(person)},can_read:self.can_read_record(actor,record.id)})}).take(64).collect();
        let preview_lives = w
            .activation
            .as_ref()
            .map_or_else(|| lives.clone(), |plan| plan.lives.clone());
        let preview = if preview_lives.is_empty() {
            InvocationPreview {
                lives: BTreeSet::new(),
                burden_rate: 0,
                activation_burden: 0,
                activation_fatigue: 0,
                borrowed_skills: Vec::new(),
                next_cost_in: None,
            }
        } else {
            self.invocation_preview(preview_lives).ok()?
        };
        Some(InvocationView {
            ready: self.invocation_ready(actor),
            available_lives: w.history.keys().copied().collect(),
            active_lives: lives,
            burden: b.burden,
            exposure: SimDuration(b.exposure),
            uninterrupted: SimDuration(b.uninterrupted),
            temporary_identity: b.temporary_identity,
            emotional_disruption: b.emotional_disruption,
            permanent_identity: b.permanent_identity,
            lifespan_loss: SimDuration(b.lifespan_loss),
            player_control: !self.controls_actor(actor)
                && self.state.physical.as_ref().unwrap().actors[&actor]
                    .vitals
                    .alive,
            loss_ends: active.and_then(|a| a.episode.as_ref()).map(|e| e.ends),
            warning: w.warning.as_ref().map(|p| p.view.clone()),
            preview,
            memories,
            records,
        })
    }
    pub(crate) fn available_invocation_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(w) = &self.state.invocation else {
            return Vec::new();
        };
        if self.controls_actor(actor) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        if let Some(warning) = &w.warning
            && warning.view.actor == actor
        {
            actions.push(InvocationAction::AcceptWarning {
                warning: warning.view.id,
            });
        }
        if w.active.as_ref().is_some_and(|a| a.actor == actor)
            || w.warning.as_ref().is_some_and(|a| a.view.actor == actor)
        {
            actions.push(InvocationAction::Dismiss);
        }
        if self.invocation_ready(actor) && w.warning.is_none() {
            for life in w.history.keys().rev().take(32) {
                let lives = BTreeSet::from([*life]);
                if w.active.as_ref().is_none_or(|a| a.lives != lives) {
                    actions.push(InvocationAction::Activate { lives });
                }
            }
        }
        for record in w
            .records
            .values()
            .filter(|record| self.can_read_record(actor, record.id))
            .take(32)
        {
            actions.push(InvocationAction::ReadRecord { record: record.id });
        }
        actions
            .into_iter()
            .map(|action| Command::Invocation { action })
            .collect()
    }

    fn memory_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        if !self.independent(actor) || self.in_combat(actor) || self.combat_restricts(actor) {
            return Err(Error::Unavailable);
        }
        match self.state.people[self.person_index[&actor]].location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn local_memory_input(
        &self,
        actor: PersonId,
        inventory: InventoryId,
        good: Good,
        quantity: u64,
    ) -> bool {
        self.state.inventories.get(&inventory).is_some_and(|stock| {
            stock.good == good
                && stock.quantity.0 >= quantity
                && (good != Good::Timber || self.unreserved_stock(inventory, actor) >= quantity)
                && (good != Good::Tool || self.tool_available_for(actor, inventory))
                && self.has_stock_access(actor, stock)
                && self.inventory_area(stock) == self.memory_area(actor).ok()
        })
    }
    pub(crate) fn invocation_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        match &self.state.invocation.as_ref()?.actions.get(&actor)?.work {
            MemoryWork::Write { tool, .. } => Some(*tool),
            _ => None,
        }
    }
    fn recalled_stock(
        &self,
        actor: PersonId,
        life: PersonId,
        inventory: InventoryId,
    ) -> Result<RecordedMemory, Error> {
        let w = self.state.invocation.as_ref().ok_or(Error::Unavailable)?;
        if !w
            .active
            .as_ref()
            .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
        {
            return Err(Error::Unavailable);
        }
        let fact = w.history[&life]
            .mind
            .knowledge
            .get(&inventory)
            .cloned()
            .ok_or(Error::Unavailable)?;
        Ok(RecordedMemory::Stock { life, fact })
    }
    fn can_read_record(&self, actor: PersonId, record: RecordId) -> bool {
        let Some(w) = &self.state.invocation else {
            return false;
        };
        let Some(record) = w.records.get(&record).filter(|r| r.intact) else {
            return false;
        };
        let Some(stock) = self.state.inventories.get(&record.inventory) else {
            return false;
        };
        self.state
            .physical
            .as_ref()
            .unwrap()
            .actors
            .get(&actor)
            .is_some_and(|a| a.vitals.alive && a.vitals.health > 0)
            && self
                .memory_area(actor)
                .ok()
                .is_some_and(|area| self.inventory_area(stock) == Some(area))
            && stock.good == Good::Record
            && stock.quantity.0 >= 1_000
            && self.has_stock_access(actor, stock)
    }
    fn start_memory_action(
        &mut self,
        actor: PersonId,
        action: &InvocationAction,
    ) -> Result<(), Error> {
        let area = self.memory_area(actor)?;
        let r = self.state.invocation.as_ref().unwrap().rules.clone();
        let (work, duration) = match action {
            InvocationAction::RecordMemory {
                life,
                inventory,
                material,
                tool,
            } => {
                let content = self.recalled_stock(actor, *life, *inventory)?;
                if !self.local_memory_input(actor, *material, Good::Timber, 1_000)
                    || !self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Write {
                        content,
                        material: *material,
                        tool: *tool,
                    },
                    r.writing_time,
                )
            }
            InvocationAction::RecordRoute {
                life,
                origin,
                destination,
                material,
                tool,
            } => {
                let w = self.state.invocation.as_ref().unwrap();
                if !w
                    .active
                    .as_ref()
                    .is_some_and(|a| a.actor == actor && a.lives.contains(life))
                {
                    return Err(Error::Unavailable);
                }
                let duration = w.history[life]
                    .mind
                    .routes
                    .get(origin)
                    .and_then(|links| links.get(destination))
                    .copied()
                    .ok_or(Error::Unavailable)?;
                if !self.local_memory_input(actor, *material, Good::Timber, 1_000)
                    || !self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Write {
                        content: RecordedMemory::Route {
                            life: *life,
                            origin: *origin,
                            destination: *destination,
                            duration,
                        },
                        material: *material,
                        tool: *tool,
                    },
                    r.writing_time,
                )
            }
            InvocationAction::ReadRecord { record } => {
                if !self.can_read_record(actor, *record) {
                    return Err(Error::Unavailable);
                }
                (MemoryWork::Read { record: *record }, r.reading_time)
            }
            InvocationAction::MoveRecord {
                record,
                destination,
            } => {
                if !self.can_read_record(actor, *record) {
                    return Err(Error::Unavailable);
                }
                match destination {
                    StockPosition::Area(target) if *target != area => {
                        return Err(Error::Unavailable);
                    }
                    StockPosition::Carried(person)
                        if self.memory_area(*person).ok() != Some(area) =>
                    {
                        return Err(Error::Unavailable);
                    }
                    _ => {}
                }
                let record_data = &self.state.invocation.as_ref().unwrap().records[record];
                let source = &self.state.inventories[&record_data.inventory];
                let desired = match destination {
                    StockPosition::Area(area) => InventoryLocation::Area(*area),
                    StockPosition::Carried(actor) => InventoryLocation::Carried(*actor),
                };
                if source.location == desired
                    || !matches!(source.location,InventoryLocation::Carried(holder)if holder==actor)
                        && source.location != InventoryLocation::Area(area)
                {
                    return Err(Error::Unavailable);
                }
                if let InventoryLocation::Carried(carrier) = desired {
                    let held: u128 = self
                        .inventory_index
                        .get(&InventoryLocation::Carried(carrier))
                        .into_iter()
                        .flatten()
                        .map(|id| u128::from(self.state.inventories[id].quantity.0))
                        .sum();
                    if held + 1_000
                        > u128::from(
                            self.state
                                .physical
                                .as_ref()
                                .unwrap()
                                .rules
                                .carrying_capacity
                                .0,
                        )
                    {
                        return Err(Error::Unavailable);
                    }
                }
                (
                    MemoryWork::Move {
                        record: *record,
                        destination: destination.clone(),
                    },
                    r.moving_time,
                )
            }
            InvocationAction::TeachMemory {
                life,
                inventory,
                recipient,
            } => {
                let content = self.recalled_stock(actor, *life, *inventory)?;
                if *recipient == actor
                    || self.memory_area(*recipient).ok() != Some(area)
                    || !self.state.physical.as_ref().unwrap().actors[recipient]
                        .vitals
                        .alive
                    || !matches!(
                        self.state.physical.as_ref().unwrap().actors[recipient].activity,
                        Activity::Idle | Activity::Rest
                    )
                {
                    return Err(Error::Unavailable);
                }
                let mind = &self.state.decisions.as_ref().unwrap().minds[recipient];
                let trust = mind.relationships.get(&actor).map_or(0, |r| r.trust);
                if i64::from(mind.personality.sociability) + i64::from(trust) < 200
                    || self.state.physical.as_ref().unwrap().actors[recipient]
                        .vitals
                        .hunger
                        >= self.state.physical.as_ref().unwrap().rules.meal_relief
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Teach {
                        content,
                        recipient: *recipient,
                    },
                    r.teaching_time,
                )
            }
            _ => return Err(Error::Unavailable),
        };
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_physical(actor);
        if let MemoryWork::Write { material, .. } = work {
            self.state
                .inventories
                .get_mut(&material)
                .unwrap()
                .quantity
                .0 -= 1_000;
            self.emit(EventKind::Consumed {
                actor,
                source: material,
                quantity: Quantity(1_000),
            });
        }
        if let MemoryWork::Teach { recipient, .. } = work {
            self.interrupt_physical(recipient);
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&recipient)
                .unwrap()
                .activity = Activity::TeachingMemory { completes };
        }
        let activity = match work {
            MemoryWork::Write { .. } => Activity::WritingMemory { completes },
            MemoryWork::Read { .. } => Activity::ReadingMemory { completes },
            MemoryWork::Move { .. } => Activity::MovingRecord { completes },
            MemoryWork::Teach { .. } => Activity::TeachingMemory { completes },
        };
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = activity;
        self.state.invocation.as_mut().unwrap().actions.insert(
            actor,
            MemoryAction {
                actor,
                area,
                completes,
                work,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_invocation_action(&mut self, actor: PersonId) {
        let Some(w) = &self.state.invocation else {
            return;
        };
        let affected: Vec<_> = w
            .actions
            .values()
            .filter(|action| {
                action.actor == actor
                    || matches!(action.work,MemoryWork::Teach {recipient,..}if recipient==actor)
            })
            .cloned()
            .collect();
        for action in affected {
            self.state
                .invocation
                .as_mut()
                .unwrap()
                .actions
                .remove(&action.actor);
            let mut actors = vec![action.actor];
            if let MemoryWork::Teach { recipient, .. } = action.work {
                actors.push(recipient);
            }
            for id in actors {
                if let Some(body) = self.state.physical.as_mut().unwrap().actors.get_mut(&id)
                    && matches!(
                        body.activity,
                        Activity::WritingMemory { .. }
                            | Activity::ReadingMemory { .. }
                            | Activity::TeachingMemory { .. }
                            | Activity::MovingRecord { .. }
                    )
                {
                    body.activity = Activity::Idle;
                }
            }
        }
    }
    fn cancel_recollection_actions(&mut self, actor: PersonId) {
        if self
            .state
            .invocation
            .as_ref()
            .unwrap()
            .actions
            .get(&actor)
            .is_some_and(|a| matches!(a.work, MemoryWork::Write { .. } | MemoryWork::Teach { .. }))
        {
            self.cancel_invocation_action(actor);
        }
    }
    fn install_recorded_memory(
        &mut self,
        actor: PersonId,
        content: &RecordedMemory,
        source: KnowledgeSource,
    ) {
        let mind = self
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        match content {
            RecordedMemory::Stock { fact, .. } => {
                if mind
                    .knowledge
                    .get(&fact.inventory)
                    .is_some_and(|old| old.observed_at > fact.observed_at)
                {
                    return;
                }
                let mut learned = fact.clone();
                learned.accessible = mind
                    .knowledge
                    .get(&fact.inventory)
                    .is_some_and(|old| old.accessible);
                learned.learned_at = self.state.time;
                learned.source = source;
                mind.knowledge.insert(fact.inventory, learned);
            }
            RecordedMemory::Route {
                origin,
                destination,
                duration,
                ..
            } => {
                mind.routes
                    .entry(*origin)
                    .or_default()
                    .entry(*destination)
                    .or_insert(*duration);
            }
        }
    }
    fn memory_life(content: &RecordedMemory) -> PersonId {
        match content {
            RecordedMemory::Stock { life, .. } | RecordedMemory::Route { life, .. } => *life,
        }
    }
    fn finish_memory_actions(&mut self) -> Result<(), Error> {
        let actions: Vec<_> = self
            .state
            .invocation
            .as_ref()
            .unwrap()
            .actions
            .values()
            .filter(|action| action.completes <= self.state.time)
            .cloned()
            .collect();
        for action in actions {
            let actor = action.actor;
            if self.memory_area(actor).ok() != Some(action.area)
                || !self.state.physical.as_ref().unwrap().actors[&actor]
                    .vitals
                    .alive
            {
                self.cancel_invocation_action(actor);
                continue;
            }
            let mut success = false;
            match &action.work {
                MemoryWork::Write { content, tool, .. } => {
                    let life = Self::memory_life(content);
                    let held: u128 = self
                        .inventory_index
                        .get(&InventoryLocation::Carried(actor))
                        .into_iter()
                        .flatten()
                        .map(|id| u128::from(self.state.inventories[id].quantity.0))
                        .sum();
                    if self
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .active
                        .as_ref()
                        .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
                        && self.local_memory_input(actor, *tool, Good::Tool, 1_000)
                        && held + 1_000
                            <= u128::from(
                                self.state
                                    .physical
                                    .as_ref()
                                    .unwrap()
                                    .rules
                                    .carrying_capacity
                                    .0,
                            )
                    {
                        let record = RecordId(allocate(&mut self.state)?);
                        let inventory = self.debug_add_inventory(
                            Good::Record,
                            InventoryLocation::Carried(actor),
                            Quantity(1_000),
                            Quantity(10_000),
                            BTreeSet::from([actor]),
                        )?;
                        self.state.invocation.as_mut().unwrap().records.insert(
                            record,
                            Record {
                                id: record,
                                inventory,
                                content: content.clone(),
                                author: actor,
                                written_at: self.state.time,
                                readers: BTreeSet::new(),
                                intact: true,
                            },
                        );
                        self.emit(EventKind::MemoryRecorded {
                            record,
                            actor,
                            inventory,
                        });
                        success = true;
                    }
                }
                MemoryWork::Read { record } => {
                    if self.can_read_record(actor, *record) {
                        let content = self.state.invocation.as_ref().unwrap().records[record]
                            .content
                            .clone();
                        self.install_recorded_memory(
                            actor,
                            &content,
                            KnowledgeSource::Recorded(*record),
                        );
                        self.state
                            .invocation
                            .as_mut()
                            .unwrap()
                            .records
                            .get_mut(record)
                            .unwrap()
                            .readers
                            .insert(actor);
                        self.emit(EventKind::MemoryRead {
                            record: *record,
                            actor,
                        });
                        success = true;
                    }
                }
                MemoryWork::Teach { content, recipient } => {
                    let life = Self::memory_life(content);
                    if self
                        .state
                        .invocation
                        .as_ref()
                        .unwrap()
                        .active
                        .as_ref()
                        .is_some_and(|a| a.actor == actor && a.lives.contains(&life))
                        && self.memory_area(*recipient).ok() == Some(action.area)
                        && matches!(self.state.physical.as_ref().unwrap().actors[recipient].activity,Activity::TeachingMemory {completes}if completes==action.completes)
                    {
                        self.install_recorded_memory(
                            *recipient,
                            content,
                            KnowledgeSource::Reported(actor),
                        );
                        self.emit(EventKind::MemoryTaught {
                            actor,
                            recipient: *recipient,
                            life,
                        });
                        success = true;
                    }
                }
                MemoryWork::Move {
                    record,
                    destination,
                } => {
                    if self.can_read_record(actor, *record) {
                        let source =
                            self.state.invocation.as_ref().unwrap().records[record].inventory;
                        let location = match destination {
                            StockPosition::Area(area) => InventoryLocation::Area(*area),
                            StockPosition::Carried(carrier) => InventoryLocation::Carried(*carrier),
                        };
                        let valid = match destination {
                            StockPosition::Area(area) => *area == action.area,
                            StockPosition::Carried(carrier) => {
                                self.memory_area(*carrier).ok() == Some(action.area)
                                    && self
                                        .inventory_index
                                        .get(&location)
                                        .into_iter()
                                        .flatten()
                                        .map(|id| u128::from(self.state.inventories[id].quantity.0))
                                        .sum::<u128>()
                                        + 1_000
                                        <= u128::from(
                                            self.state
                                                .physical
                                                .as_ref()
                                                .unwrap()
                                                .rules
                                                .carrying_capacity
                                                .0,
                                        )
                            }
                        };
                        if valid {
                            let mut access = self.state.inventories[&source].access.clone();
                            if let StockPosition::Carried(carrier) = destination {
                                access.insert(*carrier);
                            }
                            let target = self.debug_add_inventory(
                                Good::Record,
                                location,
                                Quantity(0),
                                Quantity(10_000),
                                access,
                            )?;
                            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= 1_000;
                            self.state.inventories.get_mut(&target).unwrap().quantity.0 += 1_000;
                            let original: Vec<_> = self
                                .state
                                .invocation
                                .as_ref()
                                .unwrap()
                                .records
                                .values()
                                .filter(|r| r.intact && r.inventory == source)
                                .map(|r| r.id)
                                .collect();
                            self.emit(EventKind::Transferred {
                                actor,
                                source,
                                destination: target,
                                quantity: Quantity(1_000),
                            });
                            for id in original {
                                self.state
                                    .invocation
                                    .as_mut()
                                    .unwrap()
                                    .records
                                    .get_mut(&id)
                                    .unwrap()
                                    .inventory = if id == *record { target } else { source };
                            }
                            success = true;
                        }
                    }
                }
            }
            self.cancel_invocation_action(actor);
            if !success {
                self.emit(EventKind::Interrupted { actor });
            }
        }
        Ok(())
    }
    pub(crate) fn record_transfer_allowed(&self, source: InventoryId, quantity: Quantity) -> bool {
        let Some(stock) = self.state.inventories.get(&source) else {
            return false;
        };
        if stock.good != Good::Record {
            return true;
        }
        let Some(w) = &self.state.invocation else {
            return false;
        };
        quantity.0 > 0
            && quantity.0.is_multiple_of(1_000)
            && w.records
                .values()
                .filter(|record| record.intact && record.inventory == source)
                .count() as u64
                >= quantity.0 / 1_000
    }
    pub(crate) fn relocate_record_transfer(
        &mut self,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) {
        if self
            .state
            .inventories
            .get(&source)
            .is_none_or(|s| s.good != Good::Record)
        {
            return;
        }
        let Some(w) = self.state.invocation.as_mut() else {
            return;
        };
        for record in w
            .records
            .values_mut()
            .filter(|record| record.intact && record.inventory == source)
            .take((quantity.0 / 1_000) as usize)
        {
            record.inventory = destination;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn submit(sim: &mut Simulation, command: Command) -> Result<CommandReceipt, Error> {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence()?,
            command,
        })
    }
    fn act(sim: &mut Simulation, action: InvocationAction) -> Result<CommandReceipt, Error> {
        submit(sim, Command::Invocation { action })
    }
    fn ready(workers: usize) -> Simulation {
        let mut sim = Simulation::invocation_fixture(
            9,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        let actor = sim.state.ruler;
        let child = sim.state.development.as_ref().unwrap().lives[&actor].born.0
            + sim.state.development.as_ref().unwrap().rules.infancy.0;
        sim.advance(SimDuration(child - sim.state.time.0)).unwrap();
        let teacher = *sim
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .next()
            .unwrap();
        for _ in 0..2 {
            let mut learned = false;
            for _ in 0..12 {
                if matches!(
                    sim.state.people[sim.person_index[&teacher]].location,
                    Location::AtArea(_)
                ) {
                    sim.debug_physical_command(teacher, Command::Rest).unwrap();
                }
                if sim
                    .start_lesson(actor, teacher, LessonTopic::Invocation)
                    .is_ok()
                    && matches!(
                        sim.state.physical.as_ref().unwrap().actors[&actor].activity,
                        Activity::Lesson { .. }
                    )
                {
                    sim.advance(MINUTE).unwrap();
                    learned = true;
                    break;
                }
                sim.advance(MINUTE).unwrap();
            }
            assert!(
                learned,
                "guardian and pupil must actually complete a lesson"
            );
        }
        assert!(sim.invocation_ready(actor));
        let adult = sim.state.development.as_ref().unwrap().lives[&actor].born.0
            + sim.state.development.as_ref().unwrap().rules.adulthood.0;
        if sim.state.time.0 < adult {
            sim.advance(SimDuration(adult - sim.state.time.0)).unwrap();
        }
        // The fixture's living custodians explicitly permit this person to work;
        // birth itself never supplied these rights or transferred the estate.
        for inventory in sim
            .state
            .inventories
            .values_mut()
            .filter(|i| matches!(i.location, InventoryLocation::Area(_)))
        {
            inventory.access.insert(actor);
        }
        for site in sim.state.physical.as_mut().unwrap().sites.values_mut() {
            site.access.insert(actor);
        }
        let area = match sim.state.people[sim.person_index[&actor]].location {
            Location::AtArea(area) => area,
            _ => panic!("pupil stayed local"),
        };
        sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(area),
            Quantity(100_000),
            Quantity(100_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
        sim.debug_physical_command(actor, Command::Rest).unwrap();
        sim.advance(MINUTE).unwrap();
        sim
    }
    fn past(sim: &Simulation) -> PersonId {
        *sim.state
            .invocation
            .as_ref()
            .unwrap()
            .history
            .keys()
            .next()
            .unwrap()
    }
    fn activate(sim: &mut Simulation, lives: BTreeSet<PersonId>) {
        act(sim, InvocationAction::Activate { lives }).unwrap();
        for _ in 0..16 {
            let Some(warning) = sim.required_invocation_input() else {
                return;
            };
            let kind = sim
                .state
                .invocation
                .as_ref()
                .unwrap()
                .warning
                .as_ref()
                .unwrap()
                .view
                .cost;
            assert_ne!(kind, InvocationCost::LossOfControl);
            act(sim, InvocationAction::AcceptWarning { warning }).unwrap();
        }
        panic!("activation consent did not settle");
    }
    fn approach_loss(sim: &mut Simulation) -> WarningId {
        for _ in 0..20 {
            sim.advance(SimDuration(1_000_000)).unwrap();
            let warning = sim
                .state
                .invocation
                .as_ref()
                .unwrap()
                .warning
                .as_ref()
                .expect("new cost must stop time")
                .view
                .clone();
            if warning.cost == InvocationCost::LossOfControl {
                return warning.id;
            }
            act(
                sim,
                InvocationAction::AcceptWarning {
                    warning: warning.id,
                },
            )
            .unwrap();
        }
        panic!("loss threshold unreachable");
    }
    #[test]
    fn access_requires_real_guidance_and_activation_costs_precede_benefits() {
        let mut sim = Simulation::invocation_fixture(9, Execution::default()).unwrap();
        let life = past(&sim);
        let before = sim.state.clone();
        assert_eq!(
            act(
                &mut sim,
                InvocationAction::Activate {
                    lives: BTreeSet::from([life])
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        let mut sim = ready(1);
        let actor = sim.state.ruler;
        let life = past(&sim);
        let fatigue = sim.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .fatigue;
        act(
            &mut sim,
            InvocationAction::Activate {
                lives: BTreeSet::from([life]),
            },
        )
        .unwrap();
        assert!(sim.state.invocation.as_ref().unwrap().active.is_none());
        assert_eq!(sim.borrowed_skill_level(actor, Skill::Craft), 0);
        let warning = sim
            .state
            .invocation
            .as_ref()
            .unwrap()
            .warning
            .as_ref()
            .unwrap()
            .view
            .clone();
        assert!(warning.instant_fatigue > 0 && warning.activation_burden > 0);
        act(
            &mut sim,
            InvocationAction::AcceptWarning {
                warning: warning.id,
            },
        )
        .unwrap();
        assert!(sim.borrowed_skill_level(actor, Skill::Craft) > 0);
        let burden = sim.state.invocation.as_ref().unwrap().people[&actor].burden;
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .fatigue,
            fatigue + warning.instant_fatigue
        );
        act(&mut sim, InvocationAction::Dismiss).unwrap();
        assert_eq!(
            sim.state.invocation.as_ref().unwrap().people[&actor].burden,
            burden
        );
    }
    #[test]
    fn borrowed_work_is_actual_and_ends_on_dismissal_without_copying_mastery() {
        let mut borrowed = ready(1);
        let actor = borrowed.state.ruler;
        let life = past(&borrowed);
        let site = borrowed
            .state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|s| s.area == AreaId(1))
            .unwrap()
            .id;
        // Compare the same free workstation; existing citizens may already be
        // using its single toolkit under the fixture's ordinary autonomy.
        let other_workers: Vec<_> = borrowed
            .state
            .physical
            .as_ref()
            .unwrap()
            .actors
            .iter()
            .filter(|(id, body)| {
                **id != actor
                    && matches!(body.activity,Activity::Work {site:working}if working==site)
            })
            .map(|(id, _)| *id)
            .collect();
        for other in other_workers {
            borrowed
                .debug_physical_command(other, Command::Rest)
                .unwrap();
        }
        let mut baseline =
            Simulation::load(&borrowed.save().unwrap(), Execution::default()).unwrap();
        activate(&mut borrowed, BTreeSet::from([life]));
        for sim in [&mut borrowed, &mut baseline] {
            submit(sim, Command::Work { site }).unwrap();
            sim.advance(SimDuration(30_000)).unwrap();
        }
        let a = borrowed.state.physical.as_ref().unwrap().actors[&actor]
            .job
            .as_ref()
            .unwrap()
            .remaining;
        let b = baseline.state.physical.as_ref().unwrap().actors[&actor]
            .job
            .as_ref()
            .unwrap()
            .remaining;
        assert!(a < b);
        assert_eq!(
            borrowed.own_skill_level(actor, Skill::Craft),
            baseline.own_skill_level(actor, Skill::Craft)
        );
        act(&mut borrowed, InvocationAction::Dismiss).unwrap();
        borrowed.advance(SimDuration(10_000)).unwrap();
        assert_eq!(
            borrowed.state.physical.as_ref().unwrap().actors[&actor]
                .job
                .as_ref()
                .unwrap()
                .remaining,
            a - 10_000
        );
    }
    #[test]
    fn every_new_cost_stops_before_effect_and_rest_chunking_is_exact() {
        let mut whole = ready(1);
        let actor = whole.state.ruler;
        let life = past(&whole);
        activate(&mut whole, BTreeSet::from([life]));
        let mut sliced = Simulation::load(
            &whole.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        whole.advance(SimDuration(1_000_000)).unwrap();
        for _ in 0..30 {
            if sliced.required_invocation_input().is_some() {
                break;
            }
            sliced.advance(SimDuration(7_919)).unwrap();
        }
        assert_eq!(whole.state, sliced.state);
        let warning = whole
            .state
            .invocation
            .as_ref()
            .unwrap()
            .warning
            .as_ref()
            .unwrap()
            .view
            .clone();
        assert_eq!(warning.cost, InvocationCost::Strain);
        assert_eq!(
            whole.state.invocation.as_ref().unwrap().people[&actor].emotional_disruption,
            0
        );
        for sim in [&mut whole, &mut sliced] {
            act(
                sim,
                InvocationAction::AcceptWarning {
                    warning: warning.id,
                },
            )
            .unwrap();
            submit(sim, Command::Rest).unwrap();
        }
        whole.advance(SimDuration(40_013)).unwrap();
        for amount in [3, 11_117, 28_893] {
            sliced.advance(SimDuration(amount)).unwrap();
        }
        assert_eq!(whole.state, sliced.state);
    }
    #[test]
    fn a_written_record_requires_material_time_and_reading_after_dismissal() {
        let mut sim = ready(1);
        let actor = sim.state.ruler;
        let life = past(&sim);
        let remote = sim.state.invocation.as_ref().unwrap().history[&life]
            .mind
            .knowledge
            .values()
            .find(|k| k.area == AreaId(3) && k.good == Good::Timber)
            .unwrap()
            .inventory;
        assert!(
            !sim.state.decisions.as_ref().unwrap().minds[&actor]
                .knowledge
                .contains_key(&remote)
        );
        let material = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let tool = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(actor),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        activate(&mut sim, BTreeSet::from([life]));
        act(
            &mut sim,
            InvocationAction::RecordMemory {
                life,
                inventory: remote,
                material,
                tool,
            },
        )
        .unwrap();
        assert_eq!(sim.state.inventories[&material].quantity, Quantity(1_000));
        sim.advance(SimDuration(59_999)).unwrap();
        assert!(sim.state.invocation.as_ref().unwrap().records.is_empty());
        sim.advance(SimDuration(1)).unwrap();
        let record = *sim
            .state
            .invocation
            .as_ref()
            .unwrap()
            .records
            .keys()
            .next()
            .unwrap();
        for _ in 0..10 {
            assert!(sim.invocation_view().unwrap().records[0].content.is_none());
        }
        act(&mut sim, InvocationAction::Dismiss).unwrap();
        assert!(sim.invocation_view().unwrap().memories.is_empty());
        assert!(
            !sim.state.decisions.as_ref().unwrap().minds[&actor]
                .knowledge
                .contains_key(&remote)
        );
        act(&mut sim, InvocationAction::ReadRecord { record }).unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(
            matches!(sim.state.decisions.as_ref().unwrap().minds[&actor].knowledge[&remote].source,KnowledgeSource::Recorded(id)if id==record)
        );
        assert!(!sim.state.decisions.as_ref().unwrap().minds[&actor].knowledge[&remote].accessible);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn accepted_control_loss_blocks_actions_and_continues_costs_without_more_consent() {
        let mut sim = ready(1);
        let actor = sim.state.ruler;
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        assert!(sim.controls_actor(actor));
        let ends = sim
            .state
            .invocation
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .episode
            .as_ref()
            .unwrap()
            .ends;
        let before = sim.state.clone();
        assert_eq!(submit(&mut sim, Command::Rest), Err(Error::Unavailable));
        assert_eq!(
            act(&mut sim, InvocationAction::Dismiss),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(ends.0 - sim.state.time.0)).unwrap();
        while restored.state.time < ends {
            restored
                .advance(SimDuration((ends.0 - restored.state.time.0).min(17_711)))
                .unwrap();
        }
        assert_eq!(sim.state, restored.state);
        assert!(!sim.controls_actor(actor));
        assert!(sim.state.invocation.as_ref().unwrap().people[&actor].permanent_identity > 0);
        assert!(sim.state.invocation.as_ref().unwrap().people[&actor].lifespan_loss > 0);
    }
    #[test]
    fn death_during_loss_is_not_undone_and_direct_penalties_do_not_bind_a_successor() {
        let mut sim = ready(1);
        let actor = sim.state.ruler;
        let life = past(&sim);
        activate(&mut sim, BTreeSet::from([life]));
        let warning = approach_loss(&mut sim);
        act(&mut sim, InvocationAction::AcceptWarning { warning }).unwrap();
        let damage = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(actor, damage).unwrap();
        sim.observe_invocation_deaths().unwrap();
        assert!(sim.state.invocation.as_ref().unwrap().active.is_none());
        assert!(
            !sim.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .alive
        );
        let before = sim.state.time;
        for _ in 0..20 {
            sim.advance(MINUTE).unwrap();
            if sim.state.ruler != actor {
                break;
            }
        }
        assert!(sim.state.time > before);
        if sim.state.ruler != actor {
            assert_eq!(sim.invocation_lifespan_loss(sim.state.ruler), 0);
            assert_eq!(
                sim.state
                    .invocation
                    .as_ref()
                    .unwrap()
                    .people
                    .get(&sim.state.ruler)
                    .map_or(0, |b| b.permanent_identity),
                0
            );
            assert!(!sim.controls_actor(sim.state.ruler));
        }
    }
}
