//! Independent delivery choices and accepted physical promise transfers.
use game_economy_expeditions::{MissionPhase as Phase, Objective};
use game_lots::Place;
use rayon::prelude::*;

pub(crate) fn choices(
    context: &crate::giving::Context<'_>,
    chosen: &[(u64, u64, u32, u64)],
    opinions: Option<&game_opinions::Index<'_>>,
) -> Vec<(u64, u64, u32, u64)> {
    let Some(missions) = context.missions else {
        return Vec::new();
    };
    if !missions.entries.par_iter().any(|mission| {
        !mission.objective_met
            && !matches!(
                mission.phase,
                Phase::Ready | Phase::Completed | Phase::Failed
            )
            && mission.delivery_preference.is_some_and(|preference| {
                preference.willing || preference.pressure_threshold.is_some()
            })
    }) {
        return Vec::new();
    }
    let mut occupied: Vec<_> = chosen.par_iter().map(|row| row.0).collect();
    if occupied.par_windows(2).any(|pair| pair[0] > pair[1]) {
        occupied.par_sort_unstable();
    }
    missions
        .entries
        .par_iter()
        .filter_map(|mission| {
            let preference = mission.delivery_preference?;
            let Some(Objective::Deliver {
                person: recipient,
                good,
                quantity,
                delivered,
            }) = mission.objective
            else {
                return None;
            };
            if mission.objective_met
                || matches!(
                    mission.phase,
                    Phase::Ready | Phase::Completed | Phase::Failed
                )
                || occupied.binary_search(&mission.person).is_ok()
                || context.busy.binary_search(&mission.person).is_ok()
            {
                return None;
            }
            let sender = context
                .people
                .binary_search_by_key(&mission.person, |p| p.id)
                .ok()
                .map(|i| &context.people[i])?;
            let target = context
                .people
                .binary_search_by_key(&recipient, |p| p.id)
                .ok()
                .map(|i| &context.people[i])?;
            if !context.locally_together(sender, target) {
                return None;
            }
            let pressured = preference.pressure_threshold.is_some_and(|threshold| {
                opinions.is_some_and(|index| {
                    let (fear, obligation) = index.feelings(sender.id, target.id);
                    fear.saturating_add(obligation) >= threshold
                })
            });
            if sender.controlled
                || sender.health == 0
                || target.health == 0
                || sender.region != Some(mission.destination)
                || !context.communication.allows(sender.id, target.id)
                || context
                    .locations
                    .region((sender.id, sender.region), (target.id, target.region))
                    != Some(mission.destination)
                || !(preference.willing || pressured)
            {
                return None;
            }
            let from = context.view.container(Place::Person(sender.id))?;
            let to = context.view.container(Place::Person(target.id))?;
            let amount = quantity
                .saturating_sub(delivered)
                .min(from.quantity(good).saturating_sub(preference.reserve))
                .min(context.view.remaining_quantity(to, good));
            (amount > 0).then_some((sender.id, target.id, good, amount))
        })
        .collect()
}
