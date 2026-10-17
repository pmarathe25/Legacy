use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    pub seed: u64,
    pub settings: game_worldgen::Settings,
    pub until: SimTime,
}
#[derive(Clone, Serialize, Deserialize)]
struct BeginPlay(PersonId);
impl Request for BeginPlay {
    const KEY: &'static str = "openings.begin-play";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<BeginPlay>(|tx, r| {
        require(
            tx.get::<Opening>()?.scenario.is_empty(),
            "opening already initialized",
        )?;
        tx.cancel_action(r.0)?;
        tx.call(game_lineage::Begin { person: r.0 })
    })
}
pub(super) fn initialize(
    host: &mut Host,
    store: &mut Store,
    seed: u64,
    player: bool,
) -> Result<()> {
    host.advance(game_worldgen::EPOCH, 1)?;
    let settings = game_worldgen::load(store)?;
    let traits = game_traits::load(store)?;
    let definition = game_worldgen::generate(&settings, seed, &traits)?;
    let assets = assets::Assets::load(store, &definition)?;
    host.dispatch(game_communities::Enable)?;
    build::initialize(host, &definition, assets, seed, false)?;
    host.dispatch(game_campaign::Start)?;
    let until = host.now().after(if player {
        settings.history
    } else {
        SimDuration(0)
    })?;
    loop {
        let p = host.advance(until, 256)?;
        match p.stop {
            Stop::Target => break,
            Stop::Budget => {}
            Stop::Choice(reason) => {
                return Err(Error::Rejected(format!("history interrupted: {reason}")));
            }
        }
    }
    let ruler = PersonId(host.get::<Opening>()?.names[&definition.ruler]);
    if player {
        host.dispatch(BeginPlay(ruler))?;
    }
    let mut opening = host.get::<Opening>()?.clone();
    opening.scenario = "generated".into();
    opening.version = 1;
    opening.sources = store.stamps();
    opening.definition = Some(definition);
    opening.generated = Some(History {
        seed,
        settings,
        until,
    });
    host.dispatch(Remember(opening))
}
