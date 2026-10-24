//! Explicit attention overrides; existing saves keep their stored policy by default.
use crate::Result;
use game_characters::{AttentionRates, ConfigureAttention, ConfigureBusyAttention, Minds};
use game_core::*;
use game_sim::Host;

#[derive(Default)]
pub(super) struct Overrides {
    ordinary: Option<SimDuration>,
    evidence: Option<SimDuration>,
    busy: Option<bool>,
}
impl Overrides {
    pub fn configured(&self) -> bool {
        self.ordinary.is_some() || self.evidence.is_some() || self.busy.is_some()
    }
    pub fn busy(&mut self, value: &str) -> Result<()> {
        if self.busy.is_some() {
            return Err("busy attention specified more than once".into());
        }
        self.busy = Some(value.parse()?);
        Ok(())
    }
    pub fn ordinary(&mut self, value: &str, unit: u64) -> Result<()> {
        set(&mut self.ordinary, value, unit)
    }
    pub fn evidence(&mut self, value: &str, unit: u64) -> Result<()> {
        set(&mut self.evidence, value, unit)
    }
    fn policy(&self, previous: Option<AttentionRates>) -> Result<Option<AttentionRates>> {
        if self.ordinary.is_none() && self.evidence.is_none() {
            return Ok(previous);
        }
        if self.ordinary == Some(SimDuration(0)) {
            if self.evidence.is_some() {
                return Err("evidence rate requires enabled attention".into());
            }
            return Ok(None);
        }
        let ordinary = self
            .ordinary
            .or(previous.map(|p| p.ordinary))
            .ok_or("evidence rate requires enabled attention")?;
        let evidence = self
            .evidence
            .unwrap_or_else(|| previous.map_or(ordinary, |p| p.evidence.min(ordinary)));
        let policy = AttentionRates { ordinary, evidence };
        policy.validate()?;
        Ok(Some(policy))
    }
    pub fn apply(&self, host: &mut Host) -> Result<()> {
        if !self.configured() {
            return Ok(());
        }
        let previous = host.get::<Minds>()?.rules.attention;
        let policy = self.policy(previous)?;
        if previous != policy {
            host.dispatch(ConfigureAttention(policy))?;
        }
        if let Some(busy) = self.busy
            && host.get::<Minds>()?.rules.busy_attention != busy
        {
            host.dispatch(ConfigureBusyAttention(busy))?;
        }
        Ok(())
    }
}
fn set(slot: &mut Option<SimDuration>, value: &str, unit: u64) -> Result<()> {
    if slot.is_some() {
        return Err("attention interval specified more than once; choose one unit".into());
    }
    *slot = Some(SimDuration(
        value
            .parse::<u64>()?
            .checked_mul(unit)
            .ok_or("attention interval overflow")?,
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_bounds_and_disabled_or_preserved_saved_policy() {
        let mut minutes = Overrides::default();
        minutes.ordinary("1", MINUTE.0).unwrap();
        let mut millis = Overrides::default();
        millis.ordinary("60000", 1).unwrap();
        let saved = minutes.policy(None).unwrap();
        assert_eq!(saved, millis.policy(None).unwrap());
        assert_eq!(Overrides::default().policy(saved).unwrap(), saved);
        assert_eq!(Overrides::default().policy(None).unwrap(), None);
        assert!(millis.ordinary("1", 1).is_err());
        let mut off = Overrides::default();
        off.ordinary("0", 1).unwrap();
        assert_eq!(off.policy(saved).unwrap(), None);
        off.evidence("1", 1).unwrap();
        assert!(off.policy(saved).is_err());
        let mut evidence = Overrides::default();
        evidence.evidence("1", 1).unwrap();
        assert!(evidence.policy(None).is_err());
        assert_eq!(
            evidence.policy(saved).unwrap().unwrap().evidence,
            SimDuration(1)
        );
        let mut invalid = Overrides::default();
        assert!(invalid.ordinary(&u64::MAX.to_string(), MINUTE.0).is_err());
        invalid.ordinary("86400001", 1).unwrap();
        assert!(invalid.policy(None).is_err());
    }
}
