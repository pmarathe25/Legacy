mod support;
use game_core::*;
use game_literacy::*;
use game_reasoning::Claim;
use game_sim::*;
fn setup() -> (Host, Vec<PersonId>, InventoryId) {
    let mut b = support::builder();
    game_items::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    let (mut h, p, inventory) = support::people(b);
    h.dispatch(game_communication::Enable).unwrap();
    for key in ["paper", "ink"] {
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(key)
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 10,
        })
        .unwrap();
    }
    let until = h
        .dispatch(Develop {
            person: p[0],
            inventory,
        })
        .unwrap();
    support::run(&mut h, until);
    assert!(!h.get::<Literacy>().unwrap().scripts.is_empty());
    (h, p, inventory)
}
fn claim(effect: i64) -> Claim {
    Claim {
        subject: "iron".into(),
        factor: "heat".into(),
        outcome: "hardness".into(),
        complexity: 2,
        context: [("water".into(), 1)].into(),
        effect,
        confidence: 70,
        comparisons: 3,
    }
}
fn write(h: &mut Host, p: PersonId, inventory: InventoryId, effect: i64) -> u64 {
    let claim = claim(effect);
    let key = claim.key();
    h.dispatch(game_knowledge::Conclude {
        person: p,
        key: key.clone(),
        value: serde_json::to_value(claim).unwrap(),
        confidence: 70,
    })
    .unwrap();
    let script = *h.get::<Literacy>().unwrap().known[&p]
        .keys()
        .next()
        .unwrap();
    let until = h
        .dispatch(Inscribe {
            person: p,
            inventory,
            key,
            script,
        })
        .unwrap();
    support::run(h, until);
    *h.get::<Literacy>()
        .unwrap()
        .documents
        .keys()
        .last()
        .unwrap()
}
#[test]
fn learned_assertions_have_physical_distinguishable_symbols_and_require_the_script() {
    let (mut h, p, inventory) = setup();
    let first = write(&mut h, p[0], inventory, 100);
    let second = write(&mut h, p[0], inventory, -100);
    let a = &h.get::<Literacy>().unwrap().documents[&first];
    let b = &h.get::<Literacy>().unwrap().documents[&second];
    assert_eq!(a.key, b.key);
    assert_ne!(a.glyphs, b.glyphs);
    let key = a.key.clone();
    let script = a.script;
    h.dispatch(Show {
        person: p[0],
        listener: p[2],
        document: first,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::TIMBER,
        quantity: 1,
    })
    .unwrap();
    for _ in 0..4 {
        h.dispatch(game_communication::Convey {
            speaker: p[0],
            listener: p[2],
            concept: "good:2".into(),
            demonstrate: true,
            explanation: vec![],
        })
        .unwrap();
    }
    for _ in 0..4 {
        h.dispatch(game_communication::Convey {
            speaker: p[0],
            listener: p[2],
            concept: format!("person:{}", p[0].0),
            demonstrate: true,
            explanation: vec![],
        })
        .unwrap();
    }
    for _ in 0..8 {
        h.dispatch(game_communication::Convey {
            speaker: p[0],
            listener: p[2],
            concept: key.clone(),
            demonstrate: false,
            explanation: vec!["good:2".into(), format!("person:{}", p[0].0)],
        })
        .unwrap();
    }
    assert!(
        !h.dispatch(Read {
            person: p[2],
            document: first
        })
        .unwrap()
    );
    for _ in 0..2 {
        assert!(
            h.dispatch(Teach {
                teacher: p[0],
                student: p[2],
                script
            })
            .unwrap()
        );
    }
    assert!(
        h.dispatch(Read {
            person: p[2],
            document: first
        })
        .unwrap()
    );
    let learned = game_knowledge::known::<Claim>(
        &h.get::<game_knowledge::Knowledge>().unwrap().0[&p[2]],
        &key,
    )
    .unwrap();
    assert_eq!(learned.effect, 100);
    assert_eq!(learned.context["water"], 1);
    let saved = h.save().unwrap();
    let mut b = support::builder();
    game_items::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    assert_eq!(saved, b.load(&saved, 4).unwrap().save().unwrap());
}
#[test]
fn inability_to_interpret_marks_cannot_be_overridden_by_script_proficiency() {
    let (mut h, p, inventory) = setup();
    let document = write(&mut h, p[0], inventory, 100);
    h.dispatch(game_traits::Set {
        person: p[0],
        profile: [("interpretation".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Read {
            person: p[0],
            document
        })
        .is_err()
    );
}

#[test]
fn a_known_page_secretly_moved_to_another_pack_is_not_remotely_readable() {
    let (mut h, p, inventory) = setup();
    let document = write(&mut h, p[0], inventory, 100);
    h.dispatch(Show {
        person: p[0],
        listener: p[2],
        document,
    })
    .unwrap();
    let other = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p[1]),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_items::Move {
        person: p[0],
        item: document,
        destination: other,
    })
    .unwrap();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Read {
            person: p[2],
            document
        })
        .is_err()
    );
    assert_eq!(saved, h.save().unwrap());
    assert!(
        h.dispatch(Read {
            person: p[2],
            document: u64::MAX
        })
        .is_err()
    );
    assert!(
        h.dispatch(Show {
            person: p[0],
            listener: p[2],
            document: u64::MAX
        })
        .is_err()
    );
}

