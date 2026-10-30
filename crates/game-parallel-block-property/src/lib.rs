//! Receiver-owned property commands against the physical person index.
pub mod fused;
pub mod inheritance;
pub mod work_incidents;
use game_parallel::World;
use game_parallel_production_model::PersonSnapshot;
pub struct Context<'a> {
    world: &'a mut World,
    people: &'a [PersonSnapshot],
}
impl Context<'_> {
    pub fn new<'a>(world: &'a mut World, people: &'a [PersonSnapshot]) -> Context<'a> {
        Context { world, people }
    }
    fn check(&self, actor: u64) -> Result<(), String> {
        if self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
            == Some(actor)
        {
            return Err("player control is suspended".into());
        }
        Ok(())
    }
    pub fn offer(
        &mut self,
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    ) -> Result<u64, String> {
        self.check(owner)?;
        self.world
            .property
            .as_mut()
            .ok_or("property is disabled")?
            .offer(self.people, owner, recipient, region, good, self.world.tick)
    }
    pub fn decide(&mut self, id: u64, actor: u64, accept: bool) -> Result<(), String> {
        self.check(actor)?;
        self.world
            .property
            .as_mut()
            .ok_or("property is disabled")?
            .decide(self.people, id, actor, accept)
    }
    pub fn grant(
        &mut self,
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    ) -> Result<(), String> {
        self.check(owner)?;
        self.world
            .property
            .as_mut()
            .ok_or("property is disabled")?
            .grant(self.people, owner, recipient, region, good)
    }
    pub fn revoke(
        &mut self,
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    ) -> Result<(), String> {
        self.check(owner)?;
        self.world
            .property
            .as_mut()
            .ok_or("property is disabled")?
            .revoke(self.people, owner, recipient, region, good)
    }
    pub fn withdraw(&mut self, id: u64, actor: u64) -> Result<(), String> {
        self.check(actor)?;
        self.world
            .property
            .as_mut()
            .ok_or("property is disabled")?
            .withdraw(self.people, id, actor)
    }
}

/// Stage an unauthorized pickup using the physical cargo owner; permission remains unchanged.
pub fn steal(
    world: &mut World,
    actor: u64,
    good: u32,
    quantity: u64,
) -> Result<game_parallel::PropertyTheft, String> {
    let region = world
        .regions
        .iter()
        .find(|region| {
            region
                .people
                .iter()
                .any(|person| person.id == actor && person.health > 0)
        })
        .ok_or("actor must be living and local")?;
    let property = world.property.as_ref().ok_or("property is disabled")?;
    if property.index().allows(actor, region.id, good) {
        return Err("stock is not restricted from this actor".into());
    }
    let owner = property
        .claims
        .iter()
        .find(|claim| claim.region == region.id && claim.good == good)
        .ok_or("stock has no owner")?
        .owner;
    let theft = game_parallel::PropertyTheft {
        tick: world.tick,
        actor,
        owner,
        region: region.id,
        good,
        quantity,
        witnessed: region
            .people
            .iter()
            .any(|person| person.id == owner && person.health > 0),
    };
    let suspended = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick));
    game_parallel_goods_actions::Context {
        production: &mut world.production,
        cargo: &mut world.goods_cargo,
        regions: &world.regions,
        travelers: &world.travelers,
        tick: world.tick,
        property: None,
        suspended,
    }
    .transfer(&[game_parallel::GoodsTransfer {
        person: actor,
        good,
        quantity,
        direction: game_parallel::GoodsDirection::Pickup,
    }])?;
    world.property.as_mut().unwrap().thefts.push(theft.clone());
    if theft.witnessed
        && let Some(opinions) = &mut world.opinions
    {
        opinions.witness_theft(owner, actor, world.tick);
    }
    Ok(theft)
}

/// Choose to use claimed inputs without permission; only a present claimant witnesses it.
pub fn work_without_permission(
    world: &mut World,
    job: game_parallel::StartWork,
) -> Result<(), String> {
    let production = world.production.as_ref().ok_or("production is disabled")?;
    let recipe = production
        .recipes
        .iter()
        .find(|row| row.id == job.recipe)
        .ok_or("unknown recipe")?;
    let mut incidents = Vec::new();
    if let Some(property) = &world.property {
        let index = property.index();
        for input in &recipe.inputs {
            if !index.allows(job.worker, job.region, input.good) {
                let claim = property
                    .claims
                    .iter()
                    .find(|claim| claim.region == job.region && claim.good == input.good)
                    .ok_or("stock has no owner")?;
                let quantity = input
                    .quantity
                    .checked_mul(u64::from(job.units))
                    .ok_or("work quantity overflow")?;
                if quantity == 0 {
                    continue;
                }
                let witnessed = world
                    .regions
                    .iter()
                    .find(|region| region.id == job.region)
                    .is_some_and(|region| {
                        region
                            .people
                            .iter()
                            .any(|person| person.id == claim.owner && person.health > 0)
                    });
                incidents.push(game_parallel::PropertyTheft {
                    tick: world.tick,
                    actor: job.worker,
                    owner: claim.owner,
                    region: job.region,
                    good: input.good,
                    quantity,
                    witnessed,
                });
            }
        }
    }
    let property = world.property.take();
    let result = world.start_work(&[job]);
    world.property = property;
    result?;
    if let Some(property) = &mut world.property {
        property.thefts.extend(incidents.iter().cloned());
    }
    if let Some(opinions) = &mut world.opinions {
        for incident in incidents.iter().filter(|incident| incident.witnessed) {
            opinions.witness_theft(incident.owner, incident.actor, world.tick);
        }
    }
    Ok(())
}
