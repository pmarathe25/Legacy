use super::*;
#[derive(Clone, Serialize, Deserialize)]
struct Leave {
    person: PersonId,
    other: PersonId,
    destination: AreaId,
}
impl Request for Leave {
    const KEY: &'static str = "pressure.leave";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Leave>(|tx, r| {
        clear(tx, r.person)?;
        tx.call(game_transport::Move {
            person: r.person,
            destination: r.destination,
        })?;
        tx.emit(
            Fled {
                person: r.person,
                other: r.other,
                destination: r.destination,
            },
            vec![r.person, r.other],
        )
    })
}
pub(super) fn consider(
    tx: &mut Transaction<'_>,
    person: PersonId,
    other: PersonId,
) -> Result<bool> {
    let Some(destination) = judgment::escape(tx, person, other)? else {
        return Ok(false);
    };
    // A stale remembered road must not release the encounter or invent an escape.
    Ok(tx
        .attempt(Leave {
            person,
            other,
            destination,
        })
        .is_ok())
}
