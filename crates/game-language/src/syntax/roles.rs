//! Role particles come from individual choices during grounded utterances.
use super::{Holdings, Order};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Holder,
    Object,
}

impl Role {
    fn other(self) -> Self {
        match self {
            Self::Holder => Self::Object,
            Self::Object => Self::Holder,
        }
    }

    pub(super) fn order_at(self, noun: u8) -> Order {
        if matches!((self, noun), (Self::Holder, 0) | (Self::Object, 1)) {
            Order::HolderFirst
        } else {
            Order::ObjectFirst
        }
    }

    pub(super) fn from_order(order: Order, noun: u8) -> Self {
        if matches!(
            (order, noun),
            (Order::HolderFirst, 0) | (Order::ObjectFirst, 1)
        ) {
            Self::Holder
        } else {
            Self::Object
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub sound: u8,
    pub role: Role,
}

impl Marker {
    fn valid(self, sounds: usize) -> bool {
        usize::from(self.sound) < sounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Example {
    marker: Marker,
    scene: Holdings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    #[serde(default)]
    pub initiative: u16,
    #[serde(default)]
    pub inference: u16,
    /// Personal willingness to change an already adopted convention.
    #[serde(default)]
    pub revision: u16,
    #[serde(default)]
    pub reinterpret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_role: Option<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<Marker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    example: Option<Example>,
}

impl Mind {
    pub(super) fn valid(&self, sounds: usize) -> bool {
        self.initiative <= 1000
            && self.inference <= 1000
            && self.revision <= 1000
            && self.marker.is_none_or(|row| row.valid(sounds))
            && self
                .example
                .is_none_or(|example| example.marker.valid(sounds))
    }

    pub(super) fn knows_examples(&self, people: &HashSet<u64>) -> bool {
        self.example
            .is_none_or(|example| example.scene.valid(people))
    }

    pub(super) fn speak(
        &mut self,
        chance: u64,
        form: u64,
        phonemes: &[u8],
        sounds: usize,
    ) -> Option<Marker> {
        if chance % 1000 >= u64::from(self.initiative) {
            return None;
        }
        if let Some(marker) = self.marker {
            return (phonemes.is_empty() || phonemes.binary_search(&marker.sound).is_ok())
                .then_some(marker);
        }
        let available = if phonemes.is_empty() {
            sounds
        } else {
            phonemes.len()
        };
        let slot = form.checked_rem(available as u64)? as usize;
        let sound = if phonemes.is_empty() {
            u8::try_from(slot).ok()?
        } else {
            phonemes[slot]
        };
        let role = self.preferred_role.unwrap_or_else(|| {
            if form.rotate_left(23) & 1 == 0 {
                Role::Holder
            } else {
                Role::Object
            }
        });
        let marker = Marker { sound, role };
        self.marker = Some(marker);
        self.example = None;
        Some(marker)
    }

    /// Consider a sound heard alongside terms whose roles are personally understood.
    pub(super) fn consider(
        &mut self,
        marker: Marker,
        scene: Holdings,
        chance: u64,
    ) -> Option<Marker> {
        let choice = Marker {
            role: if self.reinterpret {
                marker.role.other()
            } else {
                marker.role
            },
            ..marker
        };
        if chance % 1000 >= u64::from(self.inference)
            || self.marker.is_some_and(|known| {
                known == choice || chance.rotate_left(31) % 1000 >= u64::from(self.revision)
            })
        {
            return None;
        }
        if !self
            .example
            .is_some_and(|example| example.marker == marker && example.scene != scene)
        {
            self.example = Some(Example { marker, scene });
            return None;
        }
        self.marker = Some(choice);
        self.example = None;
        Some(choice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Use {
    pub sound: u8,
    pub noun: u8,
    pub intended_role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted: Option<Marker>,
}

impl Use {
    pub(super) fn valid(self, heard: bool, demonstrated: bool, sounds: usize) -> bool {
        self.noun < 2
            && usize::from(self.sound) < sounds
            && self
                .adopted
                .is_none_or(|marker| heard && demonstrated && marker.sound == self.sound)
    }
}
