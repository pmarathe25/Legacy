use game_core::{InventoryId, PersonId};

/// Factual activity classes that may require a physical tool allocation.
/// The assembly maps its current activity schema into these owner facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentActivity {
    Work,
    Instruction,
    Practice,
    HealthWork,
    Gather { tool: Option<InventoryId> },
    Cultivate,
    Construct,
    WritingMemory,
    Other,
}

/// Return this actor's equipment demand, if the activity consumes a tool slot.
/// A tool-free gathering job remains available even without an allocation.
pub fn equipment_demand(actor: PersonId, activity: EquipmentActivity) -> Option<(PersonId, bool)> {
    let tool_free = match activity {
        EquipmentActivity::Work
        | EquipmentActivity::Instruction
        | EquipmentActivity::Practice
        | EquipmentActivity::HealthWork
        | EquipmentActivity::Cultivate
        | EquipmentActivity::Construct
        | EquipmentActivity::WritingMemory => false,
        EquipmentActivity::Gather { tool } => tool.is_none(),
        EquipmentActivity::Other => return None,
    };
    Some((actor, tool_free))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equipment_activities_produce_tool_demand() {
        let actor = PersonId(17);
        for activity in [
            EquipmentActivity::Work,
            EquipmentActivity::Instruction,
            EquipmentActivity::Practice,
            EquipmentActivity::HealthWork,
            EquipmentActivity::Gather {
                tool: Some(game_core::InventoryId(1)),
            },
            EquipmentActivity::Cultivate,
            EquipmentActivity::Construct,
            EquipmentActivity::WritingMemory,
        ] {
            assert_eq!(equipment_demand(actor, activity), Some((actor, false)));
        }
    }

    #[test]
    fn gathering_can_run_without_an_equipment_allocation() {
        assert_eq!(
            equipment_demand(PersonId(17), EquipmentActivity::Gather { tool: None }),
            Some((PersonId(17), true))
        );
    }

    #[test]
    fn other_activities_do_not_create_equipment_demand() {
        assert_eq!(
            equipment_demand(PersonId(17), EquipmentActivity::Other),
            None
        );
    }
}
