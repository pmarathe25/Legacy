use game_economy::core_interval::{CoreHost, ExpeditionReplyKind};

#[test]
fn autonomous_invitations_arbitrate_by_the_recipients_own_preferences() {
    use game_economy::core_interval::{CoreRunEvent, ExpeditionEventKind};
    let simulate = |workers, willing, obligation| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let template = std::fs::read_to_string(root.join("content/scenarios/economy-core-player.toml")).unwrap();
            let mut source = template.split_once("[hauling]").unwrap().0.replace("controlled = true", "controlled = false");
            for id in 2..=3 {
                source.push_str(&format!("\n[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\n"));
            }
            source.push_str(&format!(r#"
[[expedition_invitation_preferences]]
person = 2
destination = 2
willing = {willing}
return_after_visit = true

[[expedition_invitation_initiatives]]
person = 1
companions = [{{ person = 2, value = 20 }}]

[[expedition_invitation_initiatives]]
person = 3
companions = [{{ person = 2, value = 30 }}]

[opinions.rules]
minimum = -100
maximum = 100
initial = 0
fulfilled_gain = 1
default_loss = 1
affinity = {{ limit = 100 }}

[[opinions.records]]
observer = 2
target = 1
trust = 0
affinity = 10
obligation = {obligation}

[[opinions.records]]
observer = 2
target = 3
trust = 0
affinity = -7

[missions]
next_id = 3
pressure_threshold = 3
entries = [
    {{ id = 1, person = 1, origin = 1, destination = 2, visit_ticks = 2, phase = "ready" }},
    {{ id = 2, person = 3, origin = 1, destination = 2, visit_ticks = 3, phase = "ready" }},
]
"#));
            let path = std::env::temp_dir().join(format!("legacy-npc-invitation-{}.toml", std::process::id()));
            std::fs::write(&path, source).unwrap();
            let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
            std::fs::remove_file(path).unwrap();
            let mut host = loaded.unwrap();
            let run = host.advance_continuous(57, &[]).unwrap();
            assert_eq!(run.failure, None);
            let expected = if willing { ExpeditionReplyKind::Joined } else if obligation >= 3 { ExpeditionReplyKind::JoinedUnderPressure } else { ExpeditionReplyKind::Declined };
            let receipts: Vec<_> = run.events.iter().filter_map(|row| {
                if let CoreRunEvent::Expeditions(events) = row { Some(events) } else { None }
            }).flatten().filter_map(|event| {
                if let ExpeditionEventKind::Invitation { recipient, response, .. } = event.kind {
                    Some((event.person, recipient, response))
                } else { None }
            }).collect();
            assert_eq!(receipts, [(1, 2, Some(expected))]);
            if expected == ExpeditionReplyKind::Declined {
                assert!(host.expedition(2).is_none());
            } else {
                let joined = host.expedition(2).unwrap();
                assert_ne!(joined.id, 1);
                assert_eq!(joined.visit_millis, 2000);
                assert!(joined.head_home);
            }
            assert!(host.expedition(1).is_some());
            assert!(host.expedition(3).is_some());
            let snapshot = host.snapshot();
            let mut resumed = CoreHost::resume(serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap()).unwrap();
            assert_eq!(host.advance_continuous(1943, &[]).unwrap().failure, None);
            assert_eq!(resumed.advance_continuous(1943, &[]).unwrap().failure, None);
            assert_eq!(host.snapshot(), resumed.snapshot());
            serde_json::to_value(host.snapshot()).unwrap()
        })
    };
    for (willing, obligation) in [(true, 0), (false, 0), (false, 4)] {
        assert_eq!(
            simulate(1, willing, obligation),
            simulate(4, willing, obligation)
        );
    }
}

#[test]
fn invitees_choose_distinct_itineraries_and_can_decline_or_leave() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let template =
        std::fs::read_to_string(root.join("content/scenarios/economy-core-player.toml")).unwrap();
    let mut source = template.split_once("[hauling]").unwrap().0.to_owned();
    for id in 2..=5 {
        source.push_str(&format!(
            "\n[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\ncontrolled = {}\n",
            id == 4,
        ));
    }
    source.push_str(
        r#"
[[expedition_invitation_preferences]]
person = 2
destination = 2
willing = true
return_after_visit = true

[opinions.rules]
minimum = -100
maximum = 100
initial = 0
fulfilled_gain = 1
default_loss = 1

[[opinions.records]]
observer = 3
target = 1
trust = 0
obligation = 4

[missions]
next_id = 1
entries = []
pressure_threshold = 3
"#,
    );
    let path = std::env::temp_dir().join(format!("legacy-invitation-{}.toml", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut host = loaded.unwrap();
    host.plan_controlled_expedition(1, 2, 150, None).unwrap();
    assert_eq!(
        host.invite_controlled_expedition(1, 2)
            .unwrap()
            .response
            .unwrap()
            .response,
        ExpeditionReplyKind::Joined
    );
    assert_eq!(
        host.invite_controlled_expedition(1, 3)
            .unwrap()
            .response
            .unwrap()
            .response,
        ExpeditionReplyKind::JoinedUnderPressure
    );
    assert_eq!(
        host.invite_controlled_expedition(1, 5)
            .unwrap()
            .response
            .unwrap()
            .response,
        ExpeditionReplyKind::Declined
    );
    assert!(host.expedition(5).is_none());
    assert!(host.expedition(2).unwrap().head_home);
    assert!(!host.expedition(3).unwrap().head_home);
    let offered = host.invite_controlled_expedition(1, 4).unwrap();
    assert!(offered.response.is_none());
    assert!(host.expedition(4).is_none());

    let checkpoint = |host: &CoreHost| {
        CoreHost::resume(
            serde_json::from_str(&serde_json::to_string(&host.snapshot()).unwrap()).unwrap(),
        )
        .unwrap()
    };
    let mut host = checkpoint(&host);
    assert_eq!(
        host.expedition_invitations(4).unwrap(),
        [offered.invitation]
    );
    assert!(
        host.reply_controlled_expedition(1, offered.invitation.id, true)
            .is_err()
    );
    assert_eq!(
        host.reply_controlled_expedition(4, offered.invitation.id, false)
            .unwrap()
            .response,
        ExpeditionReplyKind::Declined
    );
    let mut host = checkpoint(&host);
    let other_offer = host.invite_controlled_expedition(1, 4).unwrap();
    assert!(other_offer.invitation.id > offered.invitation.id);
    let accepted = host
        .reply_controlled_expedition(4, other_offer.invitation.id, true)
        .unwrap();
    assert_eq!(accepted.response, ExpeditionReplyKind::Joined);
    assert_ne!(
        accepted.joined_itinerary,
        Some(other_offer.invitation.itinerary)
    );
    host.abandon_controlled_expedition(4).unwrap();
    assert!(host.expedition(4).is_none());
    assert!(host.expedition(1).is_some());
    assert!(host.expedition(2).is_some());
}
