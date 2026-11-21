use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn default_cli_steps_an_elapsed_world_and_reloads_its_checkpoint() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checkpoint =
        std::env::temp_dir().join(format!("legacy-elapsed-shell-{}.json", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-economy-cli"))
        .arg(root.join("content/scenarios/economy-core-language-topics.toml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        writeln!(
            input,
            "step 61\nsave {}\nstep 44\nload {}\nstep 19\nstatus\nquit",
            checkpoint.display(),
            checkpoint.display()
        )
        .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let saved = std::fs::read(&checkpoint);
    if saved.is_ok() {
        std::fs::remove_file(checkpoint).unwrap();
    }
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success() && errors.is_empty(), "{errors}");
    let transcript = String::from_utf8(output.stdout).unwrap();
    assert!(transcript.contains("Elapsed simulation"));
    assert!(transcript.contains("elapsed_millis=61 now_millis=61 boundaries=0"));
    assert!(transcript.contains("elapsed_millis=44 now_millis=105 boundaries=0"));
    assert!(transcript.contains("elapsed_millis=19 now_millis=80 boundaries=0"));
    assert!(transcript.contains("now_millis=80 tick=0 people=2 living=2"));
    let saved: game_economy::core_interval::CoreHostSnapshot =
        serde_json::from_slice(&saved.unwrap()).unwrap();
    assert!(saved.core.people.iter().all(|person| !person.controlled));
}
