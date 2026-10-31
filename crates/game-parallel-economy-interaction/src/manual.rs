//! Direct personal physical actions staged with the rest of the world.
use game_parallel_economy_person::Person;
use game_parallel_lots::{Movement, Place, Transaction, View};

#[derive(Clone, Copy)]
pub struct Transfer {
    pub person: u64,
    pub good: u32,
    pub quantity: u64,
    pub taking: bool,
}
#[derive(Clone, Copy)]
pub enum PhysicalChoice {
    Stock(Transfer),
    Recover {
        person: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
    },
    Promise {
        person: u64,
        contract: u64,
    },
    Treat {
        person: u64,
    },
    Farm {
        person: u64,
        field: u64,
    },
    Build {
        person: u64,
        blueprint: u32,
    },
    DestroySite {
        person: u64,
        site: u64,
    },
    SalvageSite {
        person: u64,
        site: u64,
        good: u32,
        quantity: u64,
    },
    Gather {
        person: u64,
        node: u64,
    },
    Give {
        person: u64,
        recipient: u64,
        good: u32,
        quantity: u64,
    },
    AskFood {
        person: u64,
        target: u64,
        lifetime: u64,
    },
    AskHaul {
        person: u64,
        carrier: u64,
        source: u64,
        destination: u64,
        good: u32,
        lifetime: u64,
    },
    TreatOther {
        person: u64,
        patient: u64,
        medicine: u32,
        quantity: u64,
    },
    Rest {
        person: u64,
    },
}
impl PhysicalChoice {
    pub fn person(self) -> u64 {
        match self {
            Self::Stock(choice) => choice.person,
            Self::Recover { person, .. }
            | Self::Promise { person, .. }
            | Self::Treat { person }
            | Self::Farm { person, .. }
            | Self::Build { person, .. }
            | Self::DestroySite { person, .. }
            | Self::SalvageSite { person, .. }
            | Self::Gather { person, .. }
            | Self::Give { person, .. }
            | Self::AskFood { person, .. }
            | Self::AskHaul { person, .. }
            | Self::TreatOther { person, .. }
            | Self::Rest { person } => person,
        }
    }
}
/// A dead person's possessions remain physical and can be recovered locally.
pub fn recover(
    choice: Option<(u64, u64, u32, u64)>,
    people: &[Person],
    view: &View,
    id: u64,
) -> Result<Option<Transaction>, String> {
    let Some((person, deceased, good, quantity)) = choice else {
        return Ok(None);
    };
    let actor = people
        .binary_search_by_key(&person, |p| p.id)
        .ok()
        .map(|i| &people[i])
        .ok_or("unknown person")?;
    let body = people
        .binary_search_by_key(&deceased, |p| p.id)
        .ok()
        .map(|i| &people[i])
        .ok_or("unknown deceased person")?;
    if !actor.controlled
        || actor.health == 0
        || actor.region.is_none()
        || actor.region != body.region
        || body.health != 0
        || quantity == 0
        || view.container(Place::Person(person)).is_none()
        || view.container(Place::Person(deceased)).is_none()
    {
        return Err(
            "recovery requires a living controlled person and a local deceased person".into(),
        );
    }
    Ok(Some(Transaction {
        id,
        movements: vec![Movement {
            id,
            from: Place::Person(deceased),
            to: Some(Place::Person(person)),
            good,
            quantity,
        }],
        creations: vec![],
    }))
}
pub fn transfer(
    choice: Option<Transfer>,
    people: &[Person],
    view: &View,
    id: u64,
) -> Result<Option<Transaction>, String> {
    let Some(choice) = choice else {
        return Ok(None);
    };
    let person = people
        .binary_search_by_key(&choice.person, |p| p.id)
        .ok()
        .map(|i| &people[i])
        .ok_or("unknown person")?;
    let region = person.region.ok_or("person is traveling")?;
    if !person.controlled || person.health == 0 || choice.quantity == 0 {
        return Err("transfer requires a living controlled person and positive quantity".into());
    }
    let stock = Place::Stock {
        region,
        good: choice.good,
    };
    let holder = Place::Person(choice.person);
    if view.container(stock).is_none() || view.container(holder).is_none() {
        return Err("unknown local stock".into());
    }
    let (from, to) = if choice.taking {
        (stock, holder)
    } else {
        (holder, stock)
    };
    Ok(Some(Transaction {
        id,
        movements: vec![Movement {
            id,
            from,
            to: Some(to),
            good: choice.good,
            quantity: choice.quantity,
        }],
        creations: vec![],
    }))
}

