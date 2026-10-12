//! Scoped local facts supply authority and dated observations without hidden policy.
use crate::*;
use game_core::*;
use game_ecs::{Component, Store};
use game_society::institutions::{
    AuthorityFacts, Institution, InstitutionStatus, InstitutionView, OfficePower,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorFacts {
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub in_combat: bool,
    pub location: Location,
    pub idle_or_rest: bool,
    pub foreign_talking: bool,
    pub controlled: bool,
    pub past_life_controlled: bool,
}
pub struct RealmContext<'a, P: Presence> {
    pub now: SimTime,
    pub authority: &'a AuthorityFacts,
    pub presence: &'a P,
}
pub trait Presence {
    type Activity: Component;
    /// Factual native rows behind the supplied actor availability and contact.
    fn actor_reads(&self, actor: PersonId) -> game_ecs::ReadFootprint;
    fn inventory_reads(&self, inventory: InventoryId) -> game_ecs::ReadFootprint;
    fn actor(&self, actor: PersonId) -> Option<ActorFacts>;
    fn same_place(&self, a: PersonId, b: PersonId) -> bool;
    fn people(&self) -> impl Iterator<Item = PersonId>;
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId>;
    fn inventory_area(&self, inventory: InventoryId) -> Option<AreaId>;
}
pub fn contact_area(facts: ActorFacts) -> Option<AreaId> {
    if !facts.alive || facts.health == 0 || !facts.independent || facts.in_combat {
        return None;
    }
    match facts.location {
        Location::AtArea(area) => Some(area),
        _ => None,
    }
}
/// Receiving policy keeps caps and tie ordering independent of provider storage.
pub fn positioned_people(
    presence: &impl Presence,
    area: AreaId,
) -> std::collections::btree_set::IntoIter<PersonId> {
    presence
        .local_people(area)
        .collect::<BTreeSet<_>>()
        .into_iter()
}
pub fn actor_area(presence: &impl Presence, actor: PersonId) -> Option<AreaId> {
    contact_area(presence.actor(actor)?)
}
pub fn institution_view(store: &Store, actor: PersonId) -> Vec<InstitutionView> {
    game_society::institutions::Runtime::default().institution_views(store, actor)
}
impl Runtime {
    pub fn proposal_facts<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        proposal: &Proposal,
        fresh_local: bool,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<ProposalFacts, Error> {
        self.proposal_facts_with_views::<A>(
            store,
            actor,
            proposal,
            fresh_local,
            context,
            &institution_view(store, actor),
        )
    }
    pub(crate) fn proposal_facts_with_views<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        proposal: &Proposal,
        fresh_local: bool,
        context: &RealmContext<'_, impl Presence>,
        cached: &[InstitutionView],
    ) -> Result<ProposalFacts, Error> {
        let (now, authority, presence) = (context.now, context.authority, context.presence);
        let mind = game_characters::decisions::mind::<A>(store, actor).ok_or(Error::Unavailable)?;
        let mut facts = ProposalFacts {
            known_people: BTreeSet::from([actor]),
            known_areas: mind
                .routes
                .keys()
                .copied()
                .chain(mind.routes.values().flat_map(|links| links.keys().copied()))
                .collect(),
            ..ProposalFacts::default()
        };
        for (id, k) in &game_characters::decisions::stocks(store, actor)
            .ok_or(Error::Unavailable)?
            .known
        {
            let capacity = game_cargo::inventory(store, *id)
                .filter(|_| {
                    presence.inventory_area(*id) == actor_area(presence, actor)
                        && k.observed_at == now
                })
                .map(|i| i.capacity);
            facts.inventories.insert(
                *id,
                InventoryFacts {
                    good: k.good,
                    capacity,
                },
            );
        }
        for p in proposal.participants.values() {
            let known = cached
                .iter()
                .find(|i| i.id == p.institution)
                .ok_or(Error::Unavailable)?;
            facts.known_people.extend(known.members.iter().copied());
            let office = known
                .offices
                .iter()
                .find(|o| {
                    o.mandate.holder == p.representative
                        && o.effective
                        && o.mandate.powers.contains(&OfficePower::Negotiate)
                })
                .ok_or(Error::Unavailable)?;
            let mut mandate = &office.mandate;
            let mut authorized = known.status == InstitutionStatus::Active;
            if fresh_local && p.representative == actor {
                let actual =
                    game_society::institution(store, p.institution).ok_or(Error::Unavailable)?;
                mandate = actual
                    .charter
                    .offices
                    .iter()
                    .find(|o| o.slot == mandate.slot && o.holder == actor)
                    .ok_or(Error::Unavailable)?;
                authorized = actual.charter.jurisdiction
                    == actor_area(presence, actor).ok_or(Error::Unavailable)?
                    && actual.office_effective(mandate, authority)
                    && mandate.powers.contains(&OfficePower::Negotiate);
            }
            let delivery_caps = if mandate.powers.contains(&OfficePower::Delivery) {
                mandate
                    .inventories
                    .iter()
                    .map(|id| (*id, Quantity(u64::MAX)))
                    .collect()
            } else {
                BTreeMap::new()
            };
            facts.representatives.insert(
                p.institution,
                RepresentativeFacts {
                    settlement: known.charter.jurisdiction,
                    representative: p.representative,
                    may_negotiate: authorized,
                    delivery_caps,
                    supply_caps: mandate
                        .inventories
                        .iter()
                        .map(|id| (*id, Quantity(u64::MAX)))
                        .collect(),
                    passage_areas: BTreeSet::from([known.charter.jurisdiction]),
                    protection_areas: facts.known_areas.clone(),
                },
            );
        }
        facts.known_people.extend(
            actor_area(presence, actor)
                .into_iter()
                .flat_map(|area| positioned_people(presence, area))
                .filter(|p| actor_area(presence, *p) == actor_area(presence, actor))
                .take(64),
        );
        Ok(facts)
    }
    pub fn change_actor_allowed(
        &self,
        store: &Store,
        actor: PersonId,
        institution: &Institution,
        change: &game_society::institutions::InstitutionChange,
        proposing: bool,
        context: &RealmContext<'_, impl Presence>,
    ) -> bool {
        let (authority, presence) = (context.authority, context.presence);
        use game_society::institutions::InstitutionChange;
        let (id, revision) = match change {
            InstitutionChange::RatifyRegional {
                commitment,
                revision,
                ..
            }
            | InstitutionChange::AuthorizeRegionalPassage {
                commitment,
                revision,
                ..
            } => (*commitment, *revision),
            _ => return true,
        };
        let Some(known) = Ledger::new(store)
            .view(actor, id)
            .and_then(|k| k.revisions.get(&revision))
        else {
            return false;
        };
        if !proposing {
            return true;
        }
        known
            .proposal
            .value
            .participants
            .get(&institution.id)
            .is_some_and(|p| {
                p.representative == actor
                    && actor_area(presence, actor) == Some(institution.charter.jurisdiction)
                    && institution.charter.offices.iter().any(|o| {
                        o.holder == actor
                            && o.powers.contains(&OfficePower::Negotiate)
                            && institution.office_effective(o, authority)
                    })
            })
    }
}

