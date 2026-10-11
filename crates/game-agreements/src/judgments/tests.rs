use super::*;
#[derive(Default)]
struct Knowledge {
    stocks: BTreeMap<InventoryId, StockKnowledge>,
    sites: BTreeSet<SiteId>,
    projects: BTreeSet<ProjectId>,
    shares: BTreeMap<SiteId, ShareProspect>,
    speakers: BTreeMap<PersonId, SpeakerFacts>,
}
impl OfferKnowledge for Knowledge {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn work_site_access(&self, id: SiteId) -> bool {
        self.sites.contains(&id)
    }
    fn project_known(&self, id: ProjectId) -> bool {
        self.projects.contains(&id)
    }
    fn share_prospect(&self, id: SiteId, _: u64, _: u64) -> ShareProspect {
        self.shares
            .get(&id)
            .copied()
            .unwrap_or(ShareProspect::Unknown)
    }
    fn speaker(&self, id: PersonId) -> Option<SpeakerFacts> {
        self.speakers.get(&id).copied()
    }
}
fn actor() -> OfferActorFacts {
    OfferActorFacts {
        actor: PersonId(1),
        now: SimTime(0),
        hunger: 180_000,
        empathy: 500,
        risk_tolerance: 500,
        ambition: 900,
        hedonism: 700,
        financing_utility: 0,
    }
}
fn stock(id: u64, area: u64, quantity: u64, accessible: bool) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(area),
        good: Good::Food,
        quantity: Quantity(quantity),
        accessible,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(1)),
    }
}
fn delivery(debtor: u64, creditor: u64, deadline: u64) -> AgreementTerm {
    AgreementTerm::Delivery {
        debtor: PersonId(debtor),
        creditor: PersonId(creditor),
        source: InventoryId(10),
        destination: InventoryId(11),
        quantity: Quantity(2_000),
        deadline: SimTime(deadline),
    }
}
fn view(terms: Vec<AgreementTerm>) -> AgreementView {
    AgreementView {
        id: AgreementId(20),
        proposer: PersonId(2),
        parties: BTreeSet::from([PersonId(1), PersonId(2)]),
        accepted: BTreeSet::from([PersonId(2)]),
        status: AgreementStatus::Proposed,
        terms,
        obligations: Vec::new(),
        procedure: None,
        amendments: Vec::new(),
        revision: 0,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: Some(KnowledgeSource::Observed(PersonId(2))),
    }
}
#[test]
fn scalar_term_judgments_match_the_existing_policy() {
    let mut known = Knowledge::default();
    known
        .stocks
        .insert(InventoryId(10), stock(10, 1, 2_000, true));
    known.sites.insert(SiteId(7));
    assert_eq!(
        offer_score(actor(), &[delivery(1, 2, 120_000)], &known),
        Ok(-1_000)
    );
    assert_eq!(
        offer_score(actor(), &[delivery(1, 2, 119_999)], &known),
        Ok(-11_000)
    );
    assert_eq!(
        offer_score(actor(), &[delivery(2, 1, 120_000)], &known),
        Ok(2_000)
    );
    let labor = AgreementTerm::Labor {
        debtor: PersonId(1),
        creditor: PersonId(2),
        site: SiteId(7),
        millis: 60_000,
        deadline: SimTime(60_000),
    };
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&labor), &known),
        Ok(-1_000)
    );
    known.sites.clear();
    assert_eq!(offer_score(actor(), &[labor], &known), Ok(-11_000));
    let access = AgreementTerm::Access {
        grantor: PersonId(2),
        grantee: PersonId(1),
        inventory: InventoryId(10),
        allowed: true,
    };
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&access), &known),
        Ok(3_000)
    );
    let mut neutral = actor();
    neutral.hunger = 120_000;
    assert_eq!(
        offer_score(neutral, std::slice::from_ref(&access), &known),
        Ok(1_000)
    );
    known.stocks.clear();
    assert_eq!(offer_score(actor(), &[access], &known), Ok(-2_000));
    let project = AgreementTerm::ConstructionLabor {
        debtor: PersonId(1),
        creditor: PersonId(2),
        project: ProjectId(3),
        millis: 60_000,
        deadline: SimTime(60_000),
    };
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&project), &known),
        Ok(-11_000)
    );
    known.projects.insert(ProjectId(3));
    assert_eq!(offer_score(actor(), &[project], &known), Ok(-1_000));
}
#[test]
fn share_value_uses_projected_quantity_without_production_or_world_access() {
    let mut known = Knowledge::default();
    let term = AgreementTerm::OutputShare {
        debtor: PersonId(2),
        creditor: PersonId(1),
        site: SiteId(7),
        good: Good::Food,
        numerator: 1,
        denominator: 4,
        destination: InventoryId(10),
        pay_by: SimTime(100),
        ends_at: SimTime(200),
    };
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&term), &known),
        Ok(-2_000)
    );
    known.shares.insert(SiteId(7), ShareProspect::Established);
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&term), &known),
        Ok(400)
    );
    known.shares.insert(
        SiteId(7),
        ShareProspect::PlannedFood {
            expected_share: 2_000_000,
        },
    );
    assert_eq!(offer_score(actor(), &[term], &known), Ok(10_000));
}
#[test]
fn private_or_remote_facts_cannot_change_an_offer_judgment() {
    let mut known = Knowledge::default();
    let term = delivery(2, 1, 120_000);
    let original = offer_score(actor(), std::slice::from_ref(&term), &known).unwrap();
    known
        .stocks
        .insert(InventoryId(99), stock(99, 9, u64::MAX, true));
    known.speakers.insert(
        PersonId(2),
        SpeakerFacts {
            locally_present: false,
            trust: -2_000,
            observed_invocation_penalty: 400,
        },
    );
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&term), &known),
        Ok(original)
    );
    known
        .speakers
        .get_mut(&PersonId(2))
        .unwrap()
        .locally_present = true;
    assert_eq!(
        offer_score(actor(), std::slice::from_ref(&term), &known),
        Ok(original - 800)
    );
    known.speakers.get_mut(&PersonId(2)).unwrap().trust = 2_000;
    assert_eq!(offer_score(actor(), &[term], &known), Ok(original));
}
#[test]
fn acceptance_counteroffer_and_rejection_thresholds_preserve_consent() {
    let offer = view(vec![AgreementTerm::Labor {
        debtor: PersonId(1),
        creditor: PersonId(2),
        site: SiteId(7),
        millis: 60_000,
        deadline: SimTime(180_000),
    }]);
    assert_eq!(
        proposal_answer(PersonId(1), &offer, 0, true),
        Some(AgreementAction::Accept {
            agreement: offer.id
        })
    );
    for score in [-1, -2_000] {
        let Some(AgreementAction::Counteroffer {
            terms, procedure, ..
        }) = proposal_answer(PersonId(1), &offer, score, true)
        else {
            panic!("expected counteroffer")
        };
        assert_eq!(terms[0].amount(), 30_000);
        assert_eq!(procedure, offer.procedure);
    }
    assert_eq!(
        proposal_answer(PersonId(1), &offer, -2_001, true),
        Some(AgreementAction::Reject {
            agreement: offer.id
        })
    );
    assert_eq!(proposal_answer(PersonId(1), &offer, 10_000, false), None);
    let unchanged = view(vec![delivery(1, 2, 180_000)]);
    assert_eq!(
        proposal_answer(PersonId(1), &unchanged, -1_000, true),
        Some(AgreementAction::Reject {
            agreement: unchanged.id
        })
    );
    assert_eq!(amendment_support(1_000, 500, 800), Ok(true));
    assert_eq!(amendment_support(1_000, 499, 800), Ok(false));
}
fn exchange<'a>(
    stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    sites: &'a [KnownSiteRight],
    custodians: &'a BTreeMap<InventoryId, PersonId>,
) -> LocalExchangeFacts<'a> {
    LocalExchangeFacts {
        actor: PersonId(1),
        autonomous: true,
        area: Some(AreaId(1)),
        hunger: 180_000,
        stocks,
        sites,
        custodians,
    }
}
#[test]
fn local_exchange_keeps_stable_choice_and_does_not_publish_its_cooldown() {
    let state = State::default();
    let before = state.clone();
    let mut stocks = BTreeMap::from([
        (InventoryId(10), stock(10, 1, 1_000, false)),
        (InventoryId(20), stock(20, 1, 5_000, false)),
    ]);
    let sites = [
        KnownSiteRight {
            site: SiteId(8),
            area: AreaId(1),
            accessible: true,
        },
        KnownSiteRight {
            site: SiteId(7),
            area: AreaId(1),
            accessible: true,
        },
    ];
    let custodians = BTreeMap::from([
        (InventoryId(10), PersonId(2)),
        (InventoryId(20), PersonId(3)),
    ]);
    let proposal = state
        .local_exchange(SimTime(0), exchange(&stocks, &sites, &custodians), |_| {
            Some(AreaId(1))
        })
        .unwrap();
    assert_eq!(proposal.next_search, SimTime(300_000));
    assert_eq!(state, before);
    let AgreementAction::Propose { terms, .. } = &proposal.action else {
        panic!("expected proposal")
    };
    assert!(matches!(
        terms[0],
        AgreementTerm::Access {
            inventory: InventoryId(10),
            ..
        }
    ));
    assert!(matches!(
        terms[1],
        AgreementTerm::Labor {
            site: SiteId(7),
            ..
        }
    ));
    stocks.insert(InventoryId(5), stock(5, 9, 10_000, true));
    assert_eq!(
        state.local_exchange(
            SimTime(0),
            exchange(&stocks, &sites, &custodians),
            |_| Some(AreaId(1))
        ),
        Some(proposal)
    );
    assert_eq!(
        state.local_exchange(
            SimTime(0),
            exchange(&stocks, &sites, &custodians),
            |_| Some(AreaId(2))
        ),
        None
    );
    stocks.insert(InventoryId(5), stock(5, 1, 1_000, true));
    assert_eq!(
        state.local_exchange(
            SimTime(0),
            exchange(&stocks, &sites, &custodians),
            |_| Some(AreaId(1))
        ),
        None
    );
}
#[test]
fn deadlines_cooldowns_and_numeric_overflow_are_explicit() {
    let mut known = Knowledge::default();
    known
        .stocks
        .insert(InventoryId(10), stock(10, 1, 2_000, true));
    let mut overflow = actor();
    overflow.financing_utility = i64::MAX;
    assert_eq!(
        offer_score(overflow, &[delivery(2, 1, 120_000)], &known),
        Err(Error::Overflow)
    );
    assert_eq!(
        amendment_support(i64::MAX, i64::MAX, 1_000),
        Err(Error::Overflow)
    );
    let stocks = BTreeMap::from([(InventoryId(10), stock(10, 1, 1_000, false))]);
    let sites = [KnownSiteRight {
        site: SiteId(7),
        area: AreaId(1),
        accessible: true,
    }];
    let custodians = BTreeMap::from([(InventoryId(10), PersonId(2))]);
    let mut state = State::default();
    state.next_search.insert(PersonId(1), SimTime(1));
    assert_eq!(
        state.local_exchange(
            SimTime(0),
            exchange(&stocks, &sites, &custodians),
            |_| Some(AreaId(1))
        ),
        None
    );
    assert!(
        state
            .local_exchange(
                SimTime(1),
                exchange(&stocks, &sites, &custodians),
                |_| Some(AreaId(1))
            )
            .is_some()
    );
    state.next_search.clear();
    let before = state.clone();
    assert_eq!(
        state.local_exchange(
            SimTime(u64::MAX - 100),
            exchange(&stocks, &sites, &custodians),
            |_| Some(AreaId(1))
        ),
        None
    );
    assert_eq!(state, before);
}

