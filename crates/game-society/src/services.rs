//! Native read requests for stock access backed by institutional grants.
use crate::{
    institutions::{AccessRuleFacts, AuthorityFacts, Runtime},
    review,
};
use game_cargo::Inventory;
use game_core::{Error, InventoryId, PersonId};
use game_ecs::StableId;
use game_physiology::services::ReadBody;
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use std::collections::{BTreeMap, BTreeSet};

pub use crate::institutions::native::{
    ClearSocialLearning, EnforcementReaction, ExecuteInstitution, ExecuteInstitutionForEnforcement,
    InstitutionAreaExists, ReadAuthority, ReadInstitution, ReadInstitutionCommands,
    ReadInstitutionMemory, ReadInstitutionSite, ReadInstitutionViews, ReadInstitutionsAvailable,
    ReadRuleProposal, ReadSocialCaseViews, ReadSocialPenalty, ReceiveEnforcementOutcome,
    ReceiveInstitutionMemory, ReceiveRegionalOutcome, ReceiveRegionalPublications,
    ReceiveSocialReceipts, ReviewInstitutions, ScoreEnforcementChange, ScoreRegionalChange,
    StartInstitutionWork, SubmitRegionalPassage, ValidateEnforcementChange,
    ValidateInstitutionEvent, ValidateInstitutions, ValidateRegionalChange,
};

/// Resolve a person's effective access to one inventory from current Cargo and
/// Society rows, with office authority grounded in current body liveness.
pub struct ReadInventoryAccess {
    pub actor: PersonId,
    pub inventory: InventoryId,
}
impl Request for ReadInventoryAccess {
    type Response = bool;
}

/// An inactive personal domain preserves the legacy opt-in family behavior.
pub struct ReadFamilyIntent {
    pub actor: PersonId,
    pub partner: PersonId,
}
impl Request for ReadFamilyIntent {
    type Response = bool;
}

#[derive(Default)]
pub struct Services {
    history: Option<Box<dyn std::any::Any + Send + Sync>>,
}
impl Services {
    pub(crate) fn history_event<E: 'static>(
        &self,
        event: &E,
    ) -> Option<crate::institutions::native::HistoryEvent> {
        self.history
            .as_ref()?
            .downcast_ref::<fn(&E) -> Option<crate::institutions::native::HistoryEvent>>()?(
            event
        )
    }
    pub(crate) fn has_history(&self) -> bool {
        self.history.is_some()
    }
}
impl<E: 'static> OwnerSystem<E> for Services {
    type Runtime = Option<Runtime>;
    type Input = ();
    type Output = ();

    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}

pub fn read_inventory_access<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: ReadInventoryAccess,
) -> Result<bool, Error> {
    let Some(inventory) = tx.read::<Inventory>(StableId(request.inventory.0)).cloned() else {
        return Ok(false);
    };
    let permission = institutional_permission(tx, request.actor, request.inventory)?;
    Ok(game_cargo::effective_access(
        inventory.access.contains(&request.actor),
        permission,
    ))
}

fn institutional_permission<E: 'static>(
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    actor: PersonId,
    inventory: InventoryId,
) -> Result<bool, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(false);
    };

    let institutions = tx
        .iter::<crate::institutions::Institution>()
        .into_iter()
        .map(|(_, institution)| institution)
        .collect::<Vec<_>>();
    let authority_people = institutions
        .iter()
        .flat_map(review::authority_people)
        .collect::<BTreeSet<_>>();
    let mut authority = AuthorityFacts::default();
    for person in authority_people {
        if tx
            .request(ReadBody(person))?
            .is_some_and(|body| body.vitals.alive)
        {
            authority.alive.insert(person);
        }
    }

    let mut concepts = BTreeMap::new();
    for id in institutions
        .iter()
        .flat_map(|institution| institution.concepts.iter().copied())
        .collect::<BTreeSet<_>>()
    {
        if let Some(concept) = tx.read::<crate::Concept>(StableId(id.0)).cloned() {
            concepts.insert(id, concept);
        }
    }
    let policy = AccessRuleFacts {
        institutions,
        concepts,
    };
    Ok(runtime.allows_access_in_facts(actor, inventory, &authority, &policy))
}

pub fn read_stock_permission<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: game_cargo::facts::ReadStockPermission,
) -> Result<bool, Error> {
    institutional_permission(tx, request.actor, request.inventory)
}

/// Register the existing Society runtime slot and the native access query.
pub fn register_services<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
) -> Result<(), Error> {
    register_services_owner(registry, slot, Services::default())
}
/// The application supplies only its public event envelope's structural codec.
/// Society retains cursor admission, factual joins and reaction decisions.
pub fn register_services_with_history<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
    history: fn(&E) -> Option<crate::institutions::native::HistoryEvent>,
) -> Result<(), Error> {
    register_services_owner(
        registry,
        slot,
        Services {
            history: Some(Box::new(history)),
        },
    )
}
fn register_services_owner<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
    owner: Services,
) -> Result<(), Error> {
    registry.register(owner, slot)?;
    registry.register_read_request::<Services, ReadInventoryAccess>(read_inventory_access::<E>)?;
    registry.register_read_request::<Services, game_cargo::facts::ReadStockPermission>(
        read_stock_permission::<E>,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Concept,
        institutions::{InstitutionStatus, RuleExpression, SocialCondition, SocialEffect},
        policy_fixtures::{charter, institution},
    };
    use game_core::ConceptId;

    fn access_policy(grant: bool) -> AccessRuleFacts {
        let actor = PersonId(12);
        let holder = PersonId(2);
        let rule = crate::institutions::InstitutionalRule {
            subjects: crate::institutions::Subjects::members(),
            when: RuleExpression::Fact(SocialCondition::Local),
            effect: SocialEffect::Access {
                inventory: InventoryId(4),
                allowed: true,
            },
            authority: 0,
        };
        let mut institution = institution(10, vec![rule.clone()], InstitutionStatus::Active);
        institution.charter = charter(vec![rule.clone()]);
        institution.members.extend([actor, holder]);
        institution.concepts = vec![ConceptId(3)];
        institution
            .office_support
            .insert(0, BTreeSet::from([actor]));
        institution.grants.insert(
            ConceptId(3),
            if grant {
                BTreeSet::from([actor])
            } else {
                BTreeSet::new()
            },
        );
        AccessRuleFacts {
            institutions: vec![institution],
            concepts: BTreeMap::from([(ConceptId(3), Concept { rule })]),
        }
    }

    #[test]
    fn access_query_respects_personal_grants_policy_grants_and_unknown_policy() {
        let runtime = Runtime::default();
        let actor = PersonId(12);
        let inventory = InventoryId(4);
        let mut authority = AuthorityFacts::default();
        authority.alive.extend([PersonId(2), actor]);

        assert!(runtime.allows_access_in_facts(actor, inventory, &authority, &access_policy(true)));
        assert!(!runtime.allows_access_in_facts(
            actor,
            inventory,
            &authority,
            &access_policy(false)
        ));
        assert!(!runtime.allows_access_in_facts(
            actor,
            inventory,
            &authority,
            &AccessRuleFacts::default()
        ));
        assert!(game_cargo::effective_access(true, false));
        assert!(!game_cargo::effective_access(false, false));
    }
}
