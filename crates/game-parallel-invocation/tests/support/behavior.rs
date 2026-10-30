use crate::{Invocation, Rules, WarningKind};
fn state() -> Invocation {
    Invocation {
        rules: Rules {
            rate_per_life: 2,
            recovery_per_tick: 1,
            strain_threshold: 4,
            loss_threshold: 8,
            strain_consent_ticks: 5,
            control_loss_ticks: 3,
            strain_damage_per_tick: 0,
            permanent_exposure_threshold: 0,
            lifespan_loss_per_tick: 0,
        },
        owner: Some(9),
        active: vec![],
        burden: 0,
        exposure: 0,
        strain_accepted_until: None,
        lifespan_accepted_until: None,
        loss_approved: false,
        loss_triggered: false,
        control_until: None,
        pending: None,
        revision: 0,
    }
}
#[test]
fn warnings_stop_time_and_require_the_current_explicit_consent() {
    let mut state = state();
    state.activate(&[3], &[3], 0).unwrap();
    assert!(state.prepare(0).unwrap());
    state.advance(0).unwrap();
    assert!(!state.prepare(1).unwrap());
    let warning = state.pending.clone().unwrap();
    assert_eq!(warning.kind, WarningKind::Strain);
    assert_eq!(state.burden, 2);
    assert!(state.accept(warning.revision + 1, 1).is_err());
    state.accept(warning.revision, 1).unwrap();
    state.advance(1).unwrap();
    state.advance(2).unwrap();
    assert!(!state.prepare(3).unwrap());
    let warning = state.pending.clone().unwrap();
    assert_eq!(warning.kind, WarningKind::ControlLoss);
    state.accept(warning.revision, 3).unwrap();
    state.advance(3).unwrap();
    assert!(state.dismiss(4).is_err());
    state.advance(4).unwrap();
    state.advance(5).unwrap();
    assert!(state.control_until.is_none());
    let exposure = state.exposure;
    let burden = state.burden;
    state.dismiss(6).unwrap();
    state.advance(6).unwrap();
    assert_eq!(state.exposure, exposure);
    assert_eq!(state.burden, burden - 1);
}
#[test]
fn changing_lives_invalidates_old_warnings_without_erasing_exposure() {
    let mut state = state();
    state.activate(&[3], &[3, 4], 0).unwrap();
    state.advance(0).unwrap();
    assert!(!state.prepare(1).unwrap());
    let old = state.pending.as_ref().unwrap().revision;
    state.activate(&[4], &[3, 4], 1).unwrap();
    assert!(state.accept(old, 1).is_err());
    assert_eq!(state.exposure, 1);
    assert_eq!(state.burden, 2);
    state.bind(Some(10));
    assert!(state.active.is_empty());
    assert_eq!(state.burden, 0);
    assert_eq!(state.owner, Some(10));
}
