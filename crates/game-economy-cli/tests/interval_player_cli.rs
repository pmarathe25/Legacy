use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn interval_player_can_observe_travel_and_reload() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let token = format!(
        "legacy interval player {} {}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let save = std::env::temp_dir().join(format!("{token}.json"));
    let initial = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-player.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let bread = initial
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    let recipe = initial
        .core
        .physical
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fixture-tool")
        .unwrap()
        .id;
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-economy-cli"))
        .current_dir(&root)
        .args(["--interval-player", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "look\neat {bread}\nwork {recipe} 1\nrest on\nwait 500\nlook\nrest off\nstop 1\nmove 2\nwait 1000\nlook\nsave {}\nload {}\nquit\n",
        save.display(),
        save.display()
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("person=1 region=Some(1)"), "{stdout}");
    assert!(stdout.contains("departed=true"), "{stdout}");
    assert!(stdout.contains("consumed=true"), "{stdout}");
    assert!(stdout.contains("job=1 started=true"), "{stdout}");
    assert!(stdout.contains("resting=true"), "{stdout}");
    assert!(stdout.contains("resting=false"), "{stdout}");
    assert!(stdout.contains("stopped"), "{stdout}");
    assert!(stdout.contains("person=1 region=Some(2)"), "{stdout}");
    assert!(
        stdout.contains("saved") && stdout.contains("loaded"),
        "{stdout}"
    );
    std::fs::remove_file(save).unwrap();
}

#[test]
fn interval_player_can_create_and_read_their_own_written_form() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-economy-cli"))
        .current_dir(&root)
        .args(["--interval-player", "1"])
        .arg(root.join("content/scenarios/economy-core-player-language.toml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"look\nform 0,1 7,8\ninscribe 0,1\nlook\nread 1\ninscribe 0,1\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("word=Place(1) sounds=[0, 1]"), "{stdout}");
    assert!(
        stdout.contains("form chosen") && stdout.contains("inscribed_page=1"),
        "{stdout}"
    );
    assert!(stdout.contains("marked_page=1 marks=[7, 8]"), "{stdout}");
    assert!(
        stdout.contains("marks=[7, 8] meanings=[Place(1)]"),
        "{stdout}"
    );
    assert!(stdout.contains("inscribed=false"), "{stdout}");
}

#[test]
fn interval_save_does_not_replace_an_existing_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let token = format!(
        "legacy interval existing {} {}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let save = std::env::temp_dir().join(format!("{token}.json"));
    std::fs::write(&save, b"existing").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-economy-cli"))
        .current_dir(&root)
        .args(["--interval-player", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "save {}\nload {}\nlook\nquit\n",
        save.display(),
        save.display()
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(!output.stderr.is_empty());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("person=1 region=Some(1)")
    );
    assert_eq!(std::fs::read(&save).unwrap(), b"existing");
    std::fs::remove_file(save).unwrap();
}
