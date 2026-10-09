//! Births require chosen actions by two adults. Kinship supplies a relationship,
//! not shared ownership or mandatory care; actual care records the supporting people.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub conception_time: SimDuration,
    pub gestation: SimDuration,
    pub infancy: SimDuration,
    pub adulthood: SimDuration,
    pub elderhood: SimDuration,
    pub lifespan: SimDuration,
}
impl Rules {
    pub fn human() -> Self {
        let day = 86_400_000;
        let year = 365 * day;
        Self {
            conception_time: MINUTE,
            gestation: SimDuration(280 * day),
            infancy: SimDuration(2 * year),
            adulthood: SimDuration(18 * year),
            elderhood: SimDuration(65 * year),
            lifespan: SimDuration(90 * year),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reproduction {
    Gestates,
    SuppliesSeed,
    Neither,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Life {
    pub id: PersonId,
    pub born: SimTime,
    pub initial_age: SimDuration,
    pub stage: LifeStage,
    pub reproduction: Reproduction,
    pub parents: Vec<PersonId>,
    pub caregivers: BTreeSet<PersonId>,
    #[serde(default)]
    pub preference: Option<(ChildhoodInterest, SimTime)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PregnancyState {
    Conceiving { completes: SimTime },
    Gestating { due: SimTime },
    Born(PersonId),
    Ended,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pregnancy {
    pub id: PregnancyId,
    pub gestator: PersonId,
    pub partner: PersonId,
    pub state: PregnancyState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub lives: BTreeMap<PersonId, Life>,
    pub pregnancies: BTreeMap<PregnancyId, Pregnancy>,
}

impl Simulation {
    pub fn life_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.state.scenario = scenarios().remove(3);
        // Accelerated biological rates isolate transitions; these are explicitly
        // distinct from the configurable human setting returned by Rules::human.
        let rules = Rules {
            conception_time: MINUTE,
            gestation: SimDuration(180_000),
            infancy: SimDuration(300_000),
            adulthood: SimDuration(600_000),
            elderhood: SimDuration(1_200_000),
            lifespan: SimDuration(1_500_000),
        };
        let lives = sim
            .state
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id,
                    Life {
                        id: p.id,
                        born: SimTime(0),
                        initial_age: rules.adulthood,
                        stage: LifeStage::Adult,
                        reproduction: if p.role == Role::Guardian {
                            Reproduction::Neither
                        } else if i % 2 == 0 {
                            Reproduction::SuppliesSeed
                        } else {
                            Reproduction::Gestates
                        },
                        parents: Vec::new(),
                        caregivers: BTreeSet::new(),
                        preference: None,
                    },
                )
            })
            .collect();
        sim.state.development = Some(World {
            rules,
            lives,
            pregnancies: BTreeMap::new(),
        });
        for p in sim.state.people.clone() {
            sim.schedule_age(p.id);
        }
        let first = sim.state.people[3].id;
        let second = sim.state.people[4].id;
        for (id, partner) in [(first, second), (second, first)] {
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&id)
                .unwrap();
            mind.dream = decisions::Dream::Parenthood;
            mind.personality.empathy = 900;
            mind.relationships.insert(
                partner,
                decisions::Relationship {
                    affection: 800,
                    trust: 800,
                },
            );
        }
        Ok(sim)
    }

    pub fn childhood_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::life_fixture(seed, execution)?;
        let predecessor = sim.state.ruler;
        sim.debug_injure(
            predecessor,
            sim.state.physical.as_ref().unwrap().rules.max_health,
        )?;
        sim.advance(SimDuration(300_000))?;
        let child = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .lives
            .values()
            .find(|l| l.born == SimTime(300_000) && !l.parents.is_empty())
            .ok_or_else(|| Error::InvalidFixture("fixture did not produce its child".into()))?
            .id;
        // Explicit scenario control chooses this actual post-death newborn. This
        // fixture does not establish the game's unresolved succession policy.
        sim.state.people[sim.person_index[&predecessor]].role = Role::Citizen;
        sim.state.people[sim.person_index[&child]].role = Role::Ruler;
        sim.state.ruler = child;
        sim.state.scenario = scenarios().remove(5);
        Ok(sim)
    }

