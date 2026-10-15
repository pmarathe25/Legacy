//! Threading policy and responsive control; game rules remain in feature crates.
use game_api::*;
use game_assembly::Simulation;
use game_sim::{Progress, Stop};
use std::sync::{Arc, Mutex, mpsc};
pub enum Input {
    Command { sequence: u64, command: Command },
    Advance(SimTime),
    Pause,
    Observe,
    Save,
    Shutdown,
}
pub enum Output {
    Command(Result<CommandResult>),
    Progress(Result<Progress>),
    View(Result<Box<PlayerView>>),
    Save(Result<String>),
    Paused(SimTime),
}
pub struct Session {
    pub input: mpsc::Sender<Input>,
    pub output: mpsc::Receiver<Output>,
    progress: Arc<Mutex<Option<Progress>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Session {
    /// Ordinary progress occupies one slot even when the consumer is slow.
    pub fn progress(&self) -> Option<Progress> {
        self.progress.lock().expect("session progress lock").clone()
    }
    pub fn spawn(mut simulation: Simulation) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (out, output) = mpsc::channel();
        let progress = Arc::new(Mutex::new(None));
        let latest = progress.clone();
        let thread = std::thread::Builder::new()
            .name("legacy-session".into())
            .spawn(move || {
                let mut target = None;
                loop {
                    let input = if target.is_some() {
                        match receiver.try_recv() {
                            Ok(i) => Some(i),
                            Err(mpsc::TryRecvError::Empty) => None,
                            Err(_) => break,
                        }
                    } else {
                        match receiver.recv() {
                            Ok(i) => Some(i),
                            Err(_) => break,
                        }
                    };
                    let message = match input {
                        Some(Input::Command { sequence, command }) => {
                            Some(Output::Command(simulation.submit(sequence, command)))
                        }
                        Some(Input::Advance(time)) => {
                            target = Some(time);
                            None
                        }
                        Some(Input::Pause) => {
                            target = None;
                            Some(Output::Paused(simulation.now()))
                        }
                        Some(Input::Observe) => {
                            Some(Output::View(simulation.observe().map(Box::new)))
                        }
                        Some(Input::Save) => Some(Output::Save(simulation.save())),
                        Some(Input::Shutdown) => break,
                        None => None,
                    };
                    if let Some(message) = message
                        && out.send(message).is_err()
                    {
                        break;
                    }
                    if let Some(time) = target {
                        let progress = simulation.advance(time, 32);
                        if !progress.as_ref().is_ok_and(|p| p.stop == Stop::Budget) {
                            target = None;
                        }
                        if let Ok(ref progress) = progress {
                            *latest.lock().expect("session progress lock") = Some(progress.clone());
                        }
                        if !progress.as_ref().is_ok_and(|p| p.stop == Stop::Budget)
                            && out.send(Output::Progress(progress)).is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .map_err(|_| Error::WorkerFailure)?;
        Ok(Self {
            input: sender,
            output,
            progress,
            thread: Some(thread),
        })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.input.send(Input::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
