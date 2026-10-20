use game_core::*;
use game_sim::*;
use game_works::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_pastimes::plugin(&mut b).unwrap();
    game_items::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    game_inquiry::plugin(&mut b).unwrap();
    game_observations::plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, PersonId, PersonId, InventoryId) {
    let mut h = builder().build(9, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Grove".into(),
        })
        .unwrap();
    let mut people = vec![];
    for name in ["Author", "Reader"] {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_social_life::Register {
            person: p,
            name: name.into(),
        })
        .unwrap();
        people.push(p);
    }
    let creator = people[0];
    h.dispatch(game_traits::Set {
        person: creator,
        profile: [("creativity".into(), 900), ("intelligence".into(), 4000)].into(),
    })
    .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(creator),
            capacity: 1000,
        })
        .unwrap();
    for (key, quantity) in [("paper", 20), ("ink", 10)] {
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(key)
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity,
        })
        .unwrap();
    }
    h.dispatch(game_knowledge::ObserveSurroundings { person: creator })
        .unwrap();
    let activities =
        game_pastimes::load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap();
    h.dispatch(activities).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    h.dispatch(Enable).unwrap();
    (h, creator, people[1], inventory)
}
fn create(activity: &str) -> (Host, PersonId, PersonId, InventoryId, u64) {
    let (mut h, author, reader, inventory) = setup();
    let until = h
        .dispatch(game_pastimes::Pursue {
            person: author,
            activity: activity.into(),
            inventory,
        })
        .unwrap();
    assert!(h.get::<Works>().unwrap().pieces.is_empty());
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(until, 128).unwrap().stop == Stop::Budget {}
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let id = *h.get::<Works>().unwrap().pieces.keys().next().unwrap();
    (h, author, reader, inventory, id)
}
fn learn_script(h: &mut Host, teacher: PersonId, student: PersonId, work: u64) {
    let script = h.get::<Works>().unwrap().pieces[&work]
        .composition
        .as_ref()
        .unwrap()
        .script
        .unwrap();
    for _ in 0..2 {
        assert!(
            h.dispatch(game_literacy::Teach {
                teacher,
                student,
                script
            })
            .unwrap()
        );
    }
}
#[test]
fn poems_books_and_music_use_the_creators_language_and_ordinary_materials_and_time() {
    for (activity, genre, material) in [
        ("compose-poem", Genre::Poem, 1),
        ("write-book", Genre::Book, 8),
        ("compose-song", Genre::Song, 0),
    ] {
        let (h, author, _, inventory, id) = create(activity);
        let work = &h.get::<Works>().unwrap().pieces[&id];
        let text = work.composition.as_ref().unwrap();
        assert_eq!(text.genre, genre);
        assert_eq!(text.gloss.len(), text.sounds.len());
        for (sounds, meaning) in text.sounds.iter().zip(&text.gloss) {
            assert_eq!(
                game_communication::decode(h.get().unwrap(), author, sounds)
                    .meaning
                    .as_ref(),
                Some(meaning)
            );
        }
        let paper = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve("paper")
            .unwrap();
        assert_eq!(
            h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(paper),
            20
        );
        assert_eq!(text.item.is_some(), material > 0);
        if genre == Genre::Song {
            assert_eq!(text.notes.len(), text.sounds.len());
            assert_eq!(text.line_ends.len(), 2);
        }
    }
}
#[test]
fn a_text_does_not_teach_unknown_meanings_and_follows_its_physical_item() {
    let (mut h, author, reader, inventory, id) = create("write-book");
    let before = h.save().unwrap();
    assert!(h.dispatch(Read { reader, work: id }).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert!(view(&h, reader).unwrap().is_empty());
    h.dispatch(Present {
        holder: author,
        listener: reader,
        work: id,
    })
    .unwrap();
    let unknown = h.dispatch(Read { reader, work: id }).unwrap();
    assert!(unknown.iter().all(|w| w.meaning.is_none()));
    assert_eq!(view(&h, author).unwrap().len(), 1);
    assert_eq!(view(&h, reader).unwrap().len(), 1);
    let concept = format!("person:{}", author.0);
    h.dispatch(game_communication::Convey {
        speaker: author,
        listener: reader,
        concept: concept.clone(),
        demonstrate: true,
        explanation: vec![],
    })
    .unwrap();
    learn_script(&mut h, author, reader, id);
    let partly = h.dispatch(Read { reader, work: id }).unwrap();
    assert!(partly.iter().any(|w| w.meaning.as_ref() == Some(&concept)));
    assert!(partly.iter().any(|w| w.meaning.is_none()));
    let known = h
        .dispatch(Read {
            reader: author,
            work: id,
        })
        .unwrap();
    assert!(known.iter().all(|w| w.meaning.is_some()));
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("paper")
        .unwrap();
    let item = h.get::<Works>().unwrap().pieces[&id]
        .composition
        .as_ref()
        .unwrap()
        .item
        .unwrap();
    assert_eq!(
        h.get::<game_items::Items>().unwrap().objects[&item].good,
        good
    );
    h.dispatch(game_cargo::Consume {
        inventory,
        good,
        quantity: h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good),
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(Read { reader, work: id }).is_err());
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn known_words_require_a_working_channel_and_writing_remains_readable_to_a_deaf_reader() {
    for (speaker_can_speak, listener_can_hear) in [(true, false), (false, true)] {
        let (mut h, author, reader, inventory) = setup();
        h.dispatch(game_communication::Convey {
            speaker: author,
            listener: reader,
            concept: format!("person:{}", author.0),
            demonstrate: true,
            explanation: vec![],
        })
        .unwrap();
        if !speaker_can_speak {
            h.dispatch(game_traits::Set {
                person: author,
                profile: [("vocalization".into(), 0), ("creativity".into(), 900)].into(),
            })
            .unwrap();
        }
        if !listener_can_hear {
            h.dispatch(game_traits::Set {
                person: reader,
                profile: [("hearing".into(), 0)].into(),
            })
            .unwrap();
        }
        let pursuit = h.dispatch(game_pastimes::Pursue {
            person: author,
            inventory,
            activity: "compose-song".into(),
        });
        if !speaker_can_speak {
            assert!(pursuit.is_err());
            continue;
        }
        let until = pursuit.unwrap();
        while h.advance(until, 128).unwrap().stop == Stop::Budget {}
        assert!(
            !h.get::<Works>()
                .unwrap()
                .pieces
                .values()
                .next()
                .unwrap()
                .audience
                .contains_key(&reader)
        );
    }
    let (mut h, author, reader, _, id) = create("write-book");
    h.dispatch(Present {
        holder: author,
        listener: reader,
        work: id,
    })
    .unwrap();
    h.dispatch(game_communication::Convey {
        speaker: author,
        listener: reader,
        concept: format!("person:{}", author.0),
        demonstrate: true,
        explanation: vec![],
    })
    .unwrap();
    learn_script(&mut h, author, reader, id);
    h.dispatch(game_traits::Set {
        person: reader,
        profile: [("hearing".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Read { reader, work: id })
            .unwrap()
            .iter()
            .any(|w| w.meaning.is_some())
    );
    h.dispatch(game_traits::Set {
        person: reader,
        profile: [("interpretation".into(), 0)].into(),
    })
    .unwrap();
    assert!(
        h.dispatch(Read { reader, work: id })
            .unwrap()
            .iter()
            .all(|w| w.meaning.is_none())
    );
    assert!(
        view(&h, reader).unwrap()[0]
            .translation
            .iter()
            .all(|w| w.meaning.is_none())
    );
}

fn teach(h: &mut Host, speaker: PersonId, listener: PersonId, concept: &str) {
    h.dispatch(game_communication::Convey {
        speaker,
        listener,
        concept: concept.into(),
        demonstrate: true,
        explanation: vec![],
    })
    .unwrap();
}
fn talk(h: &mut Host, first: PersonId, second: PersonId) {
    let end = h
        .dispatch(game_social_life::Talk {
            person: first,
            other: second,
        })
        .unwrap();
    while h.advance(end, 128).unwrap().stop == Stop::Budget {}
}
fn newcomer(h: &mut Host, area: AreaId) -> PersonId {
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    h.dispatch(game_social_life::Register {
        person: p,
        name: "Later listener".into(),
    })
    .unwrap();
    p
}
#[test]
fn a_song_can_outlive_its_creator_through_personal_retelling_and_replay() {
    let (mut h, author, reader, _, id) = create("compose-song");
    assert!(
        h.get::<Works>().unwrap().pieces[&id].audience.is_empty(),
        "being nearby does not grant an experience"
    );
    let concept = format!("person:{}", author.0);
    teach(&mut h, author, reader, &concept);
    talk(&mut h, author, reader);
    assert!(
        h.get::<Works>().unwrap().pieces[&id]
            .audience
            .contains_key(&reader)
    );
    let area = game_transport::area(h.get().unwrap(), reader).unwrap();
    let third = newcomer(&mut h, area);
    teach(&mut h, author, third, &concept);
    assert!(
        h.dispatch(Read {
            reader: third,
            work: id
        })
        .is_err()
    );
    h.dispatch(game_physiology::Damage {
        person: author,
        amount: 100,
    })
    .unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        talk(host, reader, third);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert!(
        h.get::<Works>().unwrap().pieces[&id]
            .audience
            .contains_key(&third)
    );
    let text = h
        .dispatch(Read {
            reader: third,
            work: id,
        })
        .unwrap();
    assert!(text.iter().any(|w| w.meaning.as_ref() == Some(&concept)));
    assert!(
        text.iter().any(|w| w.meaning.is_none()),
        "retelling cannot disclose private intended meanings"
    );
    assert!(h.notices().any(|e| e.kind == "works.shared"
        && e.data["speaker"] == reader.0
        && e.data["listener"] == third.0));
}
#[test]
fn a_listener_can_decline_culture_or_be_interested_because_of_personal_regard() {
    let (mut h, author, reader, _, id) = create("compose-song");
    teach(&mut h, author, reader, &format!("person:{}", author.0));
    h.dispatch(game_traits::Set {
        person: reader,
        profile: [("contemplation".into(), 0)].into(),
    })
    .unwrap();
    talk(&mut h, author, reader);
    assert!(
        !h.get::<Works>().unwrap().pieces[&id]
            .audience
            .contains_key(&reader)
    );
    h.dispatch(game_relationships::SetRelationship {
        observer: reader,
        other: author,
        relationship: game_relationships::Relationship {
            respect: 100,
            admiration: 100,
            ..Default::default()
        },
    })
    .unwrap();
    talk(&mut h, author, reader);
    assert!(
        h.get::<Works>().unwrap().pieces[&id]
            .audience
            .contains_key(&reader)
    );
}
#[test]
fn retelling_cannot_bypass_absent_speech_or_hearing_even_with_a_familiar_word() {
    for (person_key, key) in [
        (true, "vocalization"),
        (false, "hearing"),
        (false, "interpretation"),
    ] {
        let (mut h, author, reader, _, id) = create("compose-song");
        teach(&mut h, author, reader, &format!("person:{}", author.0));
        h.dispatch(game_traits::Set {
            person: if person_key { author } else { reader },
            profile: [(key.into(), 0), ("creativity".into(), 900)].into(),
        })
        .unwrap();
        talk(&mut h, author, reader);
        assert!(
            !h.get::<Works>().unwrap().pieces[&id]
                .audience
                .contains_key(&reader)
        );
    }
}
#[test]
fn conversations_can_show_a_real_book_without_teaching_its_private_gloss() {
    let (mut h, author, reader, inventory, id) = create("write-book");
    assert!(view(&h, reader).unwrap().is_empty());
    talk(&mut h, author, reader);
    assert!(
        h.get::<Works>().unwrap().pieces[&id]
            .presented
            .contains_key(&reader)
    );
    assert!(
        h.dispatch(Read { reader, work: id })
            .unwrap()
            .iter()
            .any(|w| w.meaning.is_none())
    );
    let area = game_transport::area(h.get().unwrap(), author).unwrap();
    let third = newcomer(&mut h, area);
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("paper")
        .unwrap();
    h.dispatch(game_cargo::Consume {
        inventory,
        good,
        quantity: h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(good),
    })
    .unwrap();
    talk(&mut h, author, third);
    assert!(
        !h.get::<Works>().unwrap().pieces[&id]
            .presented
            .contains_key(&third)
    );
    assert!(
        h.dispatch(Read {
            reader: third,
            work: id
        })
        .is_err()
    );
}

#[test]
fn paid_writing_creates_each_copy_without_an_annual_or_creativity_gate() {
    let (mut h, author, _, inventory) = setup();
    h.dispatch(game_traits::Set {
        person: author,
        profile: [("creativity".into(), 0)].into(),
    })
    .unwrap();
    for _ in 0..2 {
        let end = h
            .dispatch(game_pastimes::Pursue {
                person: author,
                inventory,
                activity: "write-book".into(),
            })
            .unwrap();
        while h.advance(end, 128).unwrap().stop == Stop::Budget {}
    }
    let works = h.get::<Works>().unwrap();
    assert_eq!(works.pieces.len(), 2);
    let items: std::collections::BTreeSet<_> = works
        .pieces
        .values()
        .map(|w| w.composition.as_ref().unwrap().item.unwrap())
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(
            h.get::<game_objects::Objects>()
                .unwrap()
                .resolve("paper")
                .unwrap()
        ),
        20
    );
    builder().load(&h.save().unwrap(), 4).unwrap();
}
#[test]
fn repeating_a_song_reuses_its_record_and_another_composition_can_follow_immediately() {
    let (mut h, author, _, inventory, _) = create("compose-song");
    for activity in ["compose-song", "compose-poem"] {
        let end = h
            .dispatch(game_pastimes::Pursue {
                person: author,
                inventory,
                activity: activity.into(),
            })
            .unwrap();
        while h.advance(end, 128).unwrap().stop == Stop::Budget {}
    }
    assert_eq!(h.get::<Works>().unwrap().pieces.len(), 2);
    assert_eq!(
        h.get::<game_pastimes::Pastimes>().unwrap().experiences[&author]["compose-song"],
        2
    );
    assert!(h.get::<Works>().unwrap().next.is_empty());
}

#[test]
fn a_care_manual_preserves_understood_lessons_after_its_author_dies() {
    let (mut h, author, reader, inventory) = setup();
    let claim = game_reasoning::Claim {
        subject: "child-care".into(),
        factor: "dose".into(),
        outcome: "relief".into(),
        effect: 100,
        context: Default::default(),
        comparisons: 4,
        complexity: 1,
        confidence: 80,
    };
    let key = claim.key();
    h.dispatch(game_knowledge::Conclude {
        person: author,
        key: key.clone(),
        value: serde_json::to_value(&claim).unwrap(),
        confidence: 80,
    })
    .unwrap();
    let end = h
        .dispatch(game_pastimes::Pursue {
            person: author,
            activity: "write-book".into(),
            inventory,
        })
        .unwrap();
    while h.advance(end, 128).unwrap().stop == Stop::Budget {}
    let id = *h.get::<Works>().unwrap().pieces.keys().next().unwrap();
    assert!(
        h.get::<Works>().unwrap().pieces[&id]
            .evidence
            .iter()
            .any(|(k, _)| k == &key)
    );
    h.dispatch(Present {
        holder: author,
        listener: reader,
        work: id,
    })
    .unwrap();
    learn_script(&mut h, author, reader, id);
    let gloss = h.get::<Works>().unwrap().pieces[&id]
        .composition
        .as_ref()
        .unwrap()
        .gloss
        .clone();
    let explanation = vec![
        format!("person:{}", author.0),
        format!("person:{}", reader.0),
    ];
    for meaning in &explanation {
        teach(&mut h, author, reader, meaning);
    }
    for meaning in gloss {
        let received = h
            .dispatch(game_communication::Convey {
                speaker: author,
                listener: reader,
                concept: meaning.clone(),
                demonstrate: true,
                explanation: explanation.clone(),
            })
            .unwrap();
        assert_eq!(received.interpretation.as_deref(), Some(meaning.as_str()));
    }
    assert!(
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .get(&reader)
            .is_none_or(|f| !f.contains_key(&key))
    );
    let reader_pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(reader),
            capacity: 1000,
        })
        .unwrap();
    let item = h.get::<Works>().unwrap().pieces[&id]
        .composition
        .as_ref()
        .unwrap()
        .item
        .unwrap();
    h.dispatch(game_items::Move {
        person: author,
        item,
        destination: reader_pack,
    })
    .unwrap();
    h.dispatch(game_physiology::Damage {
        person: author,
        amount: 100,
    })
    .unwrap();
    assert!(!h.get::<game_physiology::Bodies>().unwrap().people[&author].alive);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        let words = host.dispatch(Read { reader, work: id }).unwrap();
        assert!(
            words
                .iter()
                .any(|w| w.meaning.as_deref() == Some(key.as_str())),
            "decoded words: {words:?}"
        );
        assert!(host.get::<game_knowledge::Knowledge>().unwrap().0[&reader].contains_key(&key));
        assert!(host.get::<game_skills::Skills>().unwrap().levels[&reader]["child-care"] > 0);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn personal_care_can_become_a_song_without_teaching_bystanders_the_memory() {
    let (mut h, author, patient, pack) = setup();
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("grain")
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_physiology::Feed {
        caregiver: author,
        patient,
        inventory: pack,
    })
    .unwrap();
    let memory = game_knowledge::with_prefix(
        &h.get::<game_knowledge::Knowledge>().unwrap().0[&author],
        "memory:episode:",
    )
    .next()
    .unwrap()
    .0
    .to_string();
    let until = h
        .dispatch(game_pastimes::Pursue {
            person: author,
            activity: "compose-song".into(),
            inventory: pack,
        })
        .unwrap();
    while h.advance(until, 128).unwrap().stop == Stop::Budget {}
    let work = h.get::<Works>().unwrap().pieces.values().last().unwrap();
    assert!(work.evidence.iter().any(|(key, _)| key == &memory));
    let text = work.composition.as_ref().unwrap();
    assert!(text.gloss.iter().any(|s| s == "relation:fed"));
    assert!(text.gloss.contains(&format!("person:{}", patient.0)));
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