    pub(crate) fn independent(&self, id: PersonId) -> bool {
        self.state.development.as_ref().is_none_or(|w| {
            w.lives
                .get(&id)
                .is_some_and(|l| !matches!(l.stage, LifeStage::Infant))
        })
    }

    fn adult(&self, id: PersonId) -> bool {
        self.state.development.as_ref().is_some_and(|w| {
            w.lives
                .get(&id)
                .is_some_and(|l| matches!(l.stage, LifeStage::Adult | LifeStage::Elder))
        })
    }

    fn willing_parent(&self, id: PersonId, partner: PersonId) -> bool {
        let Some(mind) = self.state.decisions.as_ref().and_then(|w| w.minds.get(&id)) else {
            return false;
        };
        let body = &self.state.physical.as_ref().unwrap().actors[&id];
        self.adult(id)
            && body.vitals.alive
            && body.vitals.health > 0
            && body.vitals.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief
            && matches!(body.activity, Activity::Idle | Activity::Rest)
            && mind.dream == decisions::Dream::Parenthood
            && mind
                .relationships
                .get(&partner)
                .is_some_and(|r| r.affection >= 100 && r.trust >= 0)
            && mind
                .knowledge
                .values()
                .any(|f| f.good == Good::Food && f.accessible && f.quantity.0 >= 3_000)
    }

