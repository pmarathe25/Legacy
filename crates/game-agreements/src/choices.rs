//! People review remembered proposals and choose whether to honor their duties.
use crate::*;
/// Project financing and crop estimates remain their providers' calculations.
/// The agreement owner scores these supplied personal, dated facts.
pub trait JudgmentFacts: Presence {
    fn actor_facts(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<OfferActorFacts, Error>;
    fn knowledge(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<impl OfferKnowledge, Error>;
}
pub fn score_offer(
    actor: PersonId,
    terms: &[AgreementTerm],
    facts: &impl JudgmentFacts,
) -> Result<i64, Error> {
    offer_score(
        facts.actor_facts(actor, terms)?,
        terms,
        &facts.knowledge(actor, terms)?,
    )
}
/// Proposal responses are prepared before any answer is published. Amendment
/// reviews are a later pass, so they see resulting accepted or withdrawn offers.
pub fn proposal_responses(
    store: &Store,
    controlled: PersonId,
    facts: &impl JudgmentFacts,
) -> Result<Vec<(PersonId, AgreementAction)>, Error> {
    let mut answers = Vec::new();
    for (actor, known) in knowledges(store) {
        if actor == controlled {
            continue;
        }
        for offer in known.agreements.values() {
            if !needs_proposal_response(actor, offer)
                || contact_area(facts, actor) != contact_area(facts, offer.proposer)
            {
                continue;
            }
            if let Some(answer) =
                proposal_answer(actor, offer, score_offer(actor, &offer.terms, facts)?, true)
            {
                answers.push((actor, answer));
            }
        }
    }
    Ok(answers)
}
pub fn amendment_votes(
    store: &Store,
    controlled: PersonId,
    facts: &impl JudgmentFacts,
) -> Result<Vec<(PersonId, AgreementAction)>, Error> {
    let mut votes = Vec::new();
    for (actor, known) in knowledges(store) {
        if actor == controlled {
            continue;
        }
        for offer in known.agreements.values() {
            for change in pending_amendments(actor, offer) {
                if contact_area(facts, actor) != contact_area(facts, change.proposer) {
                    continue;
                }
                let revised = amended_terms(offer, change)?;
                let support = amendment_support(
                    score_offer(actor, &offer.terms, facts)?,
                    score_offer(actor, &revised, facts)?,
                    facts.actor_facts(actor, &offer.terms)?.empathy,
                )?;
                votes.push((
                    actor,
                    AgreementAction::Vote {
                        amendment: change.id,
                        support,
                    },
                ));
            }
        }
    }
    Ok(votes)
}
pub fn share_prospect(planned_food: Option<u128>, established: bool) -> ShareProspect {
    match planned_food {
        Some(expected_share) => ShareProspect::PlannedFood { expected_share },
        None if established => ShareProspect::Established,
        None => ShareProspect::Unknown,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DutyActorFacts {
    pub controlled: bool,
    pub restricted: bool,
    pub in_combat: bool,
    pub idle_or_rest: bool,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub discipline: u16,
    pub honesty: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DutyAction<A> {
    Construct { project: ProjectId },
    Work { site: SiteId },
    Move { destination: AreaId },
    Cultivate(A),
    Delivery(game_logistics::Step<A>),
    Travel(game_transport::RouteIntent),
}
/// Natural planning ports: read remembered sites and routes, invoke project work
/// admission, read cultivation candidates, or supply Logistics' physical facts.
/// The provider never receives an agreement ledger or chooses its obligation.
pub trait DutyFacts<A: Clone>: Presence + OutputSources {
    fn actor_facts(&self, actor: PersonId) -> Option<DutyActorFacts>;
    fn known_site(&self, actor: PersonId, site: SiteId) -> Option<KnownSiteRight>;
    fn routes(&self, actor: PersonId) -> Option<&game_logistics::KnownRoutes>;
    fn cultivated_site(&self, site: SiteId) -> bool;
    fn cultivation(&self, actor: PersonId) -> Vec<(SiteId, A)>;
    fn construction_admitted(&self, actor: PersonId, project: ProjectId) -> bool;
    fn source_good(&self, actor: PersonId, source: InventoryId) -> Option<Good>;
    fn inventory_destination(
        &self,
        actor: PersonId,
        id: InventoryId,
    ) -> Option<game_logistics::KnownDestination>;
    fn delivery_facts<'a>(
        &'a self,
        actor: PersonId,
        delivery: &game_logistics::Delivery,
        destination: &game_logistics::KnownDestination,
        cargo: &BTreeMap<InventoryId, Quantity>,
    ) -> game_logistics::DeliveryFacts<'a, A>;
    fn acquisition(&self, actor: PersonId) -> impl game_logistics::AcquisitionSource<A>;
}
pub fn chosen_tasks<A: Clone>(
    store: &Store,
    facts: &impl DutyFacts<A>,
) -> BTreeMap<PersonId, DutyAction<A>> {
    let mut intentions = BTreeMap::new();
    for (actor, known) in knowledges(store) {
        for view in known.agreements.values().filter(|view| {
            matches!(
                view.status,
                AgreementStatus::Active | AgreementStatus::Breached
            )
        }) {
            for o in view
                .obligations
                .iter()
                .filter(|o| o.term.parties()[0] == actor && o.performed < o.amount())
            {
                if intentions.contains_key(&actor) {
                    continue;
                }
                let Some(person) = facts.actor_facts(actor) else {
                    continue;
                };
                if person.controlled || person.restricted || person.in_combat {
                    continue;
                }
                let Some(area) = contact_area(facts, actor) else {
                    continue;
                };
                if !person.idle_or_rest
                    || person.hunger >= person.meal_relief
                    || person.fatigue >= person.fatigue_limit / 2
                    || person.discipline < 500 && person.honesty < 500
                {
                    continue;
                }
                let task = match o.term {
                    AgreementTerm::OutputShare {
                        debtor,
                        creditor,
                        site,
                        good,
                        destination,
                        pay_by,
                        ..
                    } => {
                        let Some(source) = facts.output_source(site, good) else {
                            continue;
                        };
                        AgreementTerm::Delivery {
                            debtor,
                            creditor,
                            source,
                            destination,
                            quantity: Quantity(o.accrued),
                            deadline: pay_by,
                        }
                    }
                    ref term => term.clone(),
                };
                let action = match task {
                    AgreementTerm::ConstructionLabor { project, .. } => facts
                        .construction_admitted(actor, project)
                        .then_some(DutyAction::Construct { project }),
                    AgreementTerm::Labor { site, .. } => {
                        if facts.cultivated_site(site) {
                            facts
                                .cultivation(actor)
                                .into_iter()
                                .find(|(id, _)| *id == site)
                                .map(|(_, action)| DutyAction::Cultivate(action))
                        } else {
                            facts
                                .known_site(actor, site)
                                .filter(|site| site.accessible)
                                .and_then(|site| {
                                    if site.area == area {
                                        Some(DutyAction::Work { site: site.site })
                                    } else {
                                        facts
                                            .routes(actor)
                                            .filter(|routes| {
                                                game_logistics::reachable(
                                                    routes,
                                                    &Location::AtArea(area),
                                                    &Location::AtArea(site.area),
                                                )
                                            })
                                            .map(|_| DutyAction::Move {
                                                destination: site.area,
                                            })
                                    }
                                })
                        }
                    }
                    AgreementTerm::Delivery {
                        source,
                        destination,
                        quantity,
                        ..
                    } => {
                        let good = match o.term {
                            AgreementTerm::OutputShare { good, .. } => Some(good),
                            _ => facts.source_good(actor, source),
                        };
                        good.and_then(|good| {
                            let mut destination =
                                facts.inventory_destination(actor, destination)?;
                            destination.accessible = true;
                            let delivery = game_logistics::Delivery {
                                requirement: game_logistics::Requirement {
                                    good,
                                    quantity,
                                    source: Some(source),
                                },
                                destination: destination.destination,
                                remaining: Quantity(quantity.0.saturating_sub(o.performed)),
                                last_receipt: 0,
                            };
                            let tags = o.cargo.iter().map(|(id, q)| (*id, Quantity(*q))).collect();
                            let projected =
                                facts.delivery_facts(actor, &delivery, &destination, &tags);
                            let place = projected.place.clone();
                            let now = projected.now;
                            let routes = projected.routes;
                            let step = delivery.plan_with_acquisition(
                                projected,
                                Quantity(1),
                                &facts.acquisition(actor),
                            );
                            match step {
                                game_logistics::Step::Travel(target) => {
                                    game_transport::route_intent(&place, &target, now, routes)
                                        .ok()
                                        .map(DutyAction::Travel)
                                }
                                game_logistics::Step::Acquire(_)
                                | game_logistics::Step::Carry { .. }
                                | game_logistics::Step::Transfer { .. } => {
                                    Some(DutyAction::Delivery(step))
                                }
                                _ => None,
                            }
                        })
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    intentions.insert(actor, action);
                }
            }
        }
    }
    intentions
}
