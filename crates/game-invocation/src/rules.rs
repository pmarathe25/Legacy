use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub activation_burden: u64,
    pub activation_fatigue: u64,
    pub strain_thresholds: [u64; 3],
    pub emotional_thresholds: [u64; 3],
    pub identity_thresholds: [u64; 3],
    pub strain_per_mille: [u64; 4],
    pub health_harm_per_mille: [u64; 4],
    pub loss_threshold: u64,
    pub loss_gap: u64,
    pub loss_duration: SimDuration,
    pub permanent_burden: u64,
    pub permanent_exposure: SimDuration,
    pub permanent_interval: SimDuration,
    pub permanent_identity_per_band: u16,
    pub lifespan_loss_per_band: SimDuration,
    pub burden_recovery_per_mille: u64,
    pub exposure_recovery_per_mille: u64,
    pub controller_quantum: SimDuration,
    pub writing_time: SimDuration,
    pub reading_time: SimDuration,
    pub teaching_time: SimDuration,
    pub moving_time: SimDuration,
}

impl Rules {
    pub fn human() -> Self {
        let hour = 3_600_000;
        let day = 24 * hour;
        Self {
            activation_burden: 60_000,
            activation_fatigue: 10_000,
            strain_thresholds: [hour, 8 * hour, 2 * day],
            emotional_thresholds: [2 * hour, 12 * hour, 3 * day],
            identity_thresholds: [4 * hour, day, 4 * day],
            strain_per_mille: [0, 50, 250, 2_000],
            health_harm_per_mille: [0, 0, 0, 500],
            loss_threshold: day,
            loss_gap: 6 * hour,
            loss_duration: SimDuration(6 * hour),
            permanent_burden: 30 * day,
            permanent_exposure: SimDuration(90 * day),
            permanent_interval: SimDuration(30 * day),
            permanent_identity_per_band: 10,
            lifespan_loss_per_band: SimDuration(365 * day),
            burden_recovery_per_mille: 1_000,
            exposure_recovery_per_mille: 100,
            controller_quantum: MINUTE,
            writing_time: MINUTE,
            reading_time: MINUTE,
            teaching_time: MINUTE,
            moving_time: MINUTE,
        }
    }
    pub fn fixture() -> Self {
        Self {
            activation_burden: 6_000,
            activation_fatigue: 1_000,
            strain_thresholds: [120_000, 360_000, 720_000],
            emotional_thresholds: [180_000, 420_000, 780_000],
            identity_thresholds: [240_000, 480_000, 840_000],
            strain_per_mille: [0, 50, 250, 2_000],
            health_harm_per_mille: [0, 0, 0, 500],
            loss_threshold: 300_000,
            loss_gap: 180_000,
            loss_duration: SimDuration(420_000),
            permanent_burden: 600_000,
            permanent_exposure: SimDuration(600_000),
            permanent_interval: SimDuration(300_000),
            permanent_identity_per_band: 10,
            lifespan_loss_per_band: MINUTE,
            burden_recovery_per_mille: 1_000,
            exposure_recovery_per_mille: 100,
            controller_quantum: SimDuration(6_000),
            writing_time: MINUTE,
            reading_time: MINUTE,
            teaching_time: MINUTE,
            moving_time: MINUTE,
        }
    }
}

pub fn validate_rules(r: &Rules) -> Result<(), Error> {
    let increasing =
        |values: [u64; 3]| values[0] > 0 && values[0] < values[1] && values[1] < values[2];
    if !increasing(r.strain_thresholds)
        || !increasing(r.emotional_thresholds)
        || !increasing(r.identity_thresholds)
        || r.strain_per_mille
            .iter()
            .chain(r.health_harm_per_mille.iter())
            .any(|v| *v > 1_000_000)
        || r.activation_fatigue == 0
        || r.activation_burden == 0
        || r.loss_threshold == 0
        || r.loss_gap == 0
        || r.loss_duration.0 == 0
        || r.permanent_burden < r.loss_threshold
        || r.permanent_exposure.0 == 0
        || r.permanent_interval.0 == 0
        || r.permanent_identity_per_band == 0
        || r.permanent_identity_per_band > 100
        || r.lifespan_loss_per_band.0 == 0
        || r.lifespan_loss_per_band.0.checked_mul(252).is_none()
        || r.burden_recovery_per_mille == 0
        || r.burden_recovery_per_mille > 1_000_000
        || r.exposure_recovery_per_mille == 0
        || r.exposure_recovery_per_mille > 1_000
        || r.controller_quantum.0 == 0
        || r.controller_quantum > r.loss_duration
        || [
            r.writing_time,
            r.reading_time,
            r.teaching_time,
            r.moving_time,
        ]
        .iter()
        .any(|d| d.0 == 0)
    {
        return Err(Error::InvalidConfiguration(
            "invalid invocation rules".into(),
        ));
    }
    Ok(())
}
