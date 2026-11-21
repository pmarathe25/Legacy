//! Individual purpose follows actual outcomes and shares the physical lifecycle.
use crate::Families;
use game_purpose::Purpose;
use rayon::prelude::*;

/// Actual newborns become eligible at biological adulthood, when configured for births.
pub fn enroll(
    owner: Option<&mut Purpose>,
    family: Option<&Families>,
    batch: game_economy_family::BirthBatch<'_>,
) -> Result<(), String> {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(_, family)| family.rules.enroll_births)
    else {
        return Ok(());
    };
    let newborn_ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| batch.contains(m))
        .map(|m| m.id)
        .collect();
    if newborn_ids.is_empty() {
        return Ok(());
    }
    let eligible_at = batch
        .tick
        .checked_add(family.rules.adult_age)
        .ok_or("purpose eligibility overflow")?;
    let newborns: Vec<_> = newborn_ids
        .into_par_iter()
        .map(|id| game_purpose::Person {
            person: id,
            deprivation: 0,
            eligible_at,
            appraisal: None,
        })
        .collect();
    owner.people.par_extend(newborns);
    Ok(())
}
