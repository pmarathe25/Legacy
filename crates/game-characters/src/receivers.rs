//! Prepared receiving changes for physical experience and factual communication.
//! The publication coordinator supplies foreign witnesses and sequences effects;
//! Characters owns bounded learning, relationship reactions and knowledge policy.
use crate::decisions::*;
use game_core::*;
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeMap;

pub struct PreparedMindEffects<A> {
    reads: ReadSet,
    rows: BTreeMap<PersonId, Mind<A>>,
}
impl<A: Component> PreparedMindEffects<A> {
    fn new() -> Self {
        Self {
            reads: ReadSet::new(),
            rows: BTreeMap::new(),
        }
    }
    fn row(&mut self, store: &Store, actor: PersonId) -> Option<&mut Mind<A>> {
        match self.rows.entry(actor) {
            std::collections::btree_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let before = self
                    .reads
                    .read::<Mind<A>>(store, StableId(actor.0))
                    .cloned()?;
                Some(entry.insert(before))
            }
        }
    }
    pub fn receive_play(
        &mut self,
        store: &Store,
        actor: PersonId,
        interest: ChildhoodInterest,
    ) -> Result<u64, Error> {
        let mind = self.row(store, actor).ok_or(Error::Unavailable)?;
        mind.validate()?;
        let experience = mind.interests.entry(interest).or_default();
        let gain = 100_u64.min(1000 - *experience);
        *experience += gain;
        Ok(gain)
    }
    pub fn receive_care_relationship(
        &mut self,
        store: &Store,
        child: PersonId,
        caregiver: PersonId,
    ) -> Result<(), Error> {
        self.receive_relationship_gain(store, child, caregiver, 50, 50)
    }
    pub fn receive_comfort_relationship(
        &mut self,
        store: &Store,
        child: PersonId,
        caregiver: PersonId,
    ) -> Result<(), Error> {
        self.receive_relationship_gain(store, child, caregiver, 0, 50)
    }
    fn receive_relationship_gain(
        &mut self,
        store: &Store,
        observer: PersonId,
        subject: PersonId,
        trust: i32,
        affection: i32,
    ) -> Result<(), Error> {
        let mind = self.row(store, observer).ok_or(Error::Unavailable)?;
        let relationship = mind.relationships.entry(subject).or_insert(Relationship {
            trust: 0,
            affection: 0,
        });
        relationship.trust = (i64::from(relationship.trust) + i64::from(trust)).min(1000) as i32;
        relationship.affection =
            (i64::from(relationship.affection) + i64::from(affection)).min(1000) as i32;
        Ok(())
    }
    pub fn receive_care_receipt(
        &mut self,
        store: &Store,
        caregiver: PersonId,
        child: PersonId,
        good: Good,
        quantity: Quantity,
        sequence: u64,
    ) -> Result<(), Error> {
        if let Some(mind) = self.row(store, caregiver) {
            mind.record_care_receipt(child, good, quantity, sequence)?;
        }
        Ok(())
    }
    pub fn receive_conversation(
        &mut self,
        store: &Store,
        effect: &crate::social::ConversationEffect,
    ) -> Result<(), Error> {
        for (actor, other) in [(effect.first, effect.second), (effect.second, effect.first)] {
            if let Some(mind) = self.row(store, actor) {
                crate::orchestration::apply_relationship_gain_to_mind(
                    mind,
                    other,
                    effect.affection_gain,
                    effect.trust_gain,
                );
            }
        }
        Ok(())
    }
    pub fn receive_routes(
        &mut self,
        store: &Store,
        student: PersonId,
        routes: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    ) -> Result<(), Error> {
        if let Some(mind) = self.row(store, student) {
            crate::orchestration::learn_routes_to_mind(mind, routes);
        }
        Ok(())
    }
    pub fn set_personality(
        &mut self,
        store: &Store,
        actor: PersonId,
        personality: Personality,
    ) -> Result<(), Error> {
        if !personality.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid expressed personality".into(),
            ));
        }
        self.row(store, actor)
            .ok_or(Error::Unavailable)?
            .personality = personality;
        Ok(())
    }
    pub fn projected_mind(&self, actor: PersonId) -> Option<&Mind<A>> {
        self.rows.get(&actor)
    }
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.reads.extend(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        if self
            .rows
            .keys()
            .any(|actor| batch.touches::<Mind<A>>(StableId(actor.0)))
        {
            return Err(Error::Unavailable);
        }
        for row in self.rows.values() {
            row.validate()?;
        }
        let mut admitted = ReadFootprint::new();
        for (actor, row) in self.rows {
            admitted.protect::<Mind<A>>(StableId(actor.0));
            batch.insert(StableId(actor.0), row);
        }
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
impl<A: Component> Default for PreparedMindEffects<A> {
    fn default() -> Self {
        Self::new()
    }
}
/// Agreement defaults are interpreted through the observer's personal honesty.
/// Missing minds are skipped, and each received debtor changes trust in order.
pub fn prepare_default_knowledge_reactions<A: Component>(
    store: &Store,
    effects: impl IntoIterator<Item = (PersonId, PersonId)>,
) -> Result<PreparedMindEffects<A>, Error> {
    let mut prepared = PreparedMindEffects::new();
    for (observer, subject) in effects {
        let Some(mind) = prepared.row(store, observer) else {
            continue;
        };
        let delta = -(i32::from(mind.personality.honesty) / 100);
        let relationship = mind.relationships.entry(subject).or_insert(Relationship {
            trust: 0,
            affection: 0,
        });
        relationship.trust =
            (i64::from(relationship.trust) + i64::from(delta)).clamp(-1000, 1000) as i32;
    }
    Ok(prepared)
}
/// Apply each received delta in publication order, clamping after every delta.
/// Missing observer minds retain an absence witness and produce no reaction.
pub fn prepare_trust_reactions<A: Component>(
    store: &Store,
    effects: impl IntoIterator<Item = TrustDelta>,
) -> Result<PreparedMindEffects<A>, Error> {
    let mut prepared = PreparedMindEffects::new();
    for effect in effects {
        let Some(mind) = prepared.row(store, effect.observer) else {
            continue;
        };
        let relationship = mind
            .relationships
            .entry(effect.subject)
            .or_insert(Relationship {
                trust: 0,
                affection: 0,
            });
        relationship.trust =
            (i64::from(relationship.trust) + i64::from(effect.delta)).clamp(-1000, 1000) as i32;
    }
    Ok(prepared)
}
pub struct PreparedPlay<A> {
    gain: u64,
    change: PreparedMindEffects<A>,
}
impl<A: Component> PreparedPlay<A> {
    pub fn into_mind_effects(self) -> PreparedMindEffects<A> {
        self.change
    }
    pub fn into_minds(self) -> PreparedMindEffects<A> {
        self.change
    }
    pub fn gain(&self) -> u64 {
        self.gain
    }
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.change.include_reads(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.change.stage_into(store, batch)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.change.commit(store)
    }
}
pub fn prepare_play<A: Component>(
    store: &Store,
    actor: PersonId,
    interest: ChildhoodInterest,
) -> Result<PreparedPlay<A>, Error> {
    let mut change = PreparedMindEffects::default();
    let gain = change.receive_play(store, actor, interest)?;
    Ok(PreparedPlay { gain, change })
}
pub fn prepare_care_relationship<A: Component>(
    store: &Store,
    child: PersonId,
    caregiver: PersonId,
) -> Result<PreparedMindEffects<A>, Error> {
    let mut change = PreparedMindEffects::default();
    change.receive_care_relationship(store, child, caregiver)?;
    Ok(change)
}
pub fn prepare_comfort_relationship<A: Component>(
    store: &Store,
    child: PersonId,
    caregiver: PersonId,
) -> Result<PreparedMindEffects<A>, Error> {
    let mut change = PreparedMindEffects::default();
    change.receive_comfort_relationship(store, child, caregiver)?;
    Ok(change)
}
/// Matching delivery plans consume each actual receipt sequence at most once.
pub fn prepare_care_receipt<A: Component>(
    store: &Store,
    caregiver: PersonId,
    child: PersonId,
    good: Good,
    quantity: Quantity,
    sequence: u64,
) -> Result<PreparedMindEffects<A>, Error> {
    let mut change = PreparedMindEffects::default();
    change.receive_care_receipt(store, caregiver, child, good, quantity, sequence)?;
    Ok(change)
}

pub struct PreparedToldStock {
    reads: ReadSet,
    actor: PersonId,
    after: Stocks,
}
pub fn prepare_told_stock<A: Component>(
    store: &Store,
    actor: PersonId,
    fact: &StockKnowledge,
) -> Result<PreparedToldStock, Error> {
    let mut reads = ReadSet::new();
    reads
        .read::<Mind<A>>(store, StableId(actor.0))
        .ok_or(Error::Unavailable)?;
    let mut after = reads
        .read::<Stocks>(store, StableId(actor.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    after.known.insert(fact.inventory, fact.clone());
    after.validate()?;
    Ok(PreparedToldStock {
        reads,
        actor,
        after,
    })
}
impl PreparedToldStock {
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.reads.extend(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<Stocks>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Stocks>(id);
        batch.insert(id, self.after);
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum StockPlacement {
    Area(AreaId),
    Carried(PersonId),
}
#[derive(Clone, Debug)]
pub struct TellStockFacts {
    pub inventory: InventoryId,
    pub placement: StockPlacement,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
}
pub struct PreparedTell {
    fact: StockKnowledge,
    completes: SimTime,
    stock: PreparedToldStock,
}
impl PreparedTell {
    pub fn fact(&self) -> &StockKnowledge {
        &self.fact
    }
    pub fn completes(&self) -> SimTime {
        self.completes
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.stock.stage_into(store, batch)
    }
}
/// Admission reads canonical identity and raw spatial rows. A local observation
/// outranks the dated personal memory; remote stocks cannot refresh that memory.
pub fn prepare_tell<A: Component>(
    store: &Store,
    actor: PersonId,
    recipient: PersonId,
    inventory: InventoryId,
    now: SimTime,
    visible: Option<TellStockFacts>,
    foreign_reads: ReadSet,
) -> Result<PreparedTell, Error> {
    let mut reads = foreign_reads;
    let sender = reads
        .read::<Location>(store, StableId(actor.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let receiver = reads
        .read::<Location>(store, StableId(recipient.0))
        .cloned();
    reads
        .read::<crate::people::Person>(store, StableId(recipient.0))
        .ok_or(Error::Unavailable)?;
    let Location::AtArea(area) = sender else {
        return Err(Error::Unavailable);
    };
    if actor == recipient || receiver != Some(Location::AtArea(area)) {
        return Err(Error::Unavailable);
    }
    let remembered = reads
        .read::<Stocks>(store, StableId(actor.0))
        .and_then(|row| row.known.get(&inventory))
        .cloned();
    let observed = visible
        .filter(|stock| {
            stock.inventory == inventory
                && match stock.placement {
                    StockPlacement::Area(stock_area) => stock_area == area,
                    StockPlacement::Carried(holder) => holder == actor,
                }
        })
        .map(|stock| StockKnowledge {
            inventory,
            area,
            good: stock.good,
            quantity: stock.quantity,
            accessible: stock.accessible,
            observed_at: now,
            learned_at: now,
            source: KnowledgeSource::Observed(actor),
        });
    let fact = observed.or(remembered).ok_or(Error::Unavailable)?;
    let completes = SimTime(now.0.checked_add(60_000).ok_or(Error::Overflow)?);
    let mut stock = prepare_told_stock::<A>(store, actor, &fact)?;
    stock.include_reads(reads);
    Ok(PreparedTell {
        fact,
        completes,
        stock,
    })
}
#[derive(Clone, Copy, Debug)]
pub enum RecipientActivity {
    Idle,
    Rest,
    Other,
}
#[derive(Clone, Debug)]
pub struct TellCompletionFacts {
    pub sender_location: Location,
    pub recipient_location: Location,
    pub recipient_alive: bool,
    pub recipient_health: u64,
    pub recipient_activity: RecipientActivity,
}
pub fn tell_can_complete(facts: &TellCompletionFacts) -> bool {
    facts.sender_location == facts.recipient_location
        && matches!(facts.sender_location, Location::AtArea(_))
        && facts.recipient_alive
        && facts.recipient_health > 0
        && matches!(
            facts.recipient_activity,
            RecipientActivity::Idle | RecipientActivity::Rest
        )
}
pub struct PreparedSocialReport<A> {
    reads: ReadSet,
    recipient: PersonId,
    mind: Mind<A>,
    stocks: Stocks,
}
/// Personal report reception changes trust even for older evidence; it never
/// transfers the sender's access rights or overwrites a newer stock observation.
pub fn prepare_social_report<A: Component>(
    store: &Store,
    recipient: PersonId,
    sender: PersonId,
    now: SimTime,
    fact: StockKnowledge,
) -> Result<Option<PreparedSocialReport<A>>, Error> {
    let mut reads = ReadSet::new();
    let id = StableId(recipient.0);
    let Some(mut mind) = reads.read::<Mind<A>>(store, id).cloned() else {
        return Ok(None);
    };
    let Some(mut stocks) = reads.read::<Stocks>(store, id).cloned() else {
        return Ok(None);
    };
    if !mind.receive_report(&mut stocks, sender, now, fact) {
        return Ok(None);
    }
    mind.validate()?;
    stocks.validate()?;
    Ok(Some(PreparedSocialReport {
        reads,
        recipient,
        mind,
        stocks,
    }))
}
pub fn prepare_tell_completion<A: Component>(
    store: &Store,
    actor: PersonId,
    recipient: PersonId,
    now: SimTime,
    fact: StockKnowledge,
    facts: &TellCompletionFacts,
    foreign_reads: ReadSet,
) -> Result<Option<PreparedSocialReport<A>>, Error> {
    if !tell_can_complete(facts) {
        return Ok(None);
    }
    let Some(mut report) = prepare_social_report::<A>(store, recipient, actor, now, fact)? else {
        return Ok(None);
    };
    report.reads.extend(foreign_reads);
    Ok(Some(report))
}
impl<A: Component> PreparedSocialReport<A> {
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.reads.extend(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.recipient.0);
        if batch.touches::<Mind<A>>(id) || batch.touches::<Stocks>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Mind<A>>(id);
        admitted.protect::<Stocks>(id);
        batch.insert(id, self.mind);
        batch.insert(id, self.stocks);
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

/// A communication outcome always retains its source witnesses, including a
/// refused attempt. The caller can publish the corresponding typed event only
/// after this admitted projection has been validated with the shared batch.
pub struct PreparedReportReception<A> {
    reads: ReadSet,
    recipient: PersonId,
    after: Option<(Mind<A>, Stocks)>,
}
impl<A: Component> PreparedReportReception<A> {
    pub fn accepted(&self) -> bool {
        self.after.is_some()
    }
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.reads.extend(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.recipient.0);
        let mut admitted = ReadFootprint::new();
        if let Some((mind, stocks)) = self.after {
            if batch.touches::<Mind<A>>(id) || batch.touches::<Stocks>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Mind<A>>(id);
            admitted.protect::<Stocks>(id);
            batch.insert(id, mind);
            batch.insert(id, stocks);
        }
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
pub fn prepare_report_reception<A: Component>(
    store: &Store,
    recipient: PersonId,
    sender: PersonId,
    now: SimTime,
    fact: StockKnowledge,
) -> Result<PreparedReportReception<A>, Error> {
    let mut reads = ReadSet::new();
    let id = StableId(recipient.0);
    let mind = reads.read::<Mind<A>>(store, id).cloned();
    let stocks = reads.read::<Stocks>(store, id).cloned();
    let after = if let (Some(mut mind), Some(mut stocks)) = (mind, stocks) {
        if mind.receive_report(&mut stocks, sender, now, fact) {
            mind.validate()?;
            stocks.validate()?;
            Some((mind, stocks))
        } else {
            None
        }
    } else {
        None
    };
    Ok(PreparedReportReception {
        reads,
        recipient,
        after,
    })
}
pub fn prepare_tell_result<A: Component>(
    store: &Store,
    actor: PersonId,
    recipient: PersonId,
    now: SimTime,
    fact: StockKnowledge,
    facts: &TellCompletionFacts,
    foreign_reads: ReadSet,
) -> Result<PreparedReportReception<A>, Error> {
    let mut result = if tell_can_complete(facts) {
        prepare_report_reception::<A>(store, recipient, actor, now, fact)?
    } else {
        PreparedReportReception {
            reads: ReadSet::new(),
            recipient,
            after: None,
        }
    };
    result.reads.extend(foreign_reads);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::{Person, insert_person, register_people};
    use game_ecs::Schema;
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Readiness {
        alive: bool,
        health: u64,
        activity: u8,
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Foreign(u64);
    fn fixture() -> Store {
        let mut schema = Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        register_people(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        schema
            .register::<Readiness>("test/readiness", |_| Ok(()))
            .unwrap();
        schema
            .register::<Foreign>("test/foreign", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema);
        for actor in 1..=3 {
            insert_person(
                &mut store,
                Person {
                    id: PersonId(actor),
                    role: if actor == 1 {
                        Role::Ruler
                    } else {
                        Role::Citizen
                    },
                    inventory: InventoryId(actor + 100),
                    work_units: 0,
                    work_remainder: 0,
                    work_rate: 1000,
                    hunger_exempt: false,
                },
            )
            .unwrap();
            insert_mind::<u8>(
                &mut store,
                PersonId(actor),
                Mind::new(Personality::industrious(), Aspiration::Comfort).unwrap(),
            )
            .unwrap();
            game_spatial::set_location(&mut store, StableId(actor), Location::AtArea(AreaId(10)))
                .unwrap();
            store
                .insert_or_spawn(
                    StableId(actor),
                    Readiness {
                        alive: true,
                        health: 1000,
                        activity: 0,
                    },
                )
                .unwrap();
        }
        store
    }
    fn fact(at: u64, quantity: u64, accessible: bool) -> StockKnowledge {
        StockKnowledge {
            inventory: InventoryId(20),
            area: AreaId(10),
            good: Good::Food,
            quantity: Quantity(quantity),
            accessible,
            observed_at: SimTime(at),
            learned_at: SimTime(at),
            source: KnowledgeSource::Observed(PersonId(1)),
        }
    }
    fn completion(store: &Store) -> (TellCompletionFacts, ReadSet) {
        let mut reads = ReadSet::new();
        let sender_location = reads.read::<Location>(store, StableId(1)).unwrap().clone();
        let recipient_location = reads.read::<Location>(store, StableId(2)).unwrap().clone();
        let row = reads.read::<Readiness>(store, StableId(2)).unwrap();
        (
            TellCompletionFacts {
                sender_location,
                recipient_location,
                recipient_alive: row.alive,
                recipient_health: row.health,
                recipient_activity: match row.activity {
                    0 => RecipientActivity::Idle,
                    1 => RecipientActivity::Rest,
                    _ => RecipientActivity::Other,
                },
            },
            reads,
        )
    }
    #[test]
    fn completed_play_uses_exact_owner_gain_and_guards_publication() {
        for (experience, gain) in [(0, 100), (950, 50), (1000, 0)] {
            let mut store = fixture();
            mind_mut::<u8>(&mut store, PersonId(1))
                .unwrap()
                .interests
                .insert(ChildhoodInterest::Craft, experience);
            let prepared =
                prepare_play::<u8>(&store, PersonId(1), ChildhoodInterest::Craft).unwrap();
            assert_eq!(prepared.gain(), gain);
            let mut batch = Batch::new();
            let guard = prepared.stage_into(&store, &mut batch).unwrap();
            batch.insert(StableId(2), Foreign(7));
            guard.validate(&store, &batch).unwrap();
            store.apply_batch(batch).unwrap();
            assert_eq!(
                mind::<u8>(&store, PersonId(1)).unwrap().interests[&ChildhoodInterest::Craft],
                experience + gain
            );
        }
        let mut store = fixture();
        let prepared = prepare_play::<u8>(&store, PersonId(1), ChildhoodInterest::Craft).unwrap();
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .interests
            .insert(ChildhoodInterest::Craft, 50);
        let before = store.clone();
        assert!(prepared.commit(&mut store).is_err());
        assert_eq!(store, before);
    }
    #[test]
    fn physical_receivers_compose_on_one_row_before_one_guarded_write() {
        let mut store = fixture();
        let actor = PersonId(1);
        let subject = PersonId(2);
        let mut effects = PreparedMindEffects::<u8>::default();
        assert_eq!(
            effects
                .receive_play(&store, actor, ChildhoodInterest::Craft)
                .unwrap(),
            100
        );
        effects
            .receive_care_relationship(&store, actor, subject)
            .unwrap();
        effects
            .receive_comfort_relationship(&store, actor, subject)
            .unwrap();
        let mut personality = effects.projected_mind(actor).unwrap().personality.clone();
        personality.empathy = 700;
        effects.set_personality(&store, actor, personality).unwrap();
        let mut batch = Batch::new();
        let guard = effects.stage_into(&store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        let received = mind::<u8>(&store, actor).unwrap();
        assert_eq!(received.interests[&ChildhoodInterest::Craft], 100);
        assert_eq!(received.personality.empathy, 700);
        assert_eq!(
            received.relationships[&subject],
            Relationship {
                trust: 50,
                affection: 100,
            }
        );
    }
    #[test]
    fn care_relationship_and_delivery_receipt_share_one_admitted_batch() {
        let mut store = fixture();
        let child = PersonId(2);
        let caregiver = PersonId(1);
        mind_mut::<u8>(&mut store, child)
            .unwrap()
            .relationships
            .insert(
                caregiver,
                Relationship {
                    trust: 980,
                    affection: 990,
                },
            );
        mind_mut::<u8>(&mut store, caregiver).unwrap().plan = Some(Plan {
            action: 1,
            key: IntentionKey::Provision {
                good: Good::Food,
                beneficiary: child,
                source: None,
            },
            motives: vec![],
            prospects: Prospects::default(),
            shape: ActionShape::Care(child),
            delivery: Some(
                game_logistics::Delivery::new(
                    game_logistics::Requirement {
                        good: Good::Food,
                        quantity: Quantity(1000),
                        source: None,
                    },
                    game_logistics::Destination::Contact(child),
                )
                .unwrap(),
            ),
        });
        let relationship = prepare_care_relationship::<u8>(&store, child, caregiver).unwrap();
        let receipt =
            prepare_care_receipt::<u8>(&store, caregiver, child, Good::Food, Quantity(400), 7)
                .unwrap();
        let mut batch = Batch::new();
        let relationship_guard = relationship.stage_into(&store, &mut batch).unwrap();
        let receipt_guard = receipt.stage_into(&store, &mut batch).unwrap();
        relationship_guard.validate(&store, &batch).unwrap();
        receipt_guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(
            mind::<u8>(&store, child).unwrap().relationships[&caregiver],
            Relationship {
                trust: 1000,
                affection: 1000
            }
        );
        let before = mind::<u8>(&store, caregiver).unwrap().clone();
        assert_eq!(
            before
                .plan
                .as_ref()
                .unwrap()
                .delivery
                .as_ref()
                .unwrap()
                .remaining,
            Quantity(600)
        );
        prepare_care_receipt::<u8>(&store, caregiver, child, Good::Food, Quantity(400), 7)
            .unwrap()
            .commit(&mut store)
            .unwrap();
        assert_eq!(mind::<u8>(&store, caregiver), Some(&before));
        let mut fresh = fixture();
        prepare_comfort_relationship::<u8>(&fresh, child, caregiver)
            .unwrap()
            .commit(&mut fresh)
            .unwrap();
        assert_eq!(
            mind::<u8>(&fresh, child).unwrap().relationships[&caregiver],
            Relationship {
                trust: 0,
                affection: 50
            }
        );
    }
    #[test]
    fn trust_receiving_clamps_sequentially_skips_absent_minds_and_rejects_overwrites() {
        let mut store = fixture();
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .relationships
            .insert(
                PersonId(2),
                Relationship {
                    trust: 900,
                    affection: 50,
                },
            );
        let prepared = prepare_trust_reactions::<u8>(
            &store,
            [
                TrustDelta {
                    observer: PersonId(1),
                    subject: PersonId(2),
                    delta: 500,
                },
                TrustDelta {
                    observer: PersonId(1),
                    subject: PersonId(2),
                    delta: -500,
                },
                TrustDelta {
                    observer: PersonId(99),
                    subject: PersonId(2),
                    delta: 100,
                },
            ],
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        assert!(guard.validate(&store, &Batch::new()).is_err());
        store.apply_batch(batch).unwrap();
        assert_eq!(
            mind::<u8>(&store, PersonId(1)).unwrap().relationships[&PersonId(2)],
            Relationship {
                trust: 500,
                affection: 50
            }
        );
        assert!(mind::<u8>(&store, PersonId(99)).is_none());
        let prepared = prepare_trust_reactions::<u8>(
            &store,
            [TrustDelta {
                observer: PersonId(1),
                subject: PersonId(2),
                delta: 10,
            }],
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&store, &mut batch).unwrap();
        batch.insert(
            StableId(1),
            mind::<u8>(&store, PersonId(1)).unwrap().clone(),
        );
        assert!(guard.validate(&store, &batch).is_err());
    }
    #[test]
    fn default_knowledge_receiving_uses_guarded_honesty_and_sequential_floor() {
        let mut store = fixture();
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .personality
            .honesty = 800;
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .relationships
            .insert(
                PersonId(2),
                Relationship {
                    trust: -995,
                    affection: 40,
                },
            );
        prepare_default_knowledge_reactions::<u8>(
            &store,
            [
                (PersonId(1), PersonId(2)),
                (PersonId(1), PersonId(2)),
                (PersonId(99), PersonId(2)),
            ],
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
        assert_eq!(
            mind::<u8>(&store, PersonId(1)).unwrap().relationships[&PersonId(2)],
            Relationship {
                trust: -1000,
                affection: 40
            }
        );
        let prepared =
            prepare_default_knowledge_reactions::<u8>(&store, [(PersonId(1), PersonId(3))])
                .unwrap();
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .personality
            .honesty = 100;
        let before = store.clone();
        assert!(prepared.commit(&mut store).is_err());
        assert_eq!(store, before);
    }
    #[test]
    fn tell_admission_prefers_local_observation_and_preserves_remote_dated_belief() {
        let mut store = fixture();
        stocks_mut(&mut store, PersonId(1))
            .unwrap()
            .known
            .insert(InventoryId(20), fact(10, 1000, false));
        let local = TellStockFacts {
            inventory: InventoryId(20),
            placement: StockPlacement::Area(AreaId(10)),
            good: Good::Food,
            quantity: Quantity(50),
            accessible: true,
        };
        let prepared = prepare_tell::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            InventoryId(20),
            SimTime(100),
            Some(local),
            ReadSet::new(),
        )
        .unwrap();
        assert_eq!(prepared.completes(), SimTime(60_100));
        assert_eq!(prepared.fact().quantity, Quantity(50));
        assert_eq!(
            prepared.fact().source,
            KnowledgeSource::Observed(PersonId(1))
        );
        let expected = prepared.fact().clone();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(
            stocks(&store, PersonId(1)).unwrap().known[&InventoryId(20)],
            expected
        );
        let remote = TellStockFacts {
            inventory: InventoryId(20),
            placement: StockPlacement::Carried(PersonId(3)),
            good: Good::Food,
            quantity: Quantity(9999),
            accessible: true,
        };
        let prepared = prepare_tell::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            InventoryId(20),
            SimTime(200),
            Some(remote),
            ReadSet::new(),
        )
        .unwrap();
        assert_eq!(prepared.fact(), &expected);
        assert!(
            prepare_tell::<u8>(
                &store,
                PersonId(1),
                PersonId(1),
                InventoryId(20),
                SimTime(200),
                None,
                ReadSet::new()
            )
            .is_err()
        );
        game_spatial::set_location(&mut store, StableId(2), Location::AtArea(AreaId(11))).unwrap();
        let before = store.clone();
        assert!(prepared.stage_into(&store, &mut Batch::new()).is_err());
        assert_eq!(store, before);
    }
    #[test]
    fn tell_completion_guards_readiness_and_keeps_recipient_rights_newer_dates_and_trust() {
        let mut store = fixture();
        stocks_mut(&mut store, PersonId(2))
            .unwrap()
            .known
            .insert(InventoryId(20), fact(150, 300, true));
        let (facts, reads) = completion(&store);
        let prepared = prepare_tell_result::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(200),
            fact(100, 1000, false),
            &facts,
            reads,
        )
        .unwrap();
        assert!(prepared.accepted());
        prepared.commit(&mut store).unwrap();
        let remembered = &stocks(&store, PersonId(2)).unwrap().known[&InventoryId(20)];
        assert_eq!(remembered.observed_at, SimTime(150));
        assert_eq!(remembered.quantity, Quantity(300));
        assert!(remembered.accessible);
        assert_eq!(
            mind::<u8>(&store, PersonId(2)).unwrap().relationships[&PersonId(1)].trust,
            10
        );
        let (facts, reads) = completion(&store);
        let prepared = prepare_tell_result::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(200),
            fact(175, 500, false),
            &facts,
            reads,
        )
        .unwrap();
        store.get_mut::<Readiness>(StableId(2)).unwrap().activity = 2;
        let before = store.clone();
        assert!(prepared.commit(&mut store).is_err());
        assert_eq!(store, before);
        let (facts, reads) = completion(&store);
        let prepared = prepare_tell_result::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(200),
            fact(175, 500, false),
            &facts,
            reads,
        )
        .unwrap();
        assert!(!prepared.accepted());
        prepared.commit(&mut store).unwrap();
        assert_eq!(store, before);
        store.get_mut::<Readiness>(StableId(2)).unwrap().activity = 1;
        mind_mut::<u8>(&mut store, PersonId(2))
            .unwrap()
            .relationships
            .get_mut(&PersonId(1))
            .unwrap()
            .trust = -501;
        let (facts, reads) = completion(&store);
        let prepared = prepare_tell_result::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(200),
            fact(175, 500, false),
            &facts,
            reads,
        )
        .unwrap();
        assert!(!prepared.accepted());
        mind_mut::<u8>(&mut store, PersonId(2))
            .unwrap()
            .relationships
            .get_mut(&PersonId(1))
            .unwrap()
            .trust = 0;
        let before = store.clone();
        assert!(prepared.commit(&mut store).is_err());
        assert_eq!(store, before);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LessonKnowledge {
    Craft { skill_level: u16, lesson_gain: u64 },
    Travel { lesson_gain: u64 },
}
pub struct PreparedLesson<A> {
    gain: u64,
    change: PreparedMindEffects<A>,
}
impl<A: Component> PreparedLesson<A> {
    pub fn into_mind_effects(self) -> PreparedMindEffects<A> {
        self.change
    }
    pub fn gain(&self) -> u64 {
        self.gain
    }
    pub fn include_reads(&mut self, reads: ReadSet) {
        self.change.include_reads(reads);
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.change.stage_into(store, batch)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.change.commit(store)
    }
}
/// A lesson transmits only the teacher's personal mastery and remembered routes.
/// Invocation learning belongs to Lineage, and skill evidence comes from Skills.
pub fn prepare_lineage_lesson<A: Component>(
    store: &Store,
    teacher: PersonId,
    student: PersonId,
    knowledge: LessonKnowledge,
) -> Result<PreparedLesson<A>, Error> {
    let mut change = PreparedMindEffects::new();
    let teacher = change
        .reads
        .read::<Mind<A>>(store, StableId(teacher.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    teacher.validate()?;
    let learner = change.row(store, student).ok_or(Error::Unavailable)?;
    let gain = match knowledge {
        LessonKnowledge::Craft {
            skill_level,
            lesson_gain,
        } => {
            let mastery = teacher
                .interests
                .get(&ChildhoodInterest::Craft)
                .copied()
                .unwrap_or(0)
                .max(u64::from(skill_level) * 10);
            let learning = learner
                .interests
                .entry(ChildhoodInterest::Craft)
                .or_default();
            let gain = lesson_gain.min(mastery.saturating_sub(*learning));
            *learning = learning.checked_add(gain).ok_or(Error::Overflow)?;
            gain
        }
        LessonKnowledge::Travel { lesson_gain } => {
            let mut new_links = 0_u64;
            for (area, links) in teacher.routes {
                let own = learner.routes.entry(area).or_default();
                for (destination, duration) in links {
                    if own.insert(destination, duration).is_none() {
                        new_links = new_links.checked_add(1).ok_or(Error::Overflow)?;
                    }
                }
            }
            new_links.min(lesson_gain)
        }
    };
    learner.validate()?;
    Ok(PreparedLesson { gain, change })
}

#[cfg(test)]
mod lesson_tests {
    use super::*;
    fn fixture() -> Store {
        let mut schema = game_ecs::Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for actor in [PersonId(1), PersonId(2)] {
            insert_mind::<u8>(
                &mut store,
                actor,
                Mind::new(Personality::industrious(), Aspiration::Comfort).unwrap(),
            )
            .unwrap();
        }
        store
    }
    #[test]
    fn prepared_craft_lesson_uses_teacher_memory_and_supplied_skill_ceiling() {
        let mut store = fixture();
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .interests
            .insert(ChildhoodInterest::Craft, 500);
        mind_mut::<u8>(&mut store, PersonId(2))
            .unwrap()
            .interests
            .insert(ChildhoodInterest::Craft, 750);
        let prepared = prepare_lineage_lesson::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            LessonKnowledge::Craft {
                skill_level: 80,
                lesson_gain: 200,
            },
        )
        .unwrap();
        assert_eq!(prepared.gain(), 50);
        prepared.commit(&mut store).unwrap();
        assert_eq!(
            mind::<u8>(&store, PersonId(2)).unwrap().interests[&ChildhoodInterest::Craft],
            800
        );
        let prepared = prepare_lineage_lesson::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            LessonKnowledge::Craft {
                skill_level: 80,
                lesson_gain: 200,
            },
        )
        .unwrap();
        assert_eq!(prepared.gain(), 0);
        prepared.commit(&mut store).unwrap();
        assert_eq!(
            mind::<u8>(&store, PersonId(2)).unwrap().interests[&ChildhoodInterest::Craft],
            800
        );
    }
    #[test]
    fn prepared_travel_lesson_copies_all_personal_edges_but_credits_only_new_links() {
        let mut store = fixture();
        mind_mut::<u8>(&mut store, PersonId(1)).unwrap().routes = BTreeMap::from([(
            AreaId(1),
            BTreeMap::from([
                (AreaId(2), SimDuration(10)),
                (AreaId(3), SimDuration(20)),
                (AreaId(4), SimDuration(30)),
            ]),
        )]);
        mind_mut::<u8>(&mut store, PersonId(2)).unwrap().routes =
            BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(9))]))]);
        let prepared = prepare_lineage_lesson::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            LessonKnowledge::Travel { lesson_gain: 1 },
        )
        .unwrap();
        assert_eq!(prepared.gain(), 1);
        prepared.commit(&mut store).unwrap();
        assert_eq!(
            mind::<u8>(&store, PersonId(2)).unwrap().routes,
            mind::<u8>(&store, PersonId(1)).unwrap().routes
        );
        let prepared = prepare_lineage_lesson::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            LessonKnowledge::Travel { lesson_gain: 1 },
        )
        .unwrap();
        assert_eq!(prepared.gain(), 0);
        mind_mut::<u8>(&mut store, PersonId(1))
            .unwrap()
            .routes
            .get_mut(&AreaId(1))
            .unwrap()
            .insert(AreaId(2), SimDuration(11));
        let before = store.clone();
        assert!(prepared.commit(&mut store).is_err());
        assert_eq!(store, before);
    }
}
