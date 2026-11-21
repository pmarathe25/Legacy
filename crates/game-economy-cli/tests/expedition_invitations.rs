use game_economy::core_interval::{CoreHost, ExpeditionReplyKind};

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
