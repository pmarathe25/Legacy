//! Saved travel cadence. Added time is part of traversal, including interruption.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureArrivalWindow(pub SimDuration);
impl Request for ConfigureArrivalWindow {
    const KEY: &'static str = "transport.configure-arrival-window";
    type Response = ();
}
pub fn arrival_window(host: &Host) -> Result<SimDuration> {
    Ok(host.get::<Riders>()?.arrival_window)
}
pub(super) fn exact(window: &SimDuration) -> bool {
    window.0 == 0
}
pub(super) fn deadline(
    now: SimTime,
    duration: SimDuration,
    window: SimDuration,
) -> Result<SimTime> {
    let at = now.after(duration)?;
    if duration.0 == 0 || window.0 == 0 {
        return Ok(at);
    }
    let remainder = at.0 % window.0;
    at.after(SimDuration(if remainder == 0 {
        0
    } else {
        window.0 - remainder
    }))
}
