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
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-economy-cli"))
        .current_dir(&root)
        .args(["--interval-player", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "look\nmove 2\nwait 1000\nlook\nsave {}\nload {}\nquit\n",
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
    assert!(stdout.contains("person=1 region=Some(2)"), "{stdout}");
    assert!(
        stdout.contains("saved") && stdout.contains("loaded"),
        "{stdout}"
    );
    std::fs::remove_file(save).unwrap();
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
