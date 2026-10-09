//! A bounded host queue and one simulation owner. Control input is serviced between
//! committed quanta; no world lock is held by clients while printing or drawing.
use crate::*;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};

type Reply<T> = Sender<Result<T, Error>>;
enum Request {
    Observe(Reply<PlayerView>),
    Submit(Submission, Reply<CommandReceipt>),
    Run(SimDuration, Reply<AdvanceResult>),
    Pause(Reply<PlayerView>),
    Save(Reply<Vec<u8>>),
    Shutdown,
}
pub struct Session {
    tx: SyncSender<Request>,
    thread: Option<std::thread::JoinHandle<()>>,
}
pub struct RunHandle {
    rx: Receiver<Result<AdvanceResult, Error>>,
}
impl RunHandle {
    pub fn wait(self) -> Result<AdvanceResult, Error> {
        self.rx.recv().map_err(|_| Error::HostDisconnected)?
    }
}

impl Session {
    pub fn spawn(mut simulation: Simulation) -> Result<Self, Error> {
        let (tx, rx) = mpsc::sync_channel(64);
        let thread = std::thread::Builder::new()
            .name("game-session".into())
            .spawn(move || {
                let mut active: Option<(SimTime, Reply<AdvanceResult>)> = None;
                loop {
                    let request = if active.is_some() {
                        match rx.try_recv() {
                            Ok(r) => Some(r),
                            Err(TryRecvError::Empty) => None,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(r) => Some(r),
                            Err(_) => break,
                        }
                    };
                    if let Some(request) = request {
                        match request {
                            Request::Shutdown => break,
                            Request::Observe(reply) => {
                                let _ = reply.send(Ok(simulation.observe()));
                            }
                            Request::Submit(command, reply) => {
                                let _ = reply.send(simulation.submit(command));
                            }
                            Request::Save(reply) => {
                                let _ = reply.send(simulation.save());
                            }
                            Request::Pause(reply) => {
                                if let Some((target, run_reply)) = active.take() {
                                    let _ = run_reply.send(Ok(AdvanceResult {
                                        requested: target,
                                        reached: simulation.time(),
                                        revision: simulation.revision,
                                        reason: StopReason::Paused,
                                    }));
                                }
                                let _ = reply.send(Ok(simulation.observe()));
                            }
                            Request::Run(duration, reply) => {
                                if active.is_some() {
                                    let _ = reply.send(Err(Error::InvalidConfiguration(
                                        "an advance is already active".into(),
                                    )));
                                } else if let Some(target) =
                                    simulation.time().0.checked_add(duration.0)
                                {
                                    active = Some((SimTime(target), reply));
                                } else {
                                    let _ = reply.send(Err(Error::Overflow));
                                }
                            }
                        }
                    }
                    // Service one bounded interval even with a stream of passive queries.
                    if let Some((target, _)) = &active {
                        let target = *target;
                        let quantum = SimDuration((target.0 - simulation.time().0).min(60_000));
                        match simulation.advance(quantum) {
                            Err(error) => {
                                let (_, reply) = active.take().unwrap();
                                let _ = reply.send(Err(error));
                            }
                            Ok(mut result) if simulation.time() == target => {
                                result.requested = target;
                                let (_, reply) = active.take().unwrap();
                                let _ = reply.send(Ok(result));
                            }
                            _ => {}
                        }
                    }
                }
            })
            .map_err(|e| Error::InvalidConfiguration(e.to_string()))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    fn ask<T>(&self, build: impl FnOnce(Reply<T>) -> Request) -> Result<T, Error> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(build(tx))
            .map_err(|_| Error::HostDisconnected)?;
        rx.recv().map_err(|_| Error::HostDisconnected)?
    }
    pub fn observe(&self) -> Result<PlayerView, Error> {
        self.ask(Request::Observe)
    }
    pub fn submit(&self, command: Submission) -> Result<CommandReceipt, Error> {
        self.ask(|r| Request::Submit(command, r))
    }
    pub fn pause(&self) -> Result<PlayerView, Error> {
        self.ask(Request::Pause)
    }
    pub fn save(&self) -> Result<Vec<u8>, Error> {
        self.ask(Request::Save)
    }
    pub fn run(&self, duration: SimDuration) -> Result<RunHandle, Error> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Request::Run(duration, tx))
            .map_err(|_| Error::HostDisconnected)?;
        Ok(RunHandle { rx })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
