//! Elementary quasi-static mechanisms and compatible linear/rotary work ports.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Linear,
    Rotary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Mechanism {
    Lever,
    Gear,
    Pulley,
    WheelAndAxle,
    InclinedPlane,
    Wedge,
    Screw,
    Crank,
    Drum,
}
impl Mechanism {
    pub const ALL: [Self; 9] = [
        Self::Lever,
        Self::Gear,
        Self::Pulley,
        Self::WheelAndAxle,
        Self::InclinedPlane,
        Self::Wedge,
        Self::Screw,
        Self::Crank,
        Self::Drum,
    ];
    pub fn input(self) -> Motion {
        match self {
            Self::Gear | Self::Screw | Self::Drum => Motion::Rotary,
            _ => Motion::Linear,
        }
    }
    pub fn output(self) -> Motion {
        match self {
            Self::Gear | Self::Crank => Motion::Rotary,
            _ => Motion::Linear,
        }
    }
    /// Ratios act on generalized effort and travel; rotary units are normalized torque/angle.
    /// Gear dimensions are driving/driven teeth, unlike lever input/output arms.
    pub fn ratio(self, drive: u32, load: u32, span: u32) -> Option<(u32, u32)> {
        if drive == 0 || load == 0 || drive.checked_add(load)? > span {
            return None;
        }
        match self {
            Self::Gear if drive >= 2 && load >= 2 => Some((load, drive)),
            Self::Gear => None,
            Self::Pulley | Self::Crank if load != 1 => None,
            Self::Drum if drive != 1 => None,
            Self::Screw if drive != 6 => None,
            Self::InclinedPlane | Self::Wedge if drive < load => None,
            _ => Some((drive, load)),
        }
    }
    pub fn geometry(self, draw: u64, span: u32) -> (u32, u32) {
        if span < 2 {
            return (0, 0);
        }
        let a = 1 + draw as u32 % (span - 1);
        let b = 1 + (draw >> 32) as u32 % (span - a);
        match self {
            Self::Pulley | Self::Crank => (a, 1),
            Self::Drum => (1, a),
            Self::Screw => (6, 1 + draw as u32 % span.saturating_sub(6).max(1)),
            Self::InclinedPlane | Self::Wedge => (a.max(b), a.min(b)),
            Self::Gear => (2, 2 + (draw as u32 % span.saturating_sub(3).max(1))),
            _ => (a, b),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mechanisms_have_real_geometry_and_compatible_motion_ports() {
        assert_eq!(Mechanism::Gear.ratio(4, 12, 20), Some((12, 4)));
        assert_eq!(Mechanism::Pulley.ratio(4, 1, 8), Some((4, 1)));
        assert_eq!(Mechanism::Pulley.ratio(4, 2, 8), None);
        assert_eq!(Mechanism::Crank.output(), Mechanism::Gear.input());
        assert_eq!(Mechanism::Gear.output(), Mechanism::Drum.input());
        assert_ne!(Mechanism::Gear.output(), Mechanism::Lever.input());
    }
}
