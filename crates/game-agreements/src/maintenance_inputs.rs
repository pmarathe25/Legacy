//! Native input closure for receipt reconciliation and remembered observation.
use crate::*;

/// A coordinator may copy precisely these owner columns into a compact native
/// scene. Person identifiers request contact primitives and optional receiving
/// Mind rows; they do not request whole person or genome state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaintenanceInputs {
    pub agreements: BTreeSet<AgreementId>,
    pub obligations: BTreeSet<ObligationId>,
    pub amendments: BTreeSet<AmendmentId>,
    pub pending_work: BTreeSet<ObligationId>,
    pub knowledge: BTreeSet<PersonId>,
    pub people: BTreeSet<PersonId>,
    pub stocks: BTreeSet<InventoryId>,
    pub output_sources: BTreeMap<(SiteId, Good), Option<InventoryId>>,
}
impl OutputSources for MaintenanceInputs {
    fn output_source(&self, site: SiteId, good: Good) -> Option<InventoryId> {
        self.output_sources.get(&(site, good)).copied().flatten()
    }
}
/// Membership guards for reconciliation cover the ledger columns, and observer
/// refresh covers its knowledge column. Material and contact inputs stay typed
/// and sparse rather than requiring a copy of the rest of the world.
pub fn maintenance_inputs(
    store: &Store,
    receipts: &[Receipt],
    observations: &[ObservationEvent],
    outputs: &impl OutputSources,
) -> MaintenanceInputs {
    let mut inputs = MaintenanceInputs {
        agreements: agreements(store).map(|(id, _)| id).collect(),
        obligations: obligations(store).map(|(id, _)| id).collect(),
        amendments: amendments(store).map(|(id, _)| id).collect(),
        pending_work: pending_works(store).map(|(id, _)| id).collect(),
        knowledge: knowledges(store).map(|(id, _)| id).collect(),
        people: observation_people(store, observations),
        ..Default::default()
    };
    for (_, agreement) in agreements(store) {
        inputs.people.insert(agreement.proposer);
        inputs.people.extend(agreement.parties.iter().copied());
    }
    for (_, obligation) in obligations(store) {
        inputs.stocks.extend(obligation.cargo.keys().copied());
        match obligation.term {
            AgreementTerm::Delivery {
                source,
                destination,
                ..
            } => {
                inputs.stocks.extend([source, destination]);
            }
            AgreementTerm::OutputShare {
                site,
                good,
                destination,
                ..
            } => {
                inputs.stocks.insert(destination);
                let output = outputs.output_source(site, good);
                inputs.output_sources.insert((site, good), output);
                inputs.stocks.extend(output);
            }
            _ => {}
        }
    }
    for receipt in receipts {
        match receipt.kind {
            ReceiptKind::Transferred {
                actor,
                source,
                destination,
                ..
            } => {
                inputs.people.insert(actor);
                inputs.stocks.extend([source, destination]);
            }
            ReceiptKind::Lost { source, .. } => {
                inputs.stocks.insert(source);
            }
            ReceiptKind::Produced {
                actor, site, good, ..
            } => {
                inputs.people.insert(actor);
                let output = outputs.output_source(site, good);
                inputs.output_sources.insert((site, good), output);
                inputs.stocks.extend(output);
            }
            ReceiptKind::Harvested { actor, field, .. } => {
                inputs.people.insert(actor);
                for good in [Good::Food, Good::Seed] {
                    let output = outputs.output_source(field, good);
                    inputs.output_sources.insert((field, good), output);
                    inputs.stocks.extend(output);
                }
            }
            ReceiptKind::ProjectCompleted => {}
        }
    }
    inputs
}
