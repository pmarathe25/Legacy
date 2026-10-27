//! Travel owns position, arrival, and interruption. No work is credited in transit.
mod arrivals;
mod timing;
use game_core::*;
use game_packed::Packed;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
pub use timing::{ConfigureArrivalWindow, arrival_window};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Riders {
    #[serde(default, skip_serializing_if = "timing::exact")]
    arrival_window: SimDuration,
    moving: Table<PersonId, PersonId>,
    departures: Table<PersonId, Location>,
}
impl Resource for Riders {
    const KEY: &'static str = "transport.riders";
    fn validate(&self) -> Result<()> {
        require(
            self.arrival_window.0 <= DAY.0,
            "arrival window exceeds one day",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    At(AreaId),
    Transit {
        from: AreaId,
        to: AreaId,
        departed: SimTime,
        arrives: SimTime,
        job: ScheduleId,
    },
    Road {
        from: AreaId,
        to: AreaId,
        elapsed: SimDuration,
        duration: SimDuration,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Positions(pub Packed<PersonId, Location>);
impl Resource for Positions {
    const KEY: &'static str = "transport.positions";
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new(self.0.begin())
    }
    fn restore(&mut self, c: Checkpoint) {
        self.0.rollback(
            *c.downcast::<game_packed::Mark>()
                .expect("position checkpoint"),
        );
    }
    fn commit(&mut self, c: Checkpoint) {
        self.0.commit(
            *c.downcast::<game_packed::Mark>()
                .expect("position checkpoint"),
        );
    }
    fn validate_checkpoint(&self, _: &dyn std::any::Any) -> Result<()> {
        Ok(())
    }
}
pub fn area(positions: &Positions, person: PersonId) -> Result<AreaId> {
    match positions.0.get(&person) {
        Some(Location::At(a)) => Ok(*a),
        _ => Err(Error::Rejected("actor is not at an area".into())),
    }
}
/// Approximate the surrounding region along a road without treating it as a local area.
pub fn surrounding(positions: &Positions, person: PersonId, at: SimTime) -> Option<AreaId> {
    match positions.0.get(&person)? {
        Location::At(area) => Some(*area),
        Location::Transit {
            from,
            to,
            departed,
            arrives,
            ..
        } => Some(
            if u128::from(at.0.saturating_sub(departed.0)) * 2
                >= u128::from(arrives.0.saturating_sub(departed.0))
            {
                *to
            } else {
                *from
            },
        ),
        Location::Road {
            from,
            to,
            elapsed,
            duration,
        } => Some(if u128::from(elapsed.0) * 2 >= u128::from(duration.0) {
            *to
        } else {
            *from
        }),
    }
}
/// Roads describe a baseline traversal; current expressed speed determines actual time.
pub fn travel_time(tx: &Transaction<'_>, person: PersonId, baseline: SimDuration) -> SimDuration {
    let pace = game_vitality::factor(game_vitality::ability(tx, person, "speed"));
    SimDuration(
        (u128::from(baseline.0) * 1000 / u128::from(pace))
            .min(u128::from(u64::MAX))
            .max(u128::from(baseline.0 > 0)) as u64,
    )
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Place {
    pub person: PersonId,
    pub area: AreaId,
}
impl Request for Place {
    const KEY: &'static str = "transport.place";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Move {
    pub person: PersonId,
    pub destination: AreaId,
}
/// A colocated passenger travels with a carrier, even before independent locomotion.
#[derive(Clone, Serialize, Deserialize)]
pub struct Carry {
    pub carrier: PersonId,
    pub passenger: PersonId,
    pub destination: AreaId,
}
impl Request for Carry {
    const KEY: &'static str = "transport.carry";
    type Response = SimTime;
}
impl Request for Move {
    const KEY: &'static str = "transport.move";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Arrive {
    person: PersonId,
    destination: AreaId,
}
impl Request for Arrive {
    const KEY: &'static str = "transport.arrive";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Cancel {
    pub person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "transport.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Arrived {
    pub person: PersonId,
    pub area: AreaId,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Departed {
    pub person: PersonId,
    pub destination: AreaId,
}
impl Event for Departed {
    const KEY: &'static str = "transport.departed";
}
impl Event for Arrived {
    const KEY: &'static str = "transport.arrived";
}
/// Simultaneous travellers at the same physical point can perceive one another.
pub fn together(positions: &Positions, a: PersonId, b: PersonId) -> bool {
    match (positions.0.get(&a), positions.0.get(&b)) {
        (Some(Location::At(a)), Some(Location::At(b))) => a == b,
        (
            Some(Location::Transit {
                from: a,
                to: b,
                departed: c,
                arrives: d,
                ..
            }),
            Some(Location::Transit {
                from: w,
                to: x,
                departed: y,
                arrives: z,
                ..
            }),
        ) => (a, b, c, d) == (w, x, y, z),
        (
            Some(Location::Road {
                from: a,
                to: b,
                elapsed: c,
                duration: d,
            }),
            Some(Location::Road {
                from: w,
                to: x,
                elapsed: y,
                duration: z,
            }),
        ) => (a, b, c, d) == (w, x, y, z),
        _ => false,
    }
}
pub fn carrier(tx: &Transaction<'_>, person: PersonId) -> Option<PersonId> {
    tx.get::<Riders>().ok()?.moving.get(&person).copied()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CarriedMass {
    pub person: PersonId,
}
impl Request for CarriedMass {
    const KEY: &'static str = "transport.carried-mass";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Dismount {
    pub person: PersonId,
}
impl Request for Dismount {
    const KEY: &'static str = "transport.dismount";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("transport", 2)?;
    b.resource::<Positions>()?;
    b.resource::<Riders>()?;
    b.request::<ConfigureArrivalWindow>(|tx, r| {
        tx.edit::<Riders>()?.arrival_window = r.0;
        Ok(())
    })?;
    b.request::<Dismount>(|tx, r| {
        require(
            carrier(tx, r.person).is_some() && game_vitality::can_walk(tx, r.person),
            "cannot dismount independently",
        )?;
        tx.call(Cancel { person: r.person })
    })?;
    b.validate_save(|h| {
        let riders = h.get::<Riders>()?;
        let positions = h.get::<Positions>()?;
        require(riders.departures.is_empty(), "unfinished departure")?;
        require(
            riders.moving.iter().all(|(p, guide)| {
                if p == guide || riders.moving.contains_key(guide) {
                    return false;
                }
                let Some(Location::Transit {
                    from,
                    to,
                    departed,
                    arrives,
                    ..
                }) = positions.0.get(p)
                else {
                    return false;
                };
                match positions.0.get(guide) {
                    Some(Location::Transit {
                        from: a,
                        to: b,
                        departed: c,
                        arrives: d,
                        ..
                    }) => (from, to, departed, arrives) == (a, b, c, d),
                    Some(Location::At(area)) => area == to && *arrives == h.now(),
                    _ => false,
                }
            }),
            "passenger has no matching carrier journey",
        )
    });
    b.request::<Place>(|tx, r| {
        require(
            tx.get::<game_land::Land>()?.0.contains_key(&r.area),
            "unknown area",
        )?;
        require(
            !tx.get::<Positions>()?.0.contains_key(&r.person),
            "actor already placed",
        )?;
        tx.edit::<Positions>()?
            .0
            .insert(r.person, Location::At(r.area));
        Ok(())
    })?;
    b.request::<Move>(|tx, r| {
        game_contracts::causal::point("transport.move", || {
            serde_json::json!({"person": r.person, "destination": r.destination, "at": tx.now(), "revision": tx.revision_after()})
        });
        require(
            game_vitality::can_walk(tx, r.person),
            "cannot yet walk independently",
        )?;
        let location = tx
            .get::<Positions>()?
            .0
            .get(&r.person)
            .cloned()
            .ok_or_else(|| Error::Missing("position".into()))?;
        let (from, duration) = match location.clone() {
            Location::At(from) => {
                require(from != r.destination, "already there")?;
                let duration = tx
                    .get::<game_land::Land>()?
                    .0
                    .get(&from)
                    .and_then(|area| area.links.get(&r.destination))
                    .copied()
                    .ok_or_else(|| Error::Rejected("no direct road".into()))?;
                (from, travel_time(tx, r.person, duration))
            }
            Location::Road {
                from,
                to,
                elapsed,
                duration,
            } => {
                require(
                    r.destination == from || r.destination == to,
                    "choose a road endpoint",
                )?;
                (
                    if r.destination == to { from } else { to },
                    SimDuration(if r.destination == to {
                        duration.0 - elapsed.0
                    } else {
                        elapsed.0
                    }),
                )
            }
            _ => return Err(Error::Rejected("already travelling".into())),
        };
        tx.edit::<Riders>()?.departures.insert(r.person, location);
        let arrives = begin(tx, r.person, from, r.destination, duration, true)?;
        tx.emit(
            Departed {
                person: r.person,
                destination: r.destination,
            },
            vec![r.person],
        )?;
        tx.edit::<Riders>()?.departures.remove(&r.person);
        Ok(arrives)
    })?;
    b.request::<Carry>(|tx, r| {
        require(
            r.carrier != r.passenger && game_vitality::can_walk(tx, r.carrier),
            "invalid carrier",
        )?;
        let Some(Location::Transit {
            from,
            to,
            departed,
            arrives,
            ..
        }) = tx.get::<Positions>()?.0.get(&r.carrier).cloned()
        else {
            return Err(Error::Rejected("carrier is not departing".into()));
        };
        require(
            to == r.destination
                && departed == tx.now()
                && tx.get::<Riders>()?.departures.get(&r.carrier)
                    == tx.get::<Positions>()?.0.get(&r.passenger)
                && tx.get::<Riders>()?.departures.contains_key(&r.carrier),
            "passenger is not at departure",
        )?;
        let load = if tx.registered::<CarriedMass>() {
            tx.call(CarriedMass { person: r.carrier })?
        } else {
            0
        };
        let passengers = tx
            .get::<Riders>()?
            .moving
            .iter()
            .filter(|(_, p)| **p == r.carrier)
            .map(|(p, _)| game_vitality::ability(tx, *p, "size"))
            .sum::<u64>();
        let capacity = game_vitality::factor(game_vitality::ability(tx, r.carrier, "strength"))
            .saturating_mul(2)
            .saturating_add(game_vitality::factor(game_vitality::ability(
                tx,
                r.carrier,
                "endurance",
            )));
        require(
            load.saturating_add(passengers)
                .saturating_add(game_vitality::ability(tx, r.passenger, "size"))
                <= capacity,
            "passenger exceeds carrying capacity",
        )?;
        if tx
            .get::<Actions>()?
            .0
            .get(&r.passenger)
            .is_some_and(|a| !["rest", "express-need"].contains(&a.kind.as_str()))
        {
            tx.cancel_action(r.passenger)?;
        }
        let duration = SimDuration(arrives.0.saturating_sub(tx.now().0));
        let arrives = begin(tx, r.passenger, from, r.destination, duration, false)?;
        tx.edit::<Riders>()?.moving.insert(r.passenger, r.carrier);
        Ok(arrives)
    })?;
    b.request::<Arrive>(arrivals::one)?;
    b.scheduled_wave::<Arrive>(16384, arrivals::wave)?;
    b.request::<Cancel>(|tx, r| {
        tx.edit::<Riders>()?.moving.remove(&r.person);
        let passengers: Vec<_> = tx
            .get::<Riders>()?
            .moving
            .iter()
            .filter(|(_, guide)| **guide == r.person)
            .map(|(p, _)| *p)
            .collect();
        for passenger in passengers {
            tx.call(Cancel { person: passenger })?;
        }
        if let Some(Location::Transit {
            from,
            to,
            departed,
            arrives,
            job,
        }) = tx.get::<Positions>()?.0.get(&r.person).cloned()
        {
            tx.unschedule(job);
            let elapsed = SimDuration(tx.now().0 - departed.0);
            let duration = SimDuration(arrives.0 - departed.0);
            tx.edit::<Positions>()?.0.insert(
                r.person,
                Location::Road {
                    from,
                    to,
                    elapsed,
                    duration,
                },
            );
        }
        if tx
            .get::<Actions>()?
            .0
            .get(&r.person)
            .is_some_and(|a| a.kind == "travel")
        {
            tx.finish_action(r.person, "travel")?;
        }
        Ok(())
    })
}
fn begin(
    tx: &mut Transaction<'_>,
    person: PersonId,
    from: AreaId,
    destination: AreaId,
    duration: SimDuration,
    active: bool,
) -> Result<SimTime> {
    let now = tx.now();
    let window = tx.get::<Riders>()?.arrival_window;
    let arrives = timing::deadline(now, duration, if active { window } else { SimDuration(0) })?;
    if active {
        tx.start_action(person, "travel", Some(arrives), Cancel { person })?;
    }
    let job = tx.schedule(
        arrives,
        if window.0 > 0 { 9 } else { 10 },
        Arrive {
            person,
            destination,
        },
    )?;
    tx.edit::<Positions>()?.0.insert(
        person,
        Location::Transit {
            from,
            to: destination,
            departed: now,
            arrives,
            job,
        },
    );
    Ok(arrives)
}
