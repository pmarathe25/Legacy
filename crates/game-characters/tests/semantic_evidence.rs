use game_characters::decisions::*;
use game_core::*;
use game_logistics::{Delivery, Destination, Requirement};
use std::collections::{BTreeMap, BTreeSet};

fn mind() -> Mind<&'static str> {
    Mind {
        interests: BTreeMap::new(),
        routes: BTreeMap::new(),
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality: Personality::industrious(),
        dream: Aspiration::Independence,
        sites: BTreeMap::new(),
        relationships: BTreeMap::new(),
        values: BTreeMap::new(),
        goal: Goal::Rest,
        plan: None,
        explanation: Explanation {
            goal: Goal::Rest,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        },
        retry_after: SimTime(0),
    }
}
fn facts() -> PersonalFacts {
    PersonalFacts {
        actor: PersonId(1),
        now: SimTime(100),
        vitals: Vitals {
            hunger: 0,
            fatigue: 0,
            health: 9_000,
            alive: true,
        },
        limits: NeedLimits {
            meal_relief: 10_000,
            hunger_harm: 20_000,
            meal_size: Quantity(1_000),
            eating_time: SimDuration(1_000),
            fatigue_limit: 50_000,
            max_health: 10_000,
            drink_quantity: Quantity(1_000),
        },
        activity: Activity::Idle,
        place: Place::Area(AreaId(1)),
        control: Control::Autonomous,
        in_dialogue: false,
        independent: true,
        child: false,
        hunger_exempt: false,
        work_units: 0,
        own_cargo: BTreeSet::new(),
        observed_contacts: BTreeSet::new(),
        can_resume: false,
        hydration_urgency: 0,
        exposure_urgency: 0,
        freedom_restricted: false,
    }
}
fn choice(
    mind: &Mind<&'static str>,
    facts: &PersonalFacts,
    candidate: Candidate<&'static str>,
) -> Decision<&'static str> {
    mind.decide(facts, &Stocks::default(), &[candidate], ConceptId(2), 100)
}
#[test]
fn provider_identity_is_opaque_and_semantically_equal_effects_receive_equal_judgment() {
    let mind = mind();
    let facts = facts();
    let mut candidate = Candidate {
        action: "clinic provider",
        key: IntentionKey::Relief {
            need: NeedRef::Recovery,
            beneficiary: facts.actor,
        },
        motives: vec![Motive::Relieve {
            need: NeedRef::Recovery,
            beneficiary: facts.actor,
        }],
        prospects: Prospects {
            duration: SimDuration(1_000),
            effort: Effort::Restoration,
            ..Default::default()
        },
        shape: ActionShape::Other,
        delivery: None,
    };
    let clinic = choice(&mind, &facts, candidate.clone());
    candidate.action = "household provider";
    let household = choice(&mind, &facts, candidate);
    assert_eq!(clinic.explanation, household.explanation);
    assert_eq!(clinic.plan.unwrap().key, household.plan.unwrap().key);
}
#[test]
fn the_same_physical_opportunity_receives_contrasting_personal_judgment() {
    let mut honest = mind();
    let mut facts = facts();
    facts.vitals.hunger = 10_000;
    let opportunity = Candidate {
        action: "physically reachable unconsented stock",
        key: IntentionKey::Provision {
            good: Good::Food,
            beneficiary: facts.actor,
            source: None,
        },
        motives: vec![Motive::Provide {
            requirement: Requirement {
                good: Good::Food,
                quantity: Quantity(1_000),
                source: None,
            },
            beneficiary: facts.actor,
            need: Some(NeedRef::Hunger),
        }],
        prospects: Prospects {
            risk: Risk::UnconsentedTaking,
            uncertainty: Uncertainty::Conditional,
            ..Default::default()
        },
        shape: ActionShape::Other,
        delivery: None,
    };
    assert!(choice(&honest, &facts, opportunity.clone()).plan.is_none());
    honest.personality.honesty = 100;
    honest.personality.risk_tolerance = 900;
    assert!(choice(&honest, &facts, opportunity).plan.is_some());
    let development = Candidate {
        action: "same known construction opportunity",
        key: IntentionKey::Development {
            capability: Capability::Livelihood,
            beneficiary: facts.actor,
            subject: Some(Subject::Project(ProjectId(8))),
        },
        motives: vec![Motive::Develop {
            capability: Capability::Livelihood,
            beneficiary: facts.actor,
        }],
        prospects: Prospects::default(),
        shape: ActionShape::Other,
        delivery: None,
    };
    facts.vitals.hunger = 0;
    let ambitious = mind();
    assert!(
        choice(&ambitious, &facts, development.clone())
            .plan
            .is_some()
    );
    let mut restful = mind();
    restful.personality = Personality::restful();
    restful.dream = Aspiration::Comfort;
    assert!(choice(&restful, &facts, development).plan.is_none());
}
#[test]
fn acquisition_and_travel_keep_the_observed_intention_until_actual_consumer_receipt() {
    let mut mind = mind();
    let mut facts = facts();
    let child = PersonId(9);
    facts.observed_contacts.insert(child);
    mind.relationships.insert(
        child,
        Relationship {
            trust: 700,
            affection: 700,
        },
    );
    mind.child_signals.insert(
        child,
        ChildSignal {
            location: Location::AtArea(AreaId(1)),
            observed_at: facts.now,
            urgency: 3_000,
            hydration_urgency: 0,
            preference: None,
        },
    );
    let delivery = Delivery::new(
        Requirement {
            good: Good::Food,
            quantity: Quantity(1_000),
            source: None,
        },
        Destination::Contact(child),
    )
    .unwrap();
    let mut candidate = Candidate {
        action: "acquire a real source",
        key: IntentionKey::Provision {
            good: Good::Food,
            beneficiary: child,
            source: None,
        },
        motives: vec![Motive::Provide {
            requirement: delivery.requirement.clone(),
            beneficiary: child,
            need: Some(NeedRef::Hunger),
        }],
        prospects: Prospects {
            uncertainty: Uncertainty::Conditional,
            ..Default::default()
        },
        shape: ActionShape::Other,
        delivery: Some(delivery),
    };
    let selected = choice(&mind, &facts, candidate.clone());
    let key = selected.plan.as_ref().unwrap().key;
    mind.record_attempt(
        facts.now,
        selected.plan.unwrap(),
        selected.explanation,
        true,
    )
    .unwrap();
    facts.now = SimTime(200);
    facts.place = Place::Area(AreaId(2));
    facts.observed_contacts.clear();
    candidate.action = "travel with the acquired goods";
    candidate.shape = ActionShape::Move(AreaId(1));
    let selected = choice(&mind, &facts, candidate);
    assert_eq!(selected.plan.as_ref().unwrap().key, key);
    mind.record_attempt(
        facts.now,
        selected.plan.unwrap(),
        selected.explanation,
        true,
    )
    .unwrap();
    assert_eq!(mind.child_signals[&child].observed_at, SimTime(100));
    assert_eq!(
        mind.plan
            .as_ref()
            .unwrap()
            .delivery
            .as_ref()
            .unwrap()
            .remaining,
        Quantity(1_000)
    );
    mind.record_care_receipt(child, Good::Food, Quantity(1_000), 1)
        .unwrap();
    assert!(mind.care_deliveries(&facts).is_empty());
}
