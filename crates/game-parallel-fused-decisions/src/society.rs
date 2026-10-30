//! Stage social and decision owners from committed person views.
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE};
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
/// Whether configured decision owners need the shared compatibility index.
pub fn needs_people(world: &World) -> bool {
    world
        .agreements
        .as_ref()
        .is_some_and(|agreements| agreements.policies.iter().any(|policy| policy.automatic()))
        || world.caregiving.is_some()
        || world.social.is_some()
        || world.autowork.is_some()
        || world.market.is_some()
        || world.lineage.is_some()
        || world
            .care
            .as_ref()
            .is_some_and(|care| !care.jobs.is_empty())
}
pub struct Draft {
    pub actors: Vec<u64>,
    pub social: Option<game_parallel_social_edges::Selected>,
    pub knowledge: Option<game_parallel_knowledge_columns::Draft>,
    pub autonomy: Option<game_parallel_fused_autonomy::Draft>,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    food: &[u32],
    people: &[PersonSnapshot],
    next: u64,
    coordination: (&[u64], &[game_parallel_social::Encounter]),
    knowledge: super::knowledge::Context<'_>,
) -> Result<Draft, &'static str> {
    let (planned, forced) = coordination;
    if world.social.is_none() && world.knowledge.is_none() && world.autonomy.is_none() {
        return Ok(Draft {
            actors: Vec::new(),
            social: None,
            knowledge: None,
            autonomy: None,
        });
    }
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &work)| (work != NONE).then_some(id))
        })
        .collect();
    busy.extend_from_slice(planned);
    busy.par_sort_unstable();
    // Autonomy reads committed knowledge even after this tick's encounters are selected.
    let preparation = knowledge
        .runtime
        .map(|runtime| runtime.begin(world.tick))
        .transpose()?;
    let memories = preparation.as_ref().map(|preparation| preparation.read());
    let view = super::knowledge::View::new(blocks, &world.regions, food, knowledge.positions);
    let personalities = world
        .personality
        .as_ref()
        .filter(|_| world.social.is_some())
        .map(|owner| owner.index());
    let contacts = || -> Result<_, &'static str> {
        let social = world
            .social
            .as_ref()
            .map(|social| {
                knowledge
                    .social
                    .expect("enabled social runtime")
                    .prepare_indexed_with_extra(
                        social,
                        people,
                        &busy,
                        (world.seed, next),
                        forced,
                        |person, isolation| {
                            personalities.as_ref().is_none_or(|traits| {
                                traits.social(person, isolation, world.seed, next)
                            })
                        },
                    )
            })
            .transpose()?;
        let knowledge = preparation.map(|preparation| {
            preparation.prepare_trusted(
                &view,
                social.as_ref().map_or(&[], |s| s.events.as_slice()),
                world.opinions.as_ref(),
            )
        });
        Ok((social, knowledge))
    };
    let (social, knowledge_draft) = contacts()?;
    let mut actors = social
        .as_ref()
        .map_or_else(Vec::new, |social| social.actors());
    busy.extend_from_slice(&actors);
    busy.par_sort_unstable();
    let actor_view =
        game_parallel_fused_autonomy::View::new(blocks, &world.regions, knowledge.positions, &busy);
    let autonomy = knowledge
        .autonomy
        .map(|runtime| {
            let memories = memories.ok_or("autonomy requires knowledge")?;
            runtime.prepare(&actor_view, |person| memories.records(person), world.tick)
        })
        .transpose()?;
    game_parallel_fused_autonomy::reserve(autonomy.as_ref(), &mut actors);
    Ok(Draft {
        actors,
        social,
        knowledge: knowledge_draft,
        autonomy,
    })
}
