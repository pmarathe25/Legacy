//! Reserved newborn body creation, optionally receiving inherited expression.
use crate::{
    Body, Rules,
    traits::{self, Development, Phenotype},
};
use game_core::{Error, PersonId};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

pub struct PreparedNewbornBody {
    actor: PersonId,
    rules: Rules,
    definitions: Option<traits::Definitions>,
    body: Body,
    development: Option<Development>,
    phenotype: Option<Phenotype>,
    reads: ReadSet,
}

#[cfg(feature = "genetics")]
pub struct PreparedFounderExpression {
    actor: PersonId,
    definitions: traits::Definitions,
    body: Body,
    development: Development,
    reads: ReadSet,
}

#[cfg(feature = "genetics")]
impl PreparedFounderExpression {
    pub fn stage_into(
        self,
        runtime: &traits::Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        if runtime.definitions != self.definitions {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<Body>(id) || batch.touches::<Development>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Body>(id);
        admitted.protect::<Development>(id);
        batch.insert(id, self.body);
        batch.insert(id, self.development);
        self.reads.bind(store, batch, &admitted)
    }
}
pub fn prepare_newborn_body(
    store: &Store,
    actor: PersonId,
    rules: &Rules,
) -> Result<PreparedNewbornBody, Error> {
    let mut reads = ReadSet::new();
    if actor.0 == 0
        || reads.entity(store, StableId(actor.0))
        || reads.read::<Body>(store, StableId(actor.0)).is_some()
    {
        return Err(Error::Unavailable);
    }
    Ok(PreparedNewbornBody {
        actor,
        rules: rules.clone(),
        definitions: None,
        body: rules.newborn()?,
        development: None,
        phenotype: None,
        reads,
    })
}
impl PreparedNewbornBody {
    pub fn body(&self) -> &Body {
        &self.body
    }
    pub fn phenotype(&self) -> Option<Phenotype> {
        self.phenotype
    }
    pub fn stage_into(
        self,
        rules: &Rules,
        runtime: Option<&traits::Runtime>,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        if rules != &self.rules
            || self.definitions.as_ref().is_some_and(|definitions| {
                runtime.is_none_or(|runtime| &runtime.definitions != definitions)
            })
        {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<Body>(id)
            || self.development.is_some() && batch.touches::<Development>(id)
        {
            return Err(Error::Unavailable);
        }
        self.body.validate()?;
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Body>(id);
        batch.insert(id, self.body);
        if let Some(development) = self.development {
            development.validate()?;
            admitted.protect::<Development>(id);
            batch.insert(id, development);
        }
        self.reads.bind(store, batch, &admitted)
    }
}
#[cfg(feature = "genetics")]
impl traits::Runtime {
    /// Initial inherited expression receives an existing founder's neutral Body
    /// and creates its first Development row in the same genomic admission.
    pub fn prepare_founder_expression(
        &self,
        store: &Store,
        actor: PersonId,
        development: &Development,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedFounderExpression, Error> {
        self.definitions.validate()?;
        development.validate()?;
        let mut reads = ReadSet::new();
        let id = StableId(actor.0);
        let mut body = reads
            .read::<Body>(store, id)
            .ok_or(Error::Unavailable)?
            .clone();
        if reads.read::<Development>(store, id).is_some() {
            return Err(Error::Unavailable);
        }
        let phenotype = self.phenotype_for(development, |locus| {
            let [first, second] = source.alleles_named(actor, locus)?;
            Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
        })?;
        body.rescale_health_factor(1_000, phenotype.max_health_milli);
        body.validate()?;
        Ok(PreparedFounderExpression {
            actor,
            definitions: self.definitions.clone(),
            body,
            development: *development,
            reads,
        })
    }
    pub fn prepare_newborn_body(
        &self,
        store: &Store,
        actor: PersonId,
        rules: &Rules,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedNewbornBody, Error> {
        self.definitions.validate()?;
        let mut prepared = prepare_newborn_body(store, actor, rules)?;
        if prepared
            .reads
            .read::<Development>(store, StableId(actor.0))
            .is_some()
        {
            return Err(Error::Unavailable);
        }
        let development = Development {
            maturity_milli: 0,
            physical_training_milli: 0,
        };
        let phenotype = self.newborn_phenotype(actor, source)?;
        prepared
            .body
            .rescale_health_factor(1_000, phenotype.max_health_milli);
        prepared.body.validate()?;
        prepared.definitions = Some(self.definitions.clone());
        prepared.development = Some(development);
        prepared.phenotype = Some(phenotype);
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::{Quantity, SimDuration};

    fn rules() -> Rules {
        Rules {
            hunger_harm: 100,
            fatigue_limit: 100,
            max_health: 100,
            meal_relief: 50,
            meal_size: Quantity(1),
            eating_time: SimDuration(1),
            loading_time: SimDuration(1),
            carrying_capacity: Quantity(10_000),
        }
    }

    #[test]
    fn newborn_body_rejects_changed_rules_and_foreign_row_overwrite_before_publication() {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let actor = PersonId(1);
        let rules = rules();
        let before = store.snapshot().unwrap();
        let plan = prepare_newborn_body(&store, actor, &rules).unwrap();
        let mut changed = rules.clone();
        changed.max_health += 1;
        assert!(
            plan.stage_into(&changed, None, &store, &mut Batch::new())
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let plan = prepare_newborn_body(&store, actor, &rules).unwrap();
        assert_eq!(plan.body(), &rules.newborn().unwrap());
        let mut batch = Batch::new();
        let guard = plan.stage_into(&rules, None, &store, &mut batch).unwrap();
        batch.remove::<Body>(StableId(actor.0));
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
        let plan = prepare_newborn_body(&store, actor, &rules).unwrap();
        let mut batch = Batch::new();
        let guard = plan.stage_into(&rules, None, &store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(crate::body(&store, actor), Some(&rules.newborn().unwrap()));
    }
}
