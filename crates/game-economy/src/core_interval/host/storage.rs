//! Weather-driven storage conditions and their elapsed deadlines.
use super::*;

impl CoreHost {
    pub(super) fn next_storage_temperature(&self) -> Option<u64> {
        let environment = self.environment.as_ref()?;
        let now = self.core.simulated_millis();
        let celsius = environment.seasonal_storage_at(now)?;
        if self.core.physical.ambient_storage_celsius() != Some(celsius) {
            return Some(0);
        }
        u64::try_from(environment.next_season_change(now)?).ok()
    }

    /// Install the new season after lots due at the closing temperature have published.
    pub fn publish_storage_temperature(&mut self) -> Result<Option<i16>, String> {
        let now = self.core.simulated_millis();
        let Some(celsius) = self
            .environment
            .as_ref()
            .and_then(|environment| environment.seasonal_storage_at(now))
        else {
            return Ok(None);
        };
        self.core
            .physical
            .change_ambient_temperature(celsius)
            .map(|changed| changed.then_some(celsius))
    }
}