#[macro_export]
macro_rules! meal_method {
    () => {
        /// Consume one real portion and advance a shared boundary, even if it is unavailable.
        pub fn eat(&mut self, person: u64, good: u32) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                Some(game_parallel_economy_person::meal::Choice { person, good }),
                None,
            )?;
            let events = self.commit(draft)?;
            Ok(events.physical_movements.iter().any(|r| {
                r.request.from == game_parallel_lots::Place::Person(person)
                    && r.request.to.is_none()
                    && r.request.good == good
                    && r.movement.accepted
            }))
        }
    };
}
#[macro_export]
macro_rules! transfer_method {
    () => {
        /// Attempt to move goods between the controlled person and local stock.
        pub fn transfer_stock(
            &mut self,
            person: u64,
            good: u32,
            quantity: u64,
            taking: bool,
        ) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let choice = $crate::Transfer {
                person,
                good,
                quantity,
                taking,
            };
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some($crate::PhysicalChoice::Stock(choice)),
            )?;
            let events = self.commit(draft)?;
            Ok(events.physical_movements.iter().any(|r| {
                r.request.good == good
                    && r.request.quantity == quantity
                    && r.movement.accepted
                    && if taking {
                        r.request.to == Some(game_parallel_lots::Place::Person(person))
                            && matches!(r.request.from, game_parallel_lots::Place::Stock { .. })
                    } else {
                        r.request.from == game_parallel_lots::Place::Person(person)
                            && matches!(r.request.to, Some(game_parallel_lots::Place::Stock { .. }))
                    }
            }))
        }
        /// Attempt to recover possessions from a local deceased person.
        pub fn recover_possessions(
            &mut self,
            person: u64,
            deceased: u64,
            good: u32,
            quantity: u64,
        ) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some($crate::PhysicalChoice::Recover {
                    person,
                    deceased,
                    good,
                    quantity,
                }),
            )?;
            let events = self.commit(draft)?;
            Ok(events.physical_movements.iter().any(|r| {
                r.request.from == game_parallel_lots::Place::Person(deceased)
                    && r.request.to == Some(game_parallel_lots::Place::Person(person))
                    && r.request.good == good
                    && r.request.quantity == quantity
                    && r.movement.accepted
            }))
        }
    };
}
#[macro_export]
macro_rules! fulfill_method {
    () => {
        /// Choose to fulfill a personal promise with a physical transfer.
        pub fn fulfill_promise(&mut self, person: u64, contract: u64) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some($crate::PhysicalChoice::Promise { person, contract }),
            )?;
            self.commit(draft)?;
            Ok(self.agreements.as_ref().is_some_and(|a| {
                a.contracts.iter().any(|c| {
                    c.id == contract && c.status == game_parallel_agreements::Status::Fulfilled
                })
            }))
        }
    };
}
#[macro_export]
macro_rules! treat_method {
    () => {
        /// Use this person's chosen medicine through actual physical custody.
        pub fn treat(&mut self, person: u64) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some($crate::PhysicalChoice::Treat { person }),
            )?;
            self.commit(draft)?;
            Ok(game_parallel_economy_dynamics::treatment::treated(
                self.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
                person,
            ))
        }
    };
}