    pub(crate) fn start_conception(
        &mut self,
        actor: PersonId,
        partner: PersonId,
    ) -> Result<(), Error> {
        if self.combat_restricts(actor) || self.combat_restricts(partner) {
            return Err(Error::Unavailable);
        }
        let world = self.state.development.as_ref().ok_or(Error::Unavailable)?;
        let (Some(a), Some(b)) = (world.lives.get(&actor), world.lives.get(&partner)) else {
            return Err(Error::Unavailable);
        };
        let actor_body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let partner_body = &self.state.physical.as_ref().unwrap().actors[&partner];
        let location = &self.state.people[self.person_index[&actor]].location;
        if actor == partner
            || !self.adult(actor)
            || !self.adult(partner)
            || !actor_body.vitals.alive
            || actor_body.vitals.health == 0
            || !partner_body.vitals.alive
            || !matches!(location, Location::AtArea(_))
            || location != &self.state.people[self.person_index[&partner]].location
        {
            return Err(Error::Unavailable);
        }
        let (gestator, supplier) = match (a.reproduction, b.reproduction) {
            (Reproduction::Gestates, Reproduction::SuppliesSeed) => (actor, partner),
            (Reproduction::SuppliesSeed, Reproduction::Gestates) => (partner, actor),
            _ => return Err(Error::Unavailable),
        };
        if self.pregnancy_index.contains_key(&gestator) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(1)?;
        if !self.willing_parent(partner, actor) {
            self.emit(EventKind::ConceptionRefused { actor, partner });
            self.revision += 1;
            return Ok(());
        }
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(world.rules.conception_time.0)
                .ok_or(Error::Overflow)?,
        );
        completes
            .0
            .checked_add(world.rules.gestation.0)
            .ok_or(Error::Overflow)?;
        let id = PregnancyId(allocate(&mut self.state)?);
        self.interrupt_physical(actor);
        self.interrupt_physical(partner);
        self.state.development.as_mut().unwrap().pregnancies.insert(
            id,
            Pregnancy {
                id,
                gestator,
                partner: supplier,
                state: PregnancyState::Conceiving { completes },
            },
        );
        self.pregnancy_index.insert(gestator, id);
        for id_actor in [actor, partner] {
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&id_actor)
                .unwrap()
                .activity = Activity::Conceive {
                pregnancy: id,
                completes,
            };
            self.state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(id_actor));
        }
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn review_conception(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.development else {
            return Ok(());
        };
        let mut pairs = BTreeSet::new();
        for id in world.lives.keys().filter(|id| **id != self.state.ruler) {
            let mind = &self.state.decisions.as_ref().unwrap().minds[id];
            if mind.dream != decisions::Dream::Parenthood {
                continue;
            }
            for partner in mind
                .relationships
                .keys()
                .filter(|p| **p != self.state.ruler)
            {
                if self.willing_parent(*id, *partner) && self.willing_parent(*partner, *id) {
                    pairs.insert(((*id).min(*partner), (*id).max(*partner)));
                }
            }
        }
        for (a, b) in pairs {
            if self.willing_parent(a, b) && self.willing_parent(b, a) {
                match self.start_conception(a, b) {
                    Ok(_) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn complete_conception(&mut self, id: PregnancyId) {
        let pregnancy = self.state.development.as_ref().unwrap().pregnancies[&id].clone();
        let pair = [pregnancy.gestator, pregnancy.partner];
        let valid = pair.iter().all(|p| {
            let a = &self.state.physical.as_ref().unwrap().actors[p];
            a.vitals.alive
                && a.vitals.health > 0
                && matches!(a.activity, Activity::Conceive { pregnancy, .. } if pregnancy == id)
        }) && self.state.people[self.person_index[&pair[0]]].location
            == self.state.people[self.person_index[&pair[1]]].location;
        self.state
            .development
            .as_mut()
            .unwrap()
            .pregnancies
            .get_mut(&id)
            .unwrap()
            .state = PregnancyState::Ended;
        for p in pair {
            self.interrupt_physical(p);
        }
        if valid {
            let due = SimTime(
                self.state.time.0 + self.state.development.as_ref().unwrap().rules.gestation.0,
            );
            self.state
                .development
                .as_mut()
                .unwrap()
                .pregnancies
                .get_mut(&id)
                .unwrap()
                .state = PregnancyState::Gestating { due };
            self.state.due.entry(due).or_default().push(Due::Birth(id));
            self.emit(EventKind::Conceived {
                pregnancy: id,
                parents: pair,
            });
        } else {
            self.pregnancy_index.remove(&pregnancy.gestator);
            self.state
                .development
                .as_mut()
                .unwrap()
                .pregnancies
                .get_mut(&id)
                .unwrap()
                .state = PregnancyState::Ended;
            self.emit(EventKind::PregnancyEnded { pregnancy: id });
        }
    }

    pub(crate) fn preflight_development(&self, due: &[Due], next: SimTime) -> Result<(), Error> {
        let Some(world) = &self.state.development else {
            return Ok(());
        };
        let births = due.iter().filter(|d| matches!(d, Due::Birth(_))).count() as u64;
        self.state
            .next_id
            .checked_add(
                births
                    .checked_mul(2)
                    .ok_or(Error::Overflow)?
                    .checked_add(self.state.people.len() as u64 / 2)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        next.0
            .checked_add(world.rules.lifespan.0)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    pub(crate) fn complete_development(&mut self, due: Due) {
        match due {
            Due::Age(id) => {
                let a = &self.state.physical.as_ref().unwrap().actors[&id];
                if !a.vitals.alive {
                    return;
                }
                let world = self.state.development.as_ref().unwrap();
                let life = &world.lives[&id];
                let age = self.state.time.0 - life.born.0 + life.initial_age.0;
                if age >= world.rules.lifespan.0 {
                    self.end_life(id);
                } else {
                    let stage = if age < world.rules.infancy.0 {
                        LifeStage::Infant
                    } else if age < world.rules.adulthood.0 {
                        LifeStage::Child
                    } else if age < world.rules.elderhood.0 {
                        LifeStage::Adult
                    } else {
                        LifeStage::Elder
                    };
                    self.state
                        .development
                        .as_mut()
                        .unwrap()
                        .lives
                        .get_mut(&id)
                        .unwrap()
                        .stage = stage;
                    if stage == LifeStage::Adult {
                        for people in self.dependent_index.values_mut() {
                            people.remove(&id);
                        }
                    }
                    self.emit(EventKind::Developed { actor: id, stage });
                    self.schedule_age(id);
                }
            }
            Due::Birth(id) => self.birth(id),
            _ => {}
        }
    }

    fn birth(&mut self, id: PregnancyId) {
        let pregnancy = self.state.development.as_ref().unwrap().pregnancies[&id].clone();
        if !matches!(pregnancy.state, PregnancyState::Gestating { .. }) {
            return;
        }
        self.pregnancy_index.remove(&pregnancy.gestator);
        if !self.state.physical.as_ref().unwrap().actors[&pregnancy.gestator]
            .vitals
            .alive
        {
            self.state
                .development
                .as_mut()
                .unwrap()
                .pregnancies
                .get_mut(&id)
                .unwrap()
                .state = PregnancyState::Ended;
            self.emit(EventKind::PregnancyEnded { pregnancy: id });
            return;
        }
        let location = self.state.people[self.person_index[&pregnancy.gestator]]
            .location
            .clone();
        let child = PersonId(allocate(&mut self.state).expect("preflight child ID"));
        let inventory = InventoryId(allocate(&mut self.state).expect("preflight inventory ID"));
        let index = self.state.people.len();
        self.state.people.push(Person {
            id: child,
            role: Role::Citizen,
            location: location.clone(),
            inventory,
            work_units: 0,
            work_remainder: 0,
            work_rate: 1_000,
            route: Vec::new(),
            hunger_exempt: false,
        });
        if let Location::InTransit { arrives, .. } = location {
            self.state
                .due
                .entry(arrives)
                .or_default()
                .push(Due::Arrive(child));
        }
        self.person_index.insert(child, index);
        if let Some(world) = &mut self.state.combat {
            world
                .profiles
                .insert(child, combat::Profile { initiative: 100 });
        }
        let cargo = InventoryLocation::Carried(child);
        self.inventory_index
            .entry(cargo.clone())
            .or_default()
            .push(inventory);
        self.state.inventories.insert(
            inventory,
            Inventory {
                id: inventory,
                location: cargo,
                quantity: Quantity(0),
                capacity: Quantity(10_000),
                access: BTreeSet::from([child]),
                last_granted: None,
                good: Good::Food,
                spoils_at: None,
            },
        );
        let max_health = self.state.physical.as_ref().unwrap().rules.max_health;
        self.state.physical.as_mut().unwrap().actors.insert(
            child,
            physical::Actor {
                vitals: Vitals {
                    hunger: 0,
                    fatigue: 0,
                    health: max_health,
                    alive: true,
                },
                activity: Activity::Idle,
                job: None,
            },
        );
        let parents = [pregnancy.gestator, pregnancy.partner];
        let world = self.state.development.as_mut().unwrap();
        world.lives.insert(
            child,
            Life {
                id: child,
                born: self.state.time,
                initial_age: SimDuration(0),
                stage: LifeStage::Infant,
                reproduction: if child.0.is_multiple_of(2) {
                    Reproduction::Gestates
                } else {
                    Reproduction::SuppliesSeed
                },
                parents: parents.to_vec(),
                caregivers: BTreeSet::new(),
                preference: None,
            },
        );
        world.pregnancies.get_mut(&id).unwrap().state = PregnancyState::Born(child);
        let mut mind = self.state.decisions.as_ref().unwrap().minds[&pregnancy.gestator].clone();
        mind.interests.clear();
        mind.routes.clear();
        mind.last_failure = None;
        mind.child_signals.clear();
        mind.knowledge.clear();
        mind.sites.clear();
        mind.values.clear();
        mind.relationships.clear();
        mind.dream = decisions::Dream::Comfort;
        mind.plan = None;
        mind.retry_after = self.state.time;
        mind.goal = decisions::Goal::Survival;
        mind.explanation = decisions::Explanation {
            goal: decisions::Goal::Survival,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        };
        self.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .insert(child, mind);
        // Kinship changes a directed initial relationship. It grants no assets,
        // permissions, office, compulsory care, or legal guardianship.
        for parent in parents {
            if self.state.people[self.person_index[&parent]].location
                != self.state.people[self.person_index[&child]].location
            {
                continue;
            }
            let parent_mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&parent)
                .unwrap();
            parent_mind.relationships.insert(
                child,
                decisions::Relationship {
                    trust: 0,
                    affection: i32::from(parent_mind.personality.empathy),
                },
            );
        }
        if let Location::AtArea(area) = self.state.people[self.person_index[&child]].location {
            self.index_dependent(child, area);
        }
        self.schedule_age(child);
        self.emit(EventKind::Born {
            pregnancy: id,
            child,
            parents,
        });
    }

    pub(crate) fn index_dependent(&mut self, id: PersonId, area: AreaId) {
        if self
            .state
            .development
            .as_ref()
            .is_some_and(|w| matches!(w.lives[&id].stage, LifeStage::Infant | LifeStage::Child))
        {
            self.dependent_index.entry(area).or_default().insert(id);
        }
    }

    pub(crate) fn schedule_age(&mut self, id: PersonId) {
        if self.state.people[self.person_index[&id]].role == Role::Guardian
            || !self.state.physical.as_ref().unwrap().actors[&id]
                .vitals
                .alive
        {
            return;
        }
        let world = self.state.development.as_ref().unwrap();
        let life = &world.lives[&id];
        let age = self.state.time.0 - life.born.0 + life.initial_age.0;
        if let Some(threshold) = [
            world.rules.infancy,
            world.rules.adulthood,
            world.rules.elderhood,
            world.rules.lifespan,
        ]
        .into_iter()
        .find(|t| t.0 > age)
        {
            let at = SimTime(life.born.0 + threshold.0 - life.initial_age.0);
            self.state.due.entry(at).or_default().push(Due::Age(id));
        }
    }

    pub(crate) fn end_life(&mut self, id: PersonId) {
        let a = self
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&id)
            .unwrap();
        if !a.vitals.alive {
            return;
        }
        a.vitals.alive = false;
        a.vitals.health = 0;
        a.job = None;
        self.interrupt_physical(id);
        self.state.people[self.person_index[&id]].route.clear();
        for due in self.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Age(p) | Due::Arrive(p) if *p == id));
        }
        self.state.due.retain(|_, due| !due.is_empty());
        for people in self.dependent_index.values_mut() {
            people.remove(&id);
        }
        self.emit(EventKind::Died { actor: id });
        self.invalidate_pregnancies(id, true);
    }

    pub(crate) fn invalidate_pregnancies(&mut self, actor: PersonId, died: bool) {
        if self.state.development.is_none() {
            return;
        }
        let mut ids = BTreeSet::new();
        if let Activity::Conceive { pregnancy, .. } =
            self.state.physical.as_ref().unwrap().actors[&actor].activity
            && matches!(
                self.state.development.as_ref().unwrap().pregnancies[&pregnancy].state,
                PregnancyState::Conceiving { .. }
            )
        {
            ids.insert(pregnancy);
        }
        if died && let Some(pregnancy) = self.pregnancy_index.get(&actor) {
            ids.insert(*pregnancy);
        }
        for id in ids {
            let p = self
                .state
                .development
                .as_mut()
                .unwrap()
                .pregnancies
                .get_mut(&id)
                .unwrap();
            let pair = [p.gestator, p.partner];
            self.pregnancy_index.remove(&p.gestator);
            let was_conceiving = matches!(p.state, PregnancyState::Conceiving { .. });
            p.state = PregnancyState::Ended;
            if was_conceiving {
                for person in pair {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&person)
                        .unwrap()
                        .activity = Activity::Idle;
                    for due in self.state.due.values_mut() {
                        due.retain(|d| !matches!(d, Due::Physical(p) if *p == person));
                    }
                }
            }
            for due in self.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Birth(p) if *p == id));
            }
            self.state.due.retain(|_, due| !due.is_empty());
            self.emit(EventKind::PregnancyEnded { pregnancy: id });
        }
    }

    pub fn debug_set_relationship(
        &mut self,
        actor: PersonId,
        other: PersonId,
        relationship: decisions::Relationship,
    ) -> Result<(), Error> {
        if !self.person_index.contains_key(&other)
            || !(-1_000..=1_000).contains(&relationship.affection)
            || !(-1_000..=1_000).contains(&relationship.trust)
        {
            return Err(Error::InvalidFixture("invalid relationship".into()));
        }
        let mind = self
            .state
            .decisions
            .as_mut()
            .and_then(|w| w.minds.get_mut(&actor))
            .ok_or(Error::Unavailable)?;
        mind.relationships.insert(other, relationship);
        Ok(())
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.development else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent development state".into());
    if world.lives.len() != state.people.len()
        || world.rules.conception_time.0 == 0
        || world.rules.gestation.0 == 0
        || !(0 < world.rules.infancy.0
            && world.rules.infancy < world.rules.adulthood
            && world.rules.adulthood < world.rules.elderhood
            && world.rules.elderhood < world.rules.lifespan)
    {
        return Err(fail());
    }
    let mut age_deadlines = BTreeSet::new();
    let mut birth_deadlines = BTreeSet::new();
    for (at, due) in &state.due {
        for event in due {
            match event {
                Due::Age(id) => {
                    let life = world.lives.get(id).ok_or_else(fail)?;
                    let age =
                        at.0.checked_sub(life.born.0)
                            .and_then(|v| v.checked_add(life.initial_age.0))
                            .ok_or_else(fail)?;
                    if ![
                        world.rules.infancy,
                        world.rules.adulthood,
                        world.rules.elderhood,
                        world.rules.lifespan,
                    ]
                    .contains(&SimDuration(age))
                        || !age_deadlines.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                Due::Birth(id) => {
                    let p = world.pregnancies.get(id).ok_or_else(fail)?;
                    if p.state != (PregnancyState::Gestating { due: *at })
                        || !birth_deadlines.insert(*id)
                    {
                        return Err(fail());
                    }
                }
                _ => {}
            }
        }
    }
    let bodies = &state.physical.as_ref().unwrap().actors;
    for person in &state.people {
        let life = world.lives.get(&person.id).ok_or_else(fail)?;
        if life.id != person.id
            || life.born > state.time
            || life
                .parents
                .iter()
                .any(|id| !world.lives.contains_key(id) || *id == person.id)
            || life
                .caregivers
                .iter()
                .any(|id| !world.lives.contains_key(id))
        {
            return Err(fail());
        }
        if (bodies[&person.id].vitals.alive && person.role != Role::Guardian)
            != age_deadlines.contains(&person.id)
        {
            return Err(fail());
        }
        if bodies[&person.id].vitals.alive {
            let age = state
                .time
                .0
                .checked_sub(life.born.0)
                .and_then(|v| v.checked_add(life.initial_age.0))
                .ok_or_else(fail)?;
            let stage = if age < world.rules.infancy.0 {
                LifeStage::Infant
            } else if age < world.rules.adulthood.0 {
                LifeStage::Child
            } else if age < world.rules.elderhood.0 {
                LifeStage::Adult
            } else {
                LifeStage::Elder
            };
            if person.role != Role::Guardian
                && (age >= world.rules.lifespan.0 || stage != life.stage)
            {
                return Err(fail());
            }
        }
    }
    for (id, p) in &world.pregnancies {
        if *id != p.id
            || p.gestator == p.partner
            || !world.lives.contains_key(&p.gestator)
            || !world.lives.contains_key(&p.partner)
            || world.lives[&p.gestator].reproduction != Reproduction::Gestates
            || world.lives[&p.partner].reproduction != Reproduction::SuppliesSeed
        {
            return Err(fail());
        }
        match p.state {
            PregnancyState::Conceiving { completes } => {
                for actor in [p.gestator, p.partner] {
                    if bodies[&actor].activity
                        != (Activity::Conceive {
                            pregnancy: *id,
                            completes,
                        })
                    {
                        return Err(fail());
                    }
                }
            }
            PregnancyState::Gestating { .. } => {
                if !birth_deadlines.contains(id) || !bodies[&p.gestator].vitals.alive {
                    return Err(fail());
                }
            }
            PregnancyState::Born(child) => {
                if !world
                    .lives
                    .get(&child)
                    .is_some_and(|l| l.parents == vec![p.gestator, p.partner])
                {
                    return Err(fail());
                }
            }
            PregnancyState::Ended => {}
        }
    }
    Ok(())
}
