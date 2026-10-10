use super::*;
use std::sync::{Arc, Mutex};

struct Gate {
    entered: SyncSender<SimDuration>,
    release: Mutex<Receiver<()>>,
}
struct FakeEngine {
    time: SimTime,
    revision: u64,
    sequence: u64,
    gate: Option<Arc<Gate>>,
    stop: Option<StopReason>,
    fail: bool,
}
impl Default for FakeEngine {
    fn default() -> Self {
        Self {
            time: SimTime(0),
            revision: 0,
            sequence: 1,
            gate: None,
            stop: None,
            fail: false,
        }
    }
}
impl FakeEngine {
    fn gated() -> (Self, Receiver<SimDuration>, Sender<()>) {
        let (entered, observe) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::channel();
        (
            Self {
                gate: Some(Arc::new(Gate {
                    entered,
                    release: Mutex::new(wait),
                })),
                ..Self::default()
            },
            observe,
            release,
        )
    }
}
impl SessionEngine for FakeEngine {
    fn time(&self) -> SimTime {
        self.time
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn observe(&self) -> PlayerView {
        PlayerView {
            agriculture: None,
            armies: Vec::new(),
            realm: RealmView::default(),
            transport: None,
            genetics: None,
            social: None,
            enforcement: EnforcementView::default(),
            health: None,
            ecology: EcologyView::default(),
            missions: Vec::new(),
            force: ForceView::default(),
            invocation: None,
            projects: ProjectsView::default(),
            skills: Vec::new(),
            institutions: Vec::new(),
            social_cases: Vec::new(),
            property: PropertyView::default(),
            lineage: None,
            agreements: Vec::new(),
            environment: None,
            revision: self.revision,
            time: self.time,
            ruler: PersonId(19),
            location: Location::AtArea(AreaId(7)),
            known_areas: Vec::new(),
            available_commands: vec![Command::Rest],
            quantity_opportunities: Vec::new(),
            physical: None,
            known_stocks: Vec::new(),
            nearby_people: Vec::new(),
            nearby_life_stages: Vec::new(),
            combat: None,
            interests: Vec::new(),
            life_stage: None,
            preference: None,
        }
    }
    fn submit(&mut self, submission: Submission) -> Result<CommandReceipt, Error> {
        if submission.command != Command::Rest {
            return Err(Error::Unavailable);
        }
        self.sequence = submission
            .caller_sequence
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.revision += 1;
        Ok(CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.revision,
            accepted_at: self.time,
        })
    }
    fn next_caller_sequence(&self) -> Result<u64, Error> {
        Ok(self.sequence)
    }
    fn advance(&mut self, duration: SimDuration) -> Result<AdvanceResult, Error> {
        if self.fail {
            return Err(Error::Overflow);
        }
        if let Some(gate) = &self.gate {
            gate.entered.send(duration).unwrap();
            gate.release.lock().unwrap().recv().unwrap();
        }
        let requested = SimTime(self.time.0.checked_add(duration.0).ok_or(Error::Overflow)?);
        self.time.0 += if self.stop.is_some() {
            duration.0.min(6_000)
        } else {
            duration.0
        };
        if duration.0 > 0 {
            self.revision += 1;
        }
        Ok(AdvanceResult {
            requested,
            reached: self.time,
            revision: self.revision,
            reason: self.stop.clone().unwrap_or(StopReason::TargetReached),
        })
    }
    fn save(&self) -> Result<Vec<u8>, Error> {
        Ok([self.time.0, self.revision, self.sequence]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect())
    }
    fn restored(&self, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != 24 {
            return Err(Error::InvalidSave("fake engine snapshot".into()));
        }
        let number = |offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        Ok(Self {
            time: SimTime(number(0)),
            revision: number(8),
            sequence: number(16),
            gate: self.gate.clone(),
            stop: self.stop.clone(),
            fail: self.fail,
        })
    }
    fn debug_json(&self) -> Result<String, Error> {
        Ok(format!(
            "{{\"time\":{},\"revision\":{}}}",
            self.time.0, self.revision
        ))
    }
}
fn queued_pause(session: &Session) -> Receiver<Result<PlayerView, Error>> {
    let (reply, receiver) = mpsc::channel();
    session.tx.send(Request::Pause(reply)).unwrap();
    receiver
}

#[test]
fn automatic_sequences_debug_and_save_are_owned_by_the_engine() {
    let session = Session::spawn(FakeEngine::default()).unwrap();
    let first = session.submit_command(Command::Rest).unwrap();
    assert_eq!(first.caller_sequence, 1);
    assert_eq!(first.authoritative_sequence, 1);
    let second = session
        .submit(Submission {
            caller_sequence: 40,
            command: Command::Rest,
        })
        .unwrap();
    assert_eq!(second.caller_sequence, 40);
    assert_eq!(
        session
            .submit_command(Command::Rest)
            .unwrap()
            .caller_sequence,
        41
    );
    assert_eq!(session.debug_json().unwrap(), "{\"time\":0,\"revision\":3}");
    let before = session.save().unwrap();
    assert!(session.submit_command(Command::Cancel).is_err());
    assert_eq!(session.save().unwrap(), before);
    let loaded = FakeEngine::default().restored(&before).unwrap();
    assert_eq!(session.observe().unwrap(), loaded.observe());
}