#[test]
fn instruction_needs_an_interpretable_channel_and_presentations_can_be_forwarded() {
    let (mut h, p, inventory) = setup();
    let document = write(&mut h, p[0], inventory, 100);
    let script = h.get::<Literacy>().unwrap().documents[&document].script;
    h.dispatch(game_traits::Set {
        person: p[2],
        profile: [("interpretation".into(), 0)].into(),
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Teach {
            teacher: p[0],
            student: p[2],
            script
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    h.dispatch(game_traits::Set {
        person: p[2],
        profile: [("interpretation".into(), 1000), ("hearing".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Teach {
            teacher: p[0],
            student: p[2],
            script
        })
        .unwrap()
    );
    h.dispatch(Show {
        person: p[0],
        listener: p[2],
        document,
    })
    .unwrap();
    h.dispatch(Show {
        person: p[2],
        listener: p[1],
        document,
    })
    .unwrap();
    assert_eq!(h.get::<Literacy>().unwrap().shown[&document], p[0]);
    assert!(
        !h.dispatch(Read {
            person: p[2],
            document
        })
        .unwrap()
    ); // Script practice is still incomplete.
}

#[test]
fn unchanged_saved_notes_are_not_rewritten_and_keep_their_original_symbols() {
    let (mut h, p, inventory) = setup();
    let document = write(&mut h, p[0], inventory, 100);
    let d = &h.get::<Literacy>().unwrap().documents[&document];
    let alphabet = &h.get::<Literacy>().unwrap().scripts[&d.script].glyphs;
    let decoded: Vec<_> = d
        .glyphs
        .iter()
        .map(|glyph| alphabet.iter().position(|g| g == glyph).unwrap() as u8)
        .collect();
    let mut opaque_fact = d.fact.clone();
    opaque_fact.value = std::sync::Arc::new(serde_json::to_value(&d.fact.value).unwrap().into());
    assert_eq!(
        decoded,
        serde_json::to_vec(&(p[0], &d.key, opaque_fact)).unwrap()
    );
    assert!(
        h.dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .iter()
            .all(|p| !p.key.starts_with("literacy:write:"))
    );
    let saved = h.save().unwrap();
    let mut b = support::builder();
    game_items::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    let mut loaded = b.load(&saved, 4).unwrap();
    assert!(
        loaded
            .dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .iter()
            .all(|p| !p.key.starts_with("literacy:write:"))
    );
    loaded
        .dispatch(game_knowledge::Conclude {
            person: p[0],
            key: claim(-100).key(),
            value: serde_json::to_value(claim(-100)).unwrap(),
            confidence: 70,
        })
        .unwrap();
    assert!(
        loaded
            .dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .iter()
            .any(|p| p.key.starts_with("literacy:write:"))
    );
}

#[test]
fn local_regional_paper_and_ink_can_support_paid_script_development() {
    let (mut h, people, inventory) = setup();
    for key in ["paper", "ink"] {
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(key)
            .unwrap();
        let quantity = h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good);
        h.dispatch(game_cargo::Consume {
            inventory,
            good,
            quantity,
        })
        .unwrap();
    }
    for key in ["moss-paper", "berry-ink"] {
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(key)
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 2,
        })
        .unwrap();
    }
    let before = h.get::<Literacy>().unwrap().scripts.len();
    let until = h
        .dispatch(Develop {
            person: people[0],
            inventory,
        })
        .unwrap();
    support::run(&mut h, until);
    assert!(h.get::<Literacy>().unwrap().scripts.len() > before);
    let saved = h.save().unwrap();
    let mut b = support::builder();
    game_items::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    assert_eq!(b.load(&saved, 4).unwrap().save().unwrap(), saved);
}

#[test]
fn writing_can_be_carved_painted_or_impressed_on_real_nonpaper_surfaces() {
    for (surface, tool, method) in [
        (
            "stone",
            Some("stone-tools"),
            game_inscription::Method::Incision,
        ),
        ("cloth", Some("charcoal"), game_inscription::Method::Pigment),
        ("clay", None, game_inscription::Method::Impression),
    ] {
        let (mut h, people, old) = setup();
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(people[0]),
                capacity: 1000,
            })
            .unwrap();
        // Remove the initial ink so that each alternative method is actually exercised.
        let ink = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve("ink")
            .unwrap();
        let quantity = h.get::<game_cargo::Cargo>().unwrap().0[&old].amount(ink);
        h.dispatch(game_cargo::Consume {
            inventory: old,
            good: ink,
            quantity,
        })
        .unwrap();
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(surface)
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 2,
        })
        .unwrap();
        if let Some(tool) = tool {
            let good = h
                .get::<game_objects::Objects>()
                .unwrap()
                .resolve(tool)
                .unwrap();
            h.dispatch(game_cargo::Deposit {
                inventory,
                good,
                quantity: 1,
            })
            .unwrap();
        }
        let item = write(&mut h, people[0], inventory, 100);
        let document = &h.get::<Literacy>().unwrap().documents[&item];
        assert_eq!(document.imprint.method, method);
        assert_eq!(
            h.get::<game_items::Items>().unwrap().objects[&item].good,
            good
        );
        assert_eq!(
            h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good),
            2
        );
        let saved = h.save().unwrap();
        let mut b = support::builder();
        game_items::plugin(&mut b).unwrap();
        plugin(&mut b).unwrap();
        assert_eq!(b.load(&saved, 4).unwrap().save().unwrap(), saved);
    }
}
#[test]
fn an_incompatible_marker_cannot_carve_stone_and_failure_rolls_back() {
    let (mut h, people, old) = setup();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(people[0]),
            capacity: 1000,
        })
        .unwrap();
    let ink = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("ink")
        .unwrap();
    let quantity = h.get::<game_cargo::Cargo>().unwrap().0[&old].amount(ink);
    h.dispatch(game_cargo::Consume {
        inventory: old,
        good: ink,
        quantity,
    })
    .unwrap();
    let stone = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("stone")
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: stone,
        quantity: 1,
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Develop {
            person: people[0],
            inventory
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}
