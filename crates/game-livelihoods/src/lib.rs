//! Personal stock goals compose gathering, recipe chains, and small plantings.
mod planning;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub object: String,
    pub quantity: u64,
    pub priority: u32,
    pub affinity: String,
    pub concept: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub interval: SimDuration,
    pub goals: Vec<Goal>,
}
impl Rules {
    pub fn validate(&self) -> Result<()> {
        require(
            self.interval.0 > 0
                && self.goals.len() <= 128
                && self.goals.iter().all(|g| {
                    !g.object.is_empty()
                        && (1..=1000).contains(&g.quantity)
                        && (1..=1000).contains(&g.priority)
                        && !g.affinity.is_empty()
                        && !g.concept.is_empty()
                }),
            "invalid livelihood goals",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Plan {
    inventory: InventoryId,
    goal: GoodId,
    quantity: u64,
    concept: String,
    steps: VecDeque<planning::Step>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Livelihoods {
    pub enabled: bool,
    pub rules: Option<Arc<Rules>>,
    pub stamps: Vec<game_content::Stamp>,
    plans: Table<PersonId, Plan>,
    pub retry_at: Table<PersonId, SimTime>,
    pub achieved: Table<PersonId, u64>,
    pub failures: Table<PersonId, u64>,
}
impl Livelihoods {
    pub fn has_plan(&self, person: PersonId) -> bool {
        self.plans.contains_key(&person)
    }
}
impl Resource for Livelihoods {
    const KEY: &'static str = "livelihoods.people";
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        if self.rules.as_ref().map(Arc::as_ptr) != previous.rules.as_ref().map(Arc::as_ptr) {
            return self.validate();
        }
        require(
            !self.enabled || self.rules.is_some(),
            "livelihoods lack rules",
        )?;
        require(
            self.plans
                .changed(&previous.plans)
                .all(|(_, p)| p.quantity > 0 && p.steps.len() <= planning::MAX_STEPS),
            "invalid work plan",
        )
    }
    fn validate(&self) -> Result<()> {
        if let Some(r) = &self.rules {
            r.validate()?;
        }
        require(
            !self.enabled || self.rules.is_some(),
            "livelihoods lack rules",
        )?;
        require(
            self.plans
                .values()
                .all(|p| p.quantity > 0 && p.steps.len() <= planning::MAX_STEPS),
            "invalid work plan",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Install {
    pub rules: Rules,
    pub stamps: Vec<game_content::Stamp>,
}
impl Request for Install {
    const KEY: &'static str = "livelihoods.install";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "livelihoods.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pursue {
    person: PersonId,
    plan: Plan,
}
impl Request for Pursue {
    const KEY: &'static str = "livelihoods.pursue";
    type Response = game_choice::Outcome;
}
impl Request for Consider {
    const KEY: &'static str = "livelihoods.consider";
    type Response = bool;
}
pub fn load(store: &mut game_content::Store, objects: &game_objects::Objects) -> Result<Install> {
    let doc: game_content::Document<Rules> = store.load("livelihoods/daily.toml")?;
    doc.data.validate()?;
    for goal in &doc.data.goals {
        objects.resolve(&goal.object)?;
    }
    Ok(Install {
        stamps: vec![doc.stamp()?],
        rules: doc.data,
    })
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Supply {
    pub person: PersonId,
    pub goal: Goal,
}
impl Request for Supply {
    const KEY: &'static str = "livelihoods.supply";
    type Response = game_choice::Outcome;
}
fn goals<'a>(tx: &'a Transaction<'_>) -> Result<&'a [Goal]> {
    Ok(&tx
        .get::<Livelihoods>()?
        .rules
        .as_ref()
        .ok_or_else(|| Error::Missing("livelihood goals".into()))?
        .goals)
}
fn inventories(tx: &Transaction<'_>, person: PersonId) -> Result<Vec<InventoryId>> {
    let area = game_transport::area(tx.get()?, person)?;
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(vec![]);
    };
    let mut rows = game_food_memory::with_stores(person, facts, tx.get()?, |s| {
        s.all()
            .filter_map(|s| {
                let l = s.location.as_ref()?;
                (l.carrier == Some(person) || l.area == area)
                    .then_some((l.carrier != Some(person), s.id))
            })
            .collect::<Vec<_>>()
    });
    rows.sort_unstable();
    Ok(rows.into_iter().map(|(_, id)| id).collect())
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    if !tx.get::<Livelihoods>()?.enabled
        || tx.get::<game_lineage::Lineage>()?.current == Some(person)
        || tx.get::<Actions>()?.0.contains_key(&person)
        || game_physiology::capable(tx, person).is_err()
        || game_transport::area(tx.get()?, person).is_err()
    {
        return Ok(false);
    }
    if let Some(plan) = tx.get::<Livelihoods>()?.plans.get(&person).cloned() {
        let preference = game_vitality::disposition(tx, person, "ambition");
        if game_property::reluctance(tx, person, plan.inventory) * 8 > preference + 300 {
            tx.edit::<Livelihoods>()?.plans.remove(&person);
            return Ok(false);
        }
        let step = plan.steps.front().cloned();
        if let Some(step) = step {
            let result = match step {
                planning::Step::Gather { patch, quantity } => tx
                    .attempt(game_ecology::Gather {
                        person,
                        patch,
                        quantity,
                        destination: plan.inventory,
                    })
                    .map(|_| ()),
                planning::Step::Craft { recipe } => tx
                    .attempt(game_workshop::Craft {
                        person,
                        recipe,
                        inventory: plan.inventory,
                    })
                    .map(|_| ()),
            };
            if result.is_ok() {
                tx.edit::<Livelihoods>()?
                    .plans
                    .get_mut(&person)
                    .unwrap()
                    .steps
                    .pop_front();
                return Ok(true);
            }
            let n = tx
                .edit::<Livelihoods>()?
                .failures
                .entry(person)
                .or_default();
            *n = n.saturating_add(1);
        } else if tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&plan.inventory)
            .is_some_and(|i| i.amount(plan.goal) >= plan.quantity)
        {
            let n = tx
                .edit::<Livelihoods>()?
                .achieved
                .entry(person)
                .or_default();
            *n = n.saturating_add(1);
            tx.call(game_relationships::LearnValue {
                person,
                concept: plan.concept,
                outcome: 1,
            })?;
        }
        tx.edit::<Livelihoods>()?.plans.remove(&person);
        let next = tx
            .now()
            .after(tx.get::<Livelihoods>()?.rules.as_ref().unwrap().interval)?;
        tx.edit::<Livelihoods>()?.retry_at.insert(person, next);
        return Ok(false);
    }
    if tx
        .get::<Livelihoods>()?
        .retry_at
        .get(&person)
        .is_some_and(|t| *t > tx.now())
    {
        return Ok(false);
    }
    let stocks = inventories(tx, person)?;
    let next = tx
        .now()
        .after(tx.get::<Livelihoods>()?.rules.as_ref().unwrap().interval)?;
    tx.edit::<Livelihoods>()?.retry_at.insert(person, next);
    let Some(plan) = planning::choose(tx, person, &stocks, goals(tx)?)? else {
        return Ok(false);
    };
    tx.edit::<Livelihoods>()?.plans.insert(person, plan);
    consider(tx, person)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("livelihoods", 1)?;
    b.resource::<Livelihoods>()?;
    b.request::<Pursue>(|tx, r| {
        tx.edit::<Livelihoods>()?.plans.insert(r.person, r.plan);
        Ok(if consider(tx, r.person)? {
            game_choice::Outcome::Performed
        } else {
            game_choice::Outcome::Reconsider
        })
    })?;
    b.request::<Install>(|tx, r| {
        r.rules.validate()?;
        require(
            !tx.get::<Livelihoods>()?.enabled,
            "livelihoods already enabled",
        )?;
        for goal in &r.rules.goals {
            tx.get::<game_objects::Objects>()?.resolve(&goal.object)?;
            require(
                tx.get::<game_traits::Traits>()?
                    .definitions
                    .contains_key(&goal.affinity),
                "unknown work affinity",
            )?;
        }
        let s = tx.edit::<Livelihoods>()?;
        s.rules = Some(Arc::new(r.rules));
        s.stamps = r.stamps;
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        require(
            tx.get::<Livelihoods>()?.rules.is_some(),
            "missing livelihood goals",
        )?;
        tx.edit::<Livelihoods>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Supply>(|tx, r| {
        let good = tx.get::<game_objects::Objects>()?.resolve(&r.goal.object)?;
        if tx
            .get::<Livelihoods>()?
            .plans
            .get(&r.person)
            .is_none_or(|p| p.goal != good)
        {
            let held = inventories(tx, r.person)?;
            let Some(plan) = planning::choose(tx, r.person, &held, std::slice::from_ref(&r.goal))?
            else {
                return game_search::find(tx, r.person, good, r.goal.quantity);
            };
            tx.edit::<Livelihoods>()?.plans.insert(r.person, plan);
        }
        consider(tx, r.person).map(Into::into)
    })?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    game_choice::provider(b, "livelihoods.work", |tx, e| {
        if game_choice::collecting(tx, e.person) {
            if !tx.get::<Livelihoods>()?.enabled
                || game_transport::area(tx.get()?, e.person).is_err()
            {
                return Ok(());
            }
            let held = inventories(tx, e.person)?;
            let plan = tx.get::<Livelihoods>()?.plans.get(&e.person).cloned();
            let plan = match plan {
                Some(plan) => Some(plan),
                None => planning::choose(tx, e.person, &held, goals(tx)?)?,
            };
            if let Some(plan) = plan {
                let goal = tx
                    .get::<Livelihoods>()?
                    .rules
                    .as_ref()
                    .unwrap()
                    .goals
                    .iter()
                    .find(|g| {
                        tx.get::<game_objects::Objects>()
                            .unwrap()
                            .resolve(&g.object)
                            == Ok(plan.goal)
                    });
                let desire = goal.map_or(0, |g| {
                    i64::from(g.priority) / 2
                        + game_vitality::disposition(tx, e.person, &g.affinity) / 20
                });
                let interest = tx
                    .get::<game_relationships::Social>()?
                    .values
                    .get(&e.person)
                    .and_then(|v| v.get(plan.concept.as_str()))
                    .copied()
                    .unwrap_or(0);
                let effort = plan
                    .steps
                    .iter()
                    .map(|step| match step {
                        planning::Step::Gather { .. } => 5,
                        planning::Step::Craft { recipe } => {
                            let recipe =
                                &tx.get::<game_workshop::Workshop>().unwrap().recipes[recipe];
                            let practice = tx
                                .get::<game_skills::Skills>()
                                .ok()
                                .and_then(|s| s.levels.get(&e.person)?.get(recipe.skill.as_str()))
                                .copied()
                                .unwrap_or(0);
                            tx.get::<game_knowledge::Knowledge>()
                                .ok()
                                .and_then(|k| k.0.get(&e.person))
                                .and_then(|f| {
                                    game_knowledge::forecast(
                                        f,
                                        &format!("craft:{}", recipe.output.0),
                                        "work-time",
                                        &[(
                                            "practice".into(),
                                            practice.min(i64::MAX as u64) as i64 / 10 * 10,
                                        )]
                                        .into(),
                                    )
                                })
                                .map_or(recipe.duration.0 / MINUTE.0, |p| p.value.max(1) as u64)
                        }
                    })
                    .sum::<u64>();
                game_choice::offer(
                    tx,
                    e.person,
                    format!("work:{}", plan.goal.0),
                    game_choice::motive(
                        0,
                        desire,
                        i64::from(interest) * 2,
                        (effort / 10).min(10000) as i64,
                        0,
                    ),
                    Pursue {
                        person: e.person,
                        plan,
                    },
                )?;
            }
            return Ok(());
        }
        tx.call(Consider { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("livelihoods.death", |tx, e| {
        let s = tx.edit::<Livelihoods>()?;
        s.plans.remove(&e.person);
        s.retry_at.remove(&e.person);
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Livelihoods>()?;
        for (person, plan) in &s.plans {
            require(
                h.get::<game_physiology::Bodies>()?
                    .people
                    .get(person)
                    .is_some_and(|b| b.alive)
                    && h.get::<game_cargo::Cargo>()?
                        .0
                        .contains_key(&plan.inventory)
                    && h.get::<game_objects::Objects>()?.by_id(plan.goal).is_ok(),
                "invalid saved work plan",
            )?;
            for step in &plan.steps {
                match step {
                    planning::Step::Gather { patch, quantity } => require(
                        *quantity > 0
                            && h.get::<game_ecology::Ecology>()?
                                .patches
                                .contains_key(patch),
                        "unknown work patch",
                    )?,
                    planning::Step::Craft { recipe } => require(
                        h.get::<game_workshop::Workshop>()?
                            .recipes
                            .contains_key(recipe),
                        "unknown work recipe",
                    )?,
                }
            }
        }
        Ok(())
    });
    Ok(())
}