#[test]
fn passive_queries_do_not_starve_committed_quanta_and_pause_is_a_boundary() {
    let (engine, entered, release) = FakeEngine::gated();
    let session = Session::spawn(engine).unwrap();
    let run = session.run(SimDuration(240_000)).unwrap();
    assert_eq!(entered.recv().unwrap(), SimDuration(60_000));
    let (reply_a, view_a) = mpsc::channel();
    let (reply_b, view_b) = mpsc::channel();
    session.tx.send(Request::Observe(reply_a)).unwrap();
    session.tx.send(Request::Observe(reply_b)).unwrap();
    let paused = queued_pause(&session);
    release.send(()).unwrap();
    assert_eq!(view_a.recv().unwrap().unwrap().time, SimTime(60_000));
    assert_eq!(entered.recv().unwrap(), SimDuration(60_000));
    release.send(()).unwrap();
    assert_eq!(view_b.recv().unwrap().unwrap().time, SimTime(120_000));
    entered.recv().unwrap();
    release.send(()).unwrap();
    assert_eq!(paused.recv().unwrap().unwrap().time, SimTime(180_000));
    let result = run.wait().unwrap();
    assert_eq!(result.requested, SimTime(240_000));
    assert_eq!(result.reached, SimTime(180_000));
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(session.observe().unwrap().time, result.reached);
    assert!(entered.try_recv().is_err());
}

#[test]
fn successful_load_pauses_with_the_old_engines_time_and_revision() {
    let (mut engine, entered, release) = FakeEngine::gated();
    engine.time = SimTime(7);
    engine.revision = 3;
    let session = Session::spawn(engine).unwrap();
    let run = session.run(SimDuration(1_000_000)).unwrap();
    entered.recv().unwrap();
    let replacement = FakeEngine {
        time: SimTime(900),
        revision: 50,
        sequence: 12,
        ..FakeEngine::default()
    };
    let (reply, loaded) = mpsc::channel();
    session
        .tx
        .send(Request::Load(replacement.save().unwrap(), reply))
        .unwrap();
    release.send(()).unwrap();
    let result = run.wait().unwrap();
    assert_eq!(result.requested, SimTime(1_000_007));
    assert_eq!(result.reached, SimTime(60_007));
    assert_eq!(result.revision, 4);
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(loaded.recv().unwrap().unwrap(), replacement.observe());
    assert_eq!(session.observe().unwrap(), replacement.observe());
    assert_eq!(
        session
            .submit_command(Command::Rest)
            .unwrap()
            .caller_sequence,
        12
    );
    assert!(entered.try_recv().is_err());
}

#[test]
fn invalid_load_leaves_the_engine_and_active_run_intact() {
    let (mut engine, entered, release) = FakeEngine::gated();
    engine.time = SimTime(7);
    engine.revision = 3;
    let session = Session::spawn(engine).unwrap();
    let run = session.run(SimDuration(1_000_000)).unwrap();
    entered.recv().unwrap();
    let (reply, invalid) = mpsc::channel();
    session
        .tx
        .send(Request::Load(b"invalid".to_vec(), reply))
        .unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        invalid.recv().unwrap(),
        Err(Error::InvalidSave(_))
    ));
    // The unchanged run proceeds through another actual committed quantum.
    assert_eq!(entered.recv().unwrap(), SimDuration(60_000));
    let paused = queued_pause(&session);
    release.send(()).unwrap();
    let view = paused.recv().unwrap().unwrap();
    assert_eq!(view.time, SimTime(120_007));
    assert_eq!(view.revision, 5);
    assert_eq!(run.wait().unwrap().reason, StopReason::Paused);
    let before = session.save().unwrap();
    assert!(session.load(b"invalid".to_vec()).is_err());
    assert_eq!(session.save().unwrap(), before);
}

#[test]
fn the_host_queue_is_bounded_while_the_engine_owns_a_quantum() {
    let (engine, entered, release) = FakeEngine::gated();
    let session = Session::spawn(engine).unwrap();
    let run = session.run(SimDuration(60_000)).unwrap();
    entered.recv().unwrap();
    for _ in 0..64 {
        let (reply, _) = mpsc::channel();
        session.tx.try_send(Request::Observe(reply)).unwrap();
    }
    let (reply, _) = mpsc::channel();
    assert!(matches!(
        session.tx.try_send(Request::Observe(reply)),
        Err(mpsc::TrySendError::Full(_))
    ));
    release.send(()).unwrap();
    assert_eq!(run.wait().unwrap().reason, StopReason::TargetReached);
    assert_eq!(session.observe().unwrap().time, SimTime(60_000));
}

#[test]
fn input_stops_and_engine_errors_complete_runs_without_spinning() {
    let session = Session::spawn(FakeEngine {
        stop: Some(StopReason::CombatInput {
            encounter: EncounterId(17),
        }),
        ..FakeEngine::default()
    })
    .unwrap();
    let result = session.run(SimDuration(3_600_000)).unwrap().wait().unwrap();
    assert_eq!(result.requested, SimTime(3_600_000));
    assert_eq!(result.reached, SimTime(6_000));
    assert_eq!(
        result.reason,
        StopReason::CombatInput {
            encounter: EncounterId(17)
        }
    );
    let session = Session::spawn(FakeEngine {
        fail: true,
        ..FakeEngine::default()
    })
    .unwrap();
    assert_eq!(
        session.run(SimDuration(60_000)).unwrap().wait(),
        Err(Error::Overflow)
    );
    assert_eq!(session.observe().unwrap().time, SimTime(0));
}

#[test]
fn shutdown_reports_the_last_committed_boundary_and_disconnects_clients() {
    let (engine, entered, release) = FakeEngine::gated();
    let session = Session::spawn(engine).unwrap();
    let run = session.run(SimDuration(1_000_000)).unwrap();
    entered.recv().unwrap();
    session.tx.send(Request::Shutdown).unwrap();
    release.send(()).unwrap();
    let result = run.wait().unwrap();
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(result.reached, SimTime(60_000));
    assert_eq!(session.observe(), Err(Error::HostDisconnected));
}
