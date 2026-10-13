//! Character-owned decisions and social development kernels. Physical execution,
//! simulation scheduling, and personal perception enter through explicit inputs.
//! Resource intentions use declarative requirements from game-logistics.
pub mod temperament;

pub mod social;

pub mod decisions;

mod planning;

pub mod proposals;

pub mod resource_responses;

pub mod independence;
pub use independence::independence_material_goal;

pub mod memories;

pub mod perception;

pub mod orchestration;

pub mod personality_provider;

pub mod people;
pub use people::{Person, insert_person, person, person_ids, person_mut, persons, register_people};

pub mod receivers;

pub mod views;

pub mod save_validation;

pub mod services;
