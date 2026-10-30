//! Player-authorized, tick-boundary access to previous lives.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub rate_per_life: u64,
    pub recovery_per_tick: u64,
    pub strain_threshold: u64,
    pub loss_threshold: u64,
    pub strain_consent_ticks: u64,
    pub control_loss_ticks: u64,
    /// Physical health lost per strained interval; zero disables direct injury.
    #[serde(default)]
    pub strain_damage_per_tick: u32,
    /// Protracted active intervals required before permanent lifespan costs begin.
    #[serde(default)]
    pub permanent_exposure_threshold: u64,
    #[serde(default)]
    pub lifespan_loss_per_tick: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub rules: Rules,
    pub owner: Option<u64>,
    #[serde(default)]
    pub active: Vec<u64>,
    #[serde(default)]
    pub burden: u64,
    #[serde(default)]
    pub exposure: u64,
    #[serde(default)]
    pub strain_accepted_until: Option<u64>,
    #[serde(default)]
    pub lifespan_accepted_until: Option<u64>,
    #[serde(default)]
    pub loss_approved: bool,
    #[serde(default)]
    pub loss_triggered: bool,
    #[serde(default)]
    pub control_until: Option<u64>,
    #[serde(default)]
    pub pending: Option<Warning>,
    #[serde(default)]
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
    pub revision: u64,
    pub owner: u64,
    pub tick: u64,
    pub rate: u64,
    pub projected_burden: u64,
    pub kind: WarningKind,
    pub duration: u64,
    #[serde(default)]
    pub strain_damage_per_tick: u32,
    #[serde(default)]
    pub lifespan_loss_per_tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    Strain,
    Lifespan,
    ControlLoss,
}
impl Invocation {
    pub fn validate(&self, current: Option<u64>, history: &[u64], now: u64) -> Result<(), String> {
        let mut unique = HashSet::new();
        if self.rules.rate_per_life == 0
            || self.rules.recovery_per_tick == 0
            || self.rules.strain_threshold == 0
            || self.rules.loss_threshold <= self.rules.strain_threshold
            || self.rules.strain_consent_ticks == 0
            || self.rules.control_loss_ticks == 0
            || ((self.rules.permanent_exposure_threshold == 0)
                != (self.rules.lifespan_loss_per_tick == 0))
            || self.owner != current
            || self
                .active
                .iter()
                .any(|id| !unique.insert(*id) || !history.contains(id))
            || (current.is_none()
                && (!self.active.is_empty()
                    || self.control_until.is_some()
                    || self.pending.is_some()))
            || self.control_until.is_some_and(|until| until <= now)
            || self.pending.as_ref().is_some_and(|warning| {
                Some(warning.owner) != current
                    || warning.tick != now
                    || warning.revision != self.revision
            })
        {
            return Err("invalid invocation state".into());
        }
        if let Some(warning) = &self.pending {
            let rate = self.rate()?;
            let projected = self
                .burden
                .checked_add(rate)
                .ok_or("invocation burden overflow")?;
            let duration = match warning.kind {
                WarningKind::Strain | WarningKind::Lifespan => self.rules.strain_consent_ticks,
                WarningKind::ControlLoss => self.rules.control_loss_ticks,
            };
            if rate == 0
                || warning.rate != rate
                || warning.projected_burden != projected
                || warning.duration != duration
                || warning.strain_damage_per_tick != self.rules.strain_damage_per_tick
                || warning.lifespan_loss_per_tick != self.rules.lifespan_loss_per_tick
                || self.control_until.is_some()
                || match warning.kind {
                    WarningKind::Strain => {
                        projected < self.rules.strain_threshold
                            || self.strain_accepted_until.is_some_and(|until| {
                                now.checked_add(1).is_some_and(|next| next <= until)
                            })
                    }
                    WarningKind::Lifespan => {
                        self.rules.lifespan_loss_per_tick == 0
                            || self.exposure.checked_add(1).is_none_or(|exposure| {
                                exposure < self.rules.permanent_exposure_threshold
                            })
                            || self.lifespan_accepted_until.is_some_and(|until| {
                                now.checked_add(1).is_some_and(|next| next <= until)
                            })
                    }
                    WarningKind::ControlLoss => {
                        projected < self.rules.loss_threshold
                            || self.loss_approved
                            || self.loss_triggered
                            || self.strain_accepted_until.is_none_or(|until| {
                                now.checked_add(1).is_none_or(|next| next > until)
                            })
                    }
                }
            {
                return Err("invalid invocation warning".into());
            }
        }
        Ok(())
    }
    pub fn activate(&mut self, lives: &[u64], history: &[u64], now: u64) -> Result<(), String> {
        self.require_control(now)?;
        if self.owner.is_none() {
            return Err("there is no current ruler".into());
        }
        let mut unique = HashSet::new();
        if lives.is_empty()
            || lives
                .iter()
                .any(|id| !unique.insert(*id) || !history.contains(id))
        {
            return Err("unknown or duplicate past life".into());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("invocation revision overflow")?;
        self.active = lives.to_vec();
        self.active.sort_unstable();
        self.revision = revision;
        self.pending = None;
        self.strain_accepted_until = None;
        self.lifespan_accepted_until = None;
        self.loss_approved = false;
        Ok(())
    }
    pub fn dismiss(&mut self, now: u64) -> Result<(), String> {
        self.require_control(now)?;
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("invocation revision overflow")?;
        self.active.clear();
        self.pending = None;
        self.strain_accepted_until = None;
        self.lifespan_accepted_until = None;
        self.loss_approved = false;
        Ok(())
    }
    pub fn require_control(&self, now: u64) -> Result<(), String> {
        if self.control_until.is_some_and(|until| now < until) {
            Err("player control is suspended".into())
        } else {
            Ok(())
        }
    }
    fn rate(&self) -> Result<u64, String> {
        let count = u64::try_from(self.active.len()).map_err(|_| "invocation rate overflow")?;
        count
            .checked_mul(count)
            .and_then(|n| n.checked_mul(self.rules.rate_per_life))
            .ok_or_else(|| "invocation rate overflow".into())
    }
    /// Whether the next prepared interval starts an already accepted episode.
    pub fn will_lose_control(&self) -> bool {
        self.loss_approved
            && !self.loss_triggered
            && self
                .rate()
                .ok()
                .and_then(|rate| self.burden.checked_add(rate))
                .is_some_and(|burden| burden >= self.rules.loss_threshold)
    }
    /// Creates a persisted warning before an unaccepted cost, without advancing time.
    pub fn prepare(&mut self, now: u64) -> Result<bool, String> {
        if self.pending.is_some() {
            return Ok(false);
        }
        let next = now.checked_add(1).ok_or("tick overflow")?;
        let rate = self.rate()?;
        let burden = self
            .burden
            .checked_add(rate)
            .ok_or("invocation burden overflow")?;
        let autonomous = self.control_until.is_some_and(|until| now < until);
        let warning = if !autonomous
            && !self.active.is_empty()
            && burden >= self.rules.strain_threshold
            && self.strain_accepted_until.is_none_or(|until| next > until)
        {
            Some((WarningKind::Strain, self.rules.strain_consent_ticks))
        } else if !autonomous
            && !self.active.is_empty()
            && self.rules.lifespan_loss_per_tick > 0
            && self
                .exposure
                .checked_add(1)
                .ok_or("invocation exposure overflow")?
                >= self.rules.permanent_exposure_threshold
            && self
                .lifespan_accepted_until
                .is_none_or(|until| next > until)
        {
            Some((WarningKind::Lifespan, self.rules.strain_consent_ticks))
        } else if !autonomous
            && !self.active.is_empty()
            && burden >= self.rules.loss_threshold
            && !self.loss_triggered
            && !self.loss_approved
        {
            Some((WarningKind::ControlLoss, self.rules.control_loss_ticks))
        } else {
            None
        };
        if let Some((kind, duration)) = warning {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or("invocation revision overflow")?;
            self.pending = Some(Warning {
                revision: self.revision,
                owner: self.owner.ok_or("there is no current ruler")?,
                tick: now,
                rate,
                projected_burden: burden,
                kind,
                duration,
                strain_damage_per_tick: self.rules.strain_damage_per_tick,
                lifespan_loss_per_tick: self.rules.lifespan_loss_per_tick,
            });
            Ok(false)
        } else {
            Ok(true)
        }
    }
    /// Accept precisely the currently displayed warning, never a stale projection.
    pub fn accept(&mut self, revision: u64, now: u64) -> Result<(), String> {
        self.require_control(now)?;
        let warning = self
            .pending
            .as_ref()
            .filter(|w| w.revision == revision && w.tick == now)
            .ok_or("stale invocation warning")?;
        match warning.kind {
            WarningKind::Strain => {
                self.strain_accepted_until = Some(
                    now.checked_add(warning.duration)
                        .ok_or("consent deadline overflow")?,
                )
            }
            WarningKind::Lifespan => {
                self.lifespan_accepted_until = Some(
                    now.checked_add(warning.duration)
                        .ok_or("consent deadline overflow")?,
                )
            }
            WarningKind::ControlLoss => self.loss_approved = true,
        }
        self.pending = None;
        Ok(())
    }
    /// Called only on a staged copy after prepare succeeds.
    pub fn advance(&mut self, now: u64) -> Result<(), String> {
        if !self.prepare(now)? {
            return Err("invocation warning needs a choice".into());
        }
        let next = now.checked_add(1).ok_or("tick overflow")?;
        let rate = self.rate()?;
        if rate == 0 {
            self.burden = self.burden.saturating_sub(self.rules.recovery_per_tick);
        } else {
            self.burden = self
                .burden
                .checked_add(rate)
                .ok_or("invocation burden overflow")?;
            self.exposure = self
                .exposure
                .checked_add(1)
                .ok_or("invocation exposure overflow")?;
        }
        if self.loss_approved && !self.loss_triggered && self.burden >= self.rules.loss_threshold {
            self.control_until = Some(
                now.checked_add(self.rules.control_loss_ticks)
                    .ok_or("control deadline overflow")?,
            );
            self.loss_approved = false;
            self.loss_triggered = true;
        }
        if self.control_until.is_some_and(|until| next >= until) {
            self.control_until = None;
            self.loss_triggered = false;
        }
        if self.burden < self.rules.loss_threshold {
            self.loss_triggered = false;
        }
        Ok(())
    }
    pub fn bind(&mut self, owner: Option<u64>) {
        self.owner = owner;
        self.active.clear();
        self.pending = None;
        self.burden = 0;
        self.exposure = 0;
        self.strain_accepted_until = None;
        self.lifespan_accepted_until = None;
        self.loss_approved = false;
        self.loss_triggered = false;
        self.control_until = None;
        self.revision = self.revision.saturating_add(1);
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod integration_tests;
