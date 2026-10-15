//! Travel owns position, arrival, and interruption. No work is credited in transit.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
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
pub struct Positions(pub Table<PersonId, Location>);
impl Resource for Positions {
    const KEY: &'static str = "transport.positions";
}
pub fn area(positions: &Positions, person: PersonId) -> Result<AreaId> {
    match positions.0.get(&person) {
        Some(Location::At(a)) => Ok(*a),
        _ => Err(Error::Rejected("actor is not at an area".into())),
    }
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
impl Event for Arrived {
    const KEY: &'static str = "transport.arrived";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("transport", 1)?;
    b.resource::<Positions>()?;
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
        let location = tx
            .get::<Positions>()?
            .0
            .get(&r.person)
            .cloned()
            .ok_or_else(|| Error::Missing("position".into()))?;
        let (from, duration) = match location {
            Location::At(from) => {
                require(from != r.destination, "already there")?;
                let duration = tx
                    .get::<game_land::Land>()?
                    .0
                    .get(&from)
                    .and_then(|area| area.links.get(&r.destination))
                    .copied()
                    .ok_or_else(|| Error::Rejected("no direct road".into()))?;
                (from, duration)
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
        let now = tx.now();
        let arrives = now.after(duration)?;
        tx.start_action(
            r.person,
            "travel",
            Some(arrives),
            Cancel { person: r.person },
        )?;
        let job = tx.schedule(
            arrives,
            10,
            Arrive {
                person: r.person,
                destination: r.destination,
            },
        )?;
        tx.edit::<Positions>()?.0.insert(
            r.person,
            Location::Transit {
                from,
                to: r.destination,
                departed: now,
                arrives,
                job,
            },
        );
        Ok(arrives)
    })?;
    b.request::<Arrive>(|tx, r| {
        tx.finish_action(r.person, "travel")?;
        tx.edit::<Positions>()?
            .0
            .insert(r.person, Location::At(r.destination));
        tx.emit(
            Arrived {
                person: r.person,
                area: r.destination,
            },
            vec![r.person],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
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