#[test]
fn amendments_only_expose_unanswered_current_changes_to_chosen_approvers() {
    let mut offer = view(vec![delivery(1, 2, 180_000)]);
    offer.procedure = Some(AmendmentProcedure {
        eligible_proposers: BTreeSet::from([PersonId(2)]),
        approvers: BTreeSet::from([PersonId(1)]),
        threshold: 1,
        term_approvers: BTreeMap::new(),
    });
    offer.amendments.push(AmendmentView {
        id: AmendmentId(30),
        proposer: PersonId(2),
        replacements: BTreeMap::from([(0, delivery(1, 2, 240_000))]),
        votes: BTreeMap::new(),
        status: AmendmentStatus::Proposed,
        base_revision: 0,
    });
    assert_eq!(pending_amendments(PersonId(1), &offer).count(), 1);
    assert_eq!(pending_amendments(PersonId(2), &offer).count(), 0);
    let revised = amended_terms(&offer, &offer.amendments[0]).unwrap();
    assert_eq!(revised[0].deadline(), Some(SimTime(240_000)));
    offer.amendments[0].votes.insert(PersonId(1), true);
    assert_eq!(pending_amendments(PersonId(1), &offer).count(), 0);
    offer.amendments[0].votes.clear();
    offer.revision = 1;
    assert_eq!(pending_amendments(PersonId(1), &offer).count(), 0);
    offer.amendments[0].replacements = BTreeMap::from([(2, delivery(1, 2, 240_000))]);
    assert!(amended_terms(&offer, &offer.amendments[0]).is_err());
}
