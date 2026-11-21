use std::io::{self, BufRead, IsTerminal, Write};

pub(super) fn run(mut execute: impl FnMut(&[&str]) -> Result<bool, String>) -> Result<(), String> {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut lines = stdin.lock().lines();
    loop {
        if interactive {
            print!("> ");
            io::stdout().flush().map_err(|error| error.to_string())?;
        }
        let Some(line) = lines.next() else { break };
        let line = line.map_err(|error| error.to_string())?;
        let words: Vec<_> = line.split_whitespace().collect();
        match execute(&words) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
