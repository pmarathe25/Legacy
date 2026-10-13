//! Project receipt provenance checked against plain collaborator facts.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FarmReceiptFacts {
    pub id: SiteId,
    pub area: AreaId,
    pub seed: InventoryId,
    pub tool: InventoryId,
    pub water: InventoryId,
    pub food: InventoryId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectCompletionSiteFacts {
    pub workshop_exists: bool,
    pub farm: Option<FarmReceiptFacts>,
}

/// Facts describe the receipt's site, with absent collaborators represented as absent facts.
pub fn validate_project_completed_receipt(
    store: &Store,
    project_id: ProjectId,
    enterprise_id: EnterpriseId,
    site: SiteId,
    facts: ProjectCompletionSiteFacts,
) -> Result<(), Error> {
    let Some(plan) = project(store, project_id) else {
        return Err(invalid_receipt());
    };
    let valid = enterprise(store, enterprise_id).is_some_and(|record| {
        record.project == project_id
            && record.site == site
            && match plan.kind {
                ProjectKind::Workshop { .. } => facts.workshop_exists,
                ProjectKind::Farm { plot, water } => {
                    site == plot
                        && facts.farm.is_some_and(|field| {
                            field.id == plot
                                && field.area == plan.area
                                && field.seed == plan.input
                                && field.tool == plan.tool
                                && field.water == water
                                && field.food == plan.output
                        })
                }
            }
    });
    if valid {
        Ok(())
    } else {
        Err(invalid_receipt())
    }
}

fn invalid_receipt() -> Error {
    Error::InvalidSave("inconsistent committed state".into())
}

pub fn validate_enterprise_input_receipt(
    store: &Store,
    enterprise_id: EnterpriseId,
    site: SiteId,
) -> Result<(), Error> {
    if enterprise(store, enterprise_id).is_some_and(|record| record.site == site) {
        Ok(())
    } else {
        Err(invalid_receipt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farm_receipt_requires_every_field_to_match_its_project() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let kind = ProjectKind::Farm {
            plot: SiteId(10),
            water: InventoryId(8),
        };
        insert_project(
            &mut store,
            ProjectId(11),
            Project {
                kind: kind.clone(),
                id: ProjectId(11),
                sponsor: PersonId(3),
                area: AreaId(2),
                materials: InventoryId(4),
                tool: InventoryId(5),
                input: InventoryId(6),
                output: InventoryId(7),
                beneficiaries: BTreeSet::from([PersonId(3)]),
                workers: BTreeSet::from([PersonId(3)]),
                support: BTreeSet::new(),
                status: ProjectStatus::Completed,
                blocker: None,
                progress_ms: 0,
                contributions: BTreeMap::new(),
                materials_consumed: true,
                finish_by: SimTime(500),
                enterprise: Some(EnterpriseId(12)),
            },
        )
        .unwrap();
        insert_enterprise(
            &mut store,
            EnterpriseId(12),
            Enterprise {
                kind,
                id: EnterpriseId(12),
                project: ProjectId(11),
                sponsor: PersonId(3),
                area: AreaId(2),
                site: SiteId(10),
                beneficiaries: BTreeSet::from([PersonId(3)]),
                workers: BTreeSet::from([PersonId(3)]),
                status: EnterpriseStatus::Ready,
                blocker: None,
                labor: BTreeMap::new(),
                operating_agreements: BTreeSet::new(),
                inputs: BTreeMap::new(),
                produced: BTreeMap::new(),
            },
        )
        .unwrap();
        let field = FarmReceiptFacts {
            id: SiteId(10),
            area: AreaId(2),
            seed: InventoryId(6),
            tool: InventoryId(5),
            water: InventoryId(8),
            food: InventoryId(7),
        };
        let valid = |farm| {
            validate_project_completed_receipt(
                &store,
                ProjectId(11),
                EnterpriseId(12),
                SiteId(10),
                ProjectCompletionSiteFacts {
                    workshop_exists: true,
                    farm,
                },
            )
        };
        assert!(valid(Some(field)).is_ok());
        assert!(valid(None).is_err());
        for mismatch in [
            FarmReceiptFacts {
                id: SiteId(99),
                ..field
            },
            FarmReceiptFacts {
                area: AreaId(99),
                ..field
            },
            FarmReceiptFacts {
                seed: InventoryId(99),
                ..field
            },
            FarmReceiptFacts {
                tool: InventoryId(99),
                ..field
            },
            FarmReceiptFacts {
                water: InventoryId(99),
                ..field
            },
            FarmReceiptFacts {
                food: InventoryId(99),
                ..field
            },
        ] {
            assert!(valid(Some(mismatch)).is_err());
        }
        assert!(validate_enterprise_input_receipt(&store, EnterpriseId(12), SiteId(10)).is_ok());
        assert!(validate_enterprise_input_receipt(&store, EnterpriseId(12), SiteId(99)).is_err());
    }
}
