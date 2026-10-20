use super::*;
/// A revocable personal intention; it grants no authority over the beneficiary.
#[derive(Clone, Serialize, Deserialize)]
pub struct CommitCare {
    pub caregiver: PersonId,
    pub beneficiary: PersonId,
    pub accept: bool,
}
impl Request for CommitCare {
    const KEY: &'static str = "households.commit-care";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CareChanged {
    pub caregiver: PersonId,
    pub beneficiary: PersonId,
    pub accept: bool,
}
impl Event for CareChanged {
    const KEY: &'static str = "households.care-changed";
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<CommitCare>(|tx, r| {
        require(r.caregiver != r.beneficiary, "self care commitment")?;
        if r.accept {
            game_physiology::capable(tx, r.caregiver)?;
            game_physiology::capable(tx, r.beneficiary)?;
            require(
                game_vitality::can_manipulate(tx, r.caregiver),
                "cannot provide physical care",
            )?;
            require(
                game_transport::area(tx.get()?, r.caregiver)?
                    == game_transport::area(tx.get()?, r.beneficiary)?,
                "care needs contact",
            )?;
        }
        if tx
            .get::<Households>()?
            .committed(r.caregiver, r.beneficiary)
            == r.accept
        {
            return Ok(());
        }
        tx.edit::<Households>()?
            .set_care(r.caregiver, r.beneficiary, r.accept);
        support(tx, r.caregiver)?;
        tx.emit(
            CareChanged {
                caregiver: r.caregiver,
                beneficiary: r.beneficiary,
                accept: r.accept,
            },
            vec![r.caregiver, r.beneficiary],
        )
    })
}