impl Runtime {
    /// Bound factual institutional references for an actor's current known offers.
    pub fn authority_institutions(
        &self,
        store: &Store,
        actor: PersonId,
        extra: &[InstitutionId],
    ) -> BTreeSet<InstitutionId> {
        knowledge(store, actor)
            .into_iter()
            .flat_map(|known| {
                known.commitments.iter().rev().take(64).flat_map(|(_, k)| {
                    k.revisions
                        .values()
                        .flat_map(|r| r.proposal.value.participants.keys().copied())
                })
            })
            .chain(extra.iter().copied())
            .collect()
    }
}

impl Runtime {
    pub(crate) fn proposal_reads<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        proposal: &Proposal,
        context: &RealmContext<'_, impl Presence>,
    ) -> game_ecs::ReadFootprint {
        use game_ecs::{ReadFootprint, StableId};
        let mut reads = ReadFootprint::new();
        reads.protect::<game_characters::decisions::Mind<A>>(StableId(actor.0));
        reads.protect::<game_characters::decisions::Stocks>(StableId(actor.0));
        reads.protect::<game_society::Knowledge>(StableId(actor.0));
        reads.extend(context.presence.actor_reads(actor));
        if let Some(stocks) = game_characters::decisions::stocks(store, actor) {
            for id in stocks.known.keys() {
                reads.extend(context.presence.inventory_reads(*id));
            }
        }
        for participant in proposal
            .participants
            .values()
            .filter(|p| p.representative == actor)
        {
            reads.protect::<Institution>(StableId(participant.institution.0));
            for person in game_society::authority_people(store, [participant.institution]) {
                reads.extend(context.presence.actor_reads(person));
            }
        }
        if let Some(area) = actor_area(context.presence, actor) {
            for person in positioned_people(context.presence, area) {
                reads.extend(context.presence.actor_reads(person));
            }
        }
        reads
    }
}
