//! Personal opt-in valuations and conservative parallel offer matching.
use game_parallel_market_proposals as proposals;
mod custody;
pub mod validation;
use game_parallel_goods_cargo::{Exchange, GoodsCargo};
use game_parallel_market::{Decision, DecisionDraft, Influence, Market, Offer, Outcome, Policy};
use game_parallel_production_model::{Amount, Good, PersonSnapshot};
use rayon::prelude::*;
struct Accepted {
    market: DecisionDraft,
    cargo: GoodsCargo,
    actors: Vec<u64>,
}
pub struct Draft {
    accepted: Option<Accepted>,
    proposed: Option<proposals::Draft>,
    pub actors: Vec<u64>,
}
pub type DecisionOutcome = (Market, Option<GoodsCargo>, Vec<(u64, u64)>);
impl Draft {
    pub fn commit(
        self,
        market: &mut Market,
        cargo: &mut GoodsCargo,
        opinions: Option<&mut game_parallel_opinions::Opinions>,
    ) {
        if let Some(accepted) = self.accepted {
            if let Some(opinions) = opinions {
                opinions.learn_help(accepted.market.gifts());
            }
            accepted.market.commit(market);
            *cargo = accepted.cargo;
        }
        if let Some(proposed) = self.proposed {
            proposed.commit(market);
        }
    }
}

