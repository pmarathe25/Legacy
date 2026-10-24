//! Optional attempt telemetry. Numeric fingerprints are diagnostics, never cache keys.
#[cfg(feature = "causality")]
mod enabled;
#[cfg(feature = "causality")]
pub use enabled::*;
#[cfg(not(feature = "causality"))]
mod disabled {
    pub struct Scope;
    pub struct Review;
    #[inline]
    pub fn scope(_: &'static str, _: &str) -> Scope {
        Scope
    }
    impl Review {
        #[inline]
        pub fn begin(_: u64, _: u64, _: u64, _: impl FnOnce() -> serde_json::Value) -> Self {
            Self
        }
        #[inline]
        pub fn annotate(&self, _: &str, _: impl FnOnce() -> serde_json::Value) {}
    }
    #[inline]
    pub fn wake(_: u64, _: u64, _: bool, _: bool, _: bool) {}
    #[inline]
    pub fn candidates(_: u64, _: impl Iterator<Item = (u64, [i64; 5], Option<u64>)>) {}
}
#[cfg(not(feature = "causality"))]
pub use disabled::*;
