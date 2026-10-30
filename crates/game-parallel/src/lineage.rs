use crate::World;
impl World {
    pub(crate) fn suspended_person(&self) -> Option<u64> {
        self.lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.tick))
    }
}
pub(super) fn validate(world: &World) -> Result<(), String> {
    crate::Lineage::validate_optional(world.lineage.as_ref(), world.families.as_ref(), world.tick)
}
pub(super) fn advance(world: &mut World, tick: u64) {
    if let (Some(lineage), Some(families)) = (&mut world.lineage, &world.families) {
        lineage.advance(families, tick);
    }
}