fn value(items: &[Amount], policy: &Policy) -> Result<Option<u128>, String> {
    let mut total = 0u128;
    for item in items {
        let Some(entry) = policy.values.iter().find(|entry| entry.good == item.good) else {
            return Ok(None);
        };
        total = total
            .checked_add(u128::from(item.quantity) * u128::from(entry.value))
            .ok_or("trade value overflow")?;
    }
    Ok(Some(total))
}
/// Prepare accepted transactions against one immutable tick-start snapshot.
fn prepare_acceptance(
    market: &Market,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    goods: &[Good],
    busy: &[u64],
    tick: u64,
    opinions: Option<&game_parallel_opinions::Opinions>,
) -> Result<Option<Accepted>, String> {
    if market.policies.is_empty() || market.offers.is_empty() {
        return Ok(None);
    }
    let mut policies: Vec<_> = market.policies.par_iter().filter(|p| p.enabled).collect();
    policies.par_sort_unstable_by_key(|policy| policy.person);
    let mut influences: Vec<_> = policies
        .par_iter()
        .flat_map_iter(|policy| {
            policy
                .influences
                .iter()
                .map(move |influence| ((policy.person, influence.person), influence))
        })
        .collect();
    influences.par_sort_unstable_by_key(|row| row.0);
    let track_help = opinions.is_some_and(|owner| owner.tracks_help());
    let opinions = opinions
        .filter(|owner| !owner.records.is_empty())
        .map(|owner| owner.index());
    let rows: Vec<Option<(Offer, Exchange)>> = market
        .offers
        .par_iter()
        .map(|offer| -> Result<_, String> {
            let Ok(index) = policies.binary_search_by_key(&offer.recipient, |policy| policy.person)
            else {
                return Ok(None);
            };
            let policy = policies[index];
            let (Some(received), Some(given)) =
                (value(&offer.give, policy)?, value(&offer.receive, policy)?)
            else {
                return Ok(None);
            };
            let required = given
                .checked_add(u128::from(policy.min_gain))
                .ok_or("trade value overflow")?;
            let influence = influences
                .binary_search_by_key(&(offer.recipient, offer.proposer), |row| row.0)
                .ok()
                .map(|index| influences[index].1);
            let social = u128::from(if offer.give.is_empty() {
                policy.help_value
            } else {
                0
            }) + influence.map_or(0, |row| {
                u128::from(row.attachment) + u128::from(row.obligation) + u128::from(row.fear)
            }) + u128::from(
                opinions
                    .as_ref()
                    .map_or(0, |index| index.obligation(offer.recipient, offer.proposer)),
            ) + u128::from(
                opinions
                    .as_ref()
                    .map_or(0, |index| index.fear(offer.recipient, offer.proposer)),
            );
            if received.saturating_add(social) < required {
                return Ok(None);
            }
            Ok(Some((
                offer.clone(),
                Exchange {
                    a: offer.proposer,
                    b: offer.recipient,
                    offer_a: offer.give.clone(),
                    offer_b: offer.receive.clone(),
                },
            )))
        })
        .collect::<Result<_, _>>()?;
    let candidates: Vec<_> = rows.into_par_iter().flatten().collect();
    let exchanges: Vec<_> = candidates
        .par_iter()
        .map(|(_, exchange)| exchange.clone())
        .collect();
    let feasible = cargo.assess_exchanges(people, goods, busy, &exchanges)?;
    // Each participant's first sorted claim has their lowest feasible offer ID.
    let mut claims: Vec<_> = candidates
        .par_iter()
        .zip(feasible.par_iter())
        .filter(|(_, change)| change.is_some())
        .flat_map_iter(|((offer, _), _)| [(offer.proposer, offer.id), (offer.recipient, offer.id)])
        .collect();
    if claims.par_windows(2).any(|pair| pair[0] > pair[1]) {
        claims.par_sort_unstable();
    }
    let conflicts = claims.par_windows(2).any(|pair| pair[0].0 == pair[1].0);
    let best: Vec<_> = if conflicts {
        claims
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| group[0])
            .collect()
    } else {
        Vec::new()
    };
    let lowest = |person, id| {
        best.binary_search_by_key(&person, |claim| claim.0)
            .ok()
            .is_some_and(|index| best[index].1 == id)
    };
    let mut selected: Vec<_> = candidates
        .into_par_iter()
        .zip(feasible.into_par_iter())
        .filter(|((offer, _), change)| {
            change.is_some()
                && (!conflicts
                    || (lowest(offer.proposer, offer.id) && lowest(offer.recipient, offer.id)))
        })
        .map(|((offer, _), change)| (offer, change.expect("feasible exchange")))
        .collect();
    if selected.is_empty() {
        return Ok(None);
    }
    selected.par_sort_unstable_by_key(|(offer, _)| offer.id);
    let decisions: Vec<_> = selected
        .par_iter()
        .map(|(offer, _)| Decision {
            id: offer.id,
            actor: offer.recipient,
            outcome: Outcome::Accepted,
        })
        .collect();
    let actors: Vec<_> = selected
        .par_iter()
        .flat_map_iter(|(offer, _)| [offer.proposer, offer.recipient])
        .collect();
    let mut market = market.stage_decisions(&decisions, tick, track_help)?;
    if !influences.is_empty() || opinions.is_some() {
        let considered: Vec<_> = selected
            .par_iter()
            .filter_map(|(offer, _)| {
                let prior = influences
                    .binary_search_by_key(&(offer.recipient, offer.proposer), |row| row.0)
                    .ok()
                    .map(|index| influences[index].1.clone());
                let fear = opinions
                    .as_ref()
                    .map_or(0, |index| index.fear(offer.recipient, offer.proposer));
                let obligation = opinions
                    .as_ref()
                    .map_or(0, |index| index.obligation(offer.recipient, offer.proposer));
                if prior.is_none() && fear == 0 && obligation == 0 {
                    return None;
                }
                let mut influence = prior.unwrap_or(Influence {
                    person: offer.proposer,
                    attachment: 0,
                    obligation: 0,
                    fear: 0,
                });
                influence.fear = influence.fear.saturating_add(fear);
                influence.obligation = influence.obligation.saturating_add(obligation);
                Some((offer.id, influence))
            })
            .collect();
        market.record_influences(&considered);
    }
    let changes = selected
        .into_par_iter()
        .map(|(_, changes)| changes)
        .collect();
    let cargo = custody::apply(cargo, &actors, changes);
    Ok(Some(Accepted {
        market,
        cargo,
        actors,
    }))
}
/// Expose individually selected offers without publishing aggregate cargo changes.
pub fn selected_offers(
    market: &Market,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    goods: &[Good],
    busy: &[u64],
    tick: u64,
    opinions: Option<&game_parallel_opinions::Opinions>,
) -> Result<Vec<game_parallel_market::Event>, String> {
    Ok(
        prepare_acceptance(market, cargo, people, goods, busy, tick, opinions)?
            .map_or_else(Vec::new, |accepted| accepted.market.events().to_vec()),
    )
}
/// Stage accepted offers and new proposals from independent available people.
pub fn prepare(
    market: &Market,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    goods: &[Good],
    busy: &[u64],
    tick: u64,
) -> Result<Option<Draft>, String> {
    prepare_with_opinions(market, cargo, people, goods, busy, tick, None)
}
pub fn prepare_with_opinions(
    market: &Market,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    goods: &[Good],
    busy: &[u64],
    tick: u64,
    opinions: Option<&game_parallel_opinions::Opinions>,
) -> Result<Option<Draft>, String> {
    let accepted = prepare_acceptance(market, cargo, people, goods, busy, tick, opinions)?;
    let mut occupied = busy.to_vec();
    if let Some(accepted) = &accepted {
        occupied.extend_from_slice(&accepted.actors);
    }
    let proposed = proposals::prepare(market, cargo, people, &occupied, tick)?;
    let mut actors = accepted
        .as_ref()
        .map_or_else(Vec::new, |draft| draft.actors.clone());
    if let Some(proposed) = &proposed {
        actors.extend_from_slice(&proposed.actors);
    }
    if accepted.is_none() && proposed.is_none() {
        Ok(None)
    } else {
        Ok(Some(Draft {
            accepted,
            proposed,
            actors,
        }))
    }
}
/// Validate and prepare simultaneous offers using the shared physical index.
pub fn propose(
    market: &Market,
    production: &game_parallel_production::Production,
    people: &[PersonSnapshot],
    proposals: &[game_parallel_market::Proposal],
    tick: u64,
) -> Result<(Market, Vec<u64>), String> {
    use game_parallel_production_model::person_index;
    let mut busy = production.busy_people();
    busy.par_sort_unstable();
    if proposals.par_iter().any(|proposal| {
        let a=person_index(people,proposal.proposer).map(|i|people[i]);
        let b=person_index(people,proposal.recipient).map(|i|people[i]);
        !matches!((a,b),(Some((_,Some(area_a),health_a,_)),Some((_,Some(area_b),health_b,_))) if area_a == area_b && health_a > 0 && health_b > 0)
            || busy.binary_search(&proposal.proposer).is_ok() || busy.binary_search(&proposal.recipient).is_ok()
    }) { return Err("traders must be living, local and available".into()); }
    let (next, ids) = market.propose_batch(proposals, tick)?;
    let goods = production.goods.iter().map(|good| good.id).collect();
    next.validate_index(people, &goods, tick)?;
    Ok((next, ids))
}
/// Validate passive goods owners against the production phase's shared person snapshot.
pub fn validate_boundary(
    production: Option<&game_parallel_production::Production>,
    draft: Option<&game_parallel_production::Draft>,
    cargo: Option<&GoodsCargo>,
    market: Option<&Market>,
    tick: u64,
) -> Result<(), &'static str> {
    if cargo.is_none() && market.is_none() {
        return Ok(());
    }
    let state = production.ok_or("goods owners require production")?;
    let people = draft.ok_or("goods owners require production")?.people();
    if let Some(cargo) = cargo {
        cargo
            .validate_index(people, &state.goods)
            .map_err(|_| "invalid goods cargo")?;
    }
    if let Some(market) = market {
        if cargo.is_none() {
            return Err("market requires goods cargo");
        }
        let goods = state.goods.iter().map(|good| good.id).collect();
        market
            .validate_index(people, &goods, tick)
            .map_err(|_| "invalid market state")?;
    }
    Ok(())
}
/// Read-only boundary context for explicitly requested market actions.
pub struct Context<'a> {
    pub market: Option<&'a Market>,
    pub production: Option<&'a game_parallel_production::Production>,
    pub cargo: Option<&'a GoodsCargo>,
    pub regions: &'a [game_parallel_people::Region],
    pub travelers: &'a [game_parallel_transport::Traveler],
    pub tick: u64,
    pub suspended: Option<u64>,
}
impl<'a> Context<'a> {
    fn inputs(
        &self,
    ) -> Result<
        (
            &'a Market,
            &'a game_parallel_production::Production,
            Vec<PersonSnapshot>,
        ),
        String,
    > {
        let market = self.market.ok_or("market is disabled")?;
        let production = self.production.ok_or("market requires production")?;
        let cargo = self.cargo.ok_or("market requires goods cargo")?;
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        let goods = production.goods.iter().map(|good| good.id).collect();
        cargo.validate_index(&people, &production.goods)?;
        market.validate_index(&people, &goods, self.tick)?;
        Ok((market, production, people))
    }
    pub fn propose(
        &self,
        proposals: &[game_parallel_market::Proposal],
    ) -> Result<(Market, Vec<u64>), String> {
        if self.suspended.is_some_and(|person| {
            proposals
                .par_iter()
                .any(|proposal| proposal.proposer == person)
        }) {
            return Err("player control is suspended".into());
        }
        let (market, production, people) = self.inputs()?;
        propose(market, production, &people, proposals, self.tick)
    }
    pub fn decide(&self, decisions: &[Decision]) -> Result<DecisionOutcome, String> {
        if self.suspended.is_some_and(|person| {
            decisions
                .par_iter()
                .any(|decision| decision.actor == person)
        }) {
            return Err("player control is suspended".into());
        }
        use game_parallel_production_model::person_index;
        let (market, production, people) = self.inputs()?;
        if decisions
            .par_iter()
            .any(|d| person_index(&people, d.actor).is_none_or(|i| people[i].2 == 0))
        {
            return Err("decision maker must be living".into());
        }
        let (next, offers) = market.decide_batch(decisions, self.tick)?;
        let gifts = Market::gift_pairs(&offers, decisions);
        let exchanges: Vec<_> = decisions
            .par_iter()
            .zip(offers.into_par_iter())
            .filter(|(decision, _)| decision.outcome == Outcome::Accepted)
            .map(|(_, offer)| Exchange {
                a: offer.proposer,
                b: offer.recipient,
                offer_a: offer.give,
                offer_b: offer.receive,
            })
            .collect();
        let cargo = if exchanges.is_empty() {
            None
        } else {
            Some(self.cargo.unwrap().exchange(
                &people,
                &production.goods,
                &production.busy_people(),
                &exchanges,
            )?)
        };
        Ok((next, cargo, gifts))
    }
}
