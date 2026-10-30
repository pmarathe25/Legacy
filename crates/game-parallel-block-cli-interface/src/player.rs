//! Restrict interactive player commands to the current incarnation.
use game_parallel_fused_physical::Kernel;

pub fn check(engine: &Kernel, line: &str) -> Result<(), String> {
    let words: Vec<_> = line.split_whitespace().collect();
    let Some(&command) = words.first() else {
        return Ok(());
    };
    if command == "mission-party" {
        let world = engine.export();
        let current = world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.current)
            .ok_or("no current ruler")?;
        let ids: Vec<u64> = words[1..]
            .iter()
            .map(|word| word.parse().map_err(|_| "invalid mission"))
            .collect::<Result<_, _>>()?;
        let owns_one = world.missions.as_ref().is_some_and(|missions| {
            missions
                .entries
                .iter()
                .any(|mission| mission.person == current && ids.contains(&mission.id))
        });
        if !owns_one {
            return Err("player must participate in the party".into());
        }
        return Ok(());
    }
    let actor_position = match command {
        "help" | "quit" | "save" | "load" | "observe" | "spectate-guardian" | "run" | "invoke"
        | "dismiss-invocation" | "accept-invocation" | "memories" | "request-release" => {
            return Ok(());
        }
        "partnership-preference"
        | "reproduction"
        | "leave-partnership"
        | "move"
        | "take"
        | "store"
        | "eat"
        | "eat-good"
        | "social-pressure"
        | "food-seeking"
        | "food-source"
        | "food-stores"
        | "store-building"
        | "deposit-claim"
        | "store-build"
        | "store-objection"
        | "store-claim"
        | "confrontation"
        | "store-take"
        | "store-put"
        | "give-food"
        | "food-aid"
        | "food-deposit"
        | "food-asking"
        | "request-food"
        | "food-requests"
        | "withdraw-food"
        | "pickup-good"
        | "deposit-good"
        | "steal-good"
        | "recover"
        | "recover-good"
        | "offer"
        | "offers"
        | "agreement-propose"
        | "agreements"
        | "agreement-policy"
        | "opinions"
        | "skills"
        | "goal-set"
        | "goals"
        | "dream-set"
        | "dreams"
        | "mission-start"
        | "mission-cancel"
        | "missions"
        | "talk"
        | "social"
        | "mission-consent"
        | "mission-meet"
        | "mission-candidates"
        | "mission-invite"
        | "care-policy"
        | "caregiving"
        | "farm-policy"
        | "farm-status"
        | "teach"
        | "learn"
        | "request-work"
        | "work-requests"
        | "property-offer"
        | "property-access" => 1,
        "work" | "work-unpermitted" | "accept" | "decline" | "withdraw" | "combat-order"
        | "tactical-move" | "agreement-decide" | "agreement-deliver" | "property-decide"
        | "property-withdraw" | "respond-work" => 2,
        _ => return Err("command unavailable in player mode".into()),
    };
    let actor: u64 = words
        .get(actor_position)
        .ok_or("missing actor")?
        .parse()
        .map_err(|_| "invalid actor")?;
    let current = engine
        .export()
        .lineage
        .and_then(|lineage| lineage.current)
        .ok_or("no current ruler")?;
    if actor != current {
        return Err("player can only command the current ruler".into());
    }
    Ok(())
}
