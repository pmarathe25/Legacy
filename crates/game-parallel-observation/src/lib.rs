//! Player-facing projections of authoritative state. These views are read-only.
use game_parallel::{
    Amount, AutonomousActor, CaregivingEvent, Dream, FatiguePerson, Goal, Job, KnowledgeRecord,
    Mission, NutritionPerson, Opinion, Person, PersonalityPerson, PropertyEstate, PropertyTheft,
    PurposePerson, Route, SkillRecord, World,
};
mod civic;
mod tactical;
pub use civic::{ApplicationView, BallotView, InstitutionView};
pub use tactical::TacticalView;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Local {
        region: u64,
        food: u32,
        people: Vec<u64>,
    },
    Traveling {
        source: u64,
        destination: u64,
        ticks_remaining: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub tactical: Option<TacticalView>,
    pub tick: u64,
    pub season: Option<String>,
    pub ruler: Person,
    pub location: Location,
    /// Routes departing the ruler's current region, without remote state.
    pub outgoing_routes: Vec<Route>,
    /// Records can be stale and retain their witness and report provenance.
    pub records: Vec<KnowledgeRecord>,
    pub fatigue: Option<FatiguePerson>,
    pub nutrition: Option<NutritionPerson>,
    pub skills: Vec<SkillRecord>,
    pub skill_rates: Vec<SkillRate>,
    pub lessons: Vec<game_parallel::SkillLesson>,
    /// Temporary production capabilities from currently invoked lives.
    pub borrowed_skills: Vec<BorrowedSkill>,
    pub goals: Vec<Goal>,
    pub dreams: Vec<Dream>,
    pub missions: Vec<Mission>,
    pub purpose: Option<PurposePerson>,
    pub personality: Option<PersonalityPerson>,
    pub caregiving: Vec<CaregivingEvent>,
    pub opinions: Vec<Opinion>,
    pub thefts: Vec<PropertyTheft>,
    pub estates: Vec<PropertyEstate>,
    /// Local institutions; open ballot details are shown to members only.
    pub institutions: Vec<InstitutionView>,
    pub membership_requests: Vec<MembershipRequestView>,
    pub membership_decisions: Vec<game_parallel::InstitutionAdmissionEvent>,
    pub property_claims: Vec<game_parallel::PropertyClaim>,
    pub property_offers: Vec<game_parallel::PropertyOffer>,
    pub disposition: Option<AutonomousActor>,
    pub isolation: Option<u32>,
    /// Noticeable symptoms, without disclosing latent infection or its source.
    pub illness_symptoms: bool,
    pub carried_food: u32,
    pub carried_goods: Vec<Amount>,
    pub jobs: Vec<Job>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowedSkill {
    pub life: u64,
    pub recipe: u32,
    pub rate: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillRate {
    pub recipe: u32,
    pub rate: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipRequestView {
    pub body: u64,
    pub applicant: u64,
    pub requested_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardianView {
    pub tick: u64,
    pub guardian: u64,
    pub region: u64,
    pub destination: Option<u64>,
    pub ticks_remaining: Option<u32>,
    pub student: u64,
}
/// A ruler sees their own condition, present location, and personal records.
/// A remote region's current state is never substituted for a stale report.
pub fn ruler(world: &World) -> Result<View, &'static str> {
    let id = world
        .lineage
        .as_ref()
        .ok_or("lineage is disabled")?
        .current
        .ok_or("no current ruler")?;
    let tactical = tactical::view(world, id);
    let (person, location) = if let Some((region, person)) =
        world.regions.iter().find_map(|region| {
            region
                .people
                .iter()
                .find(|person| person.id == id)
                .map(|person| (region, person))
        }) {
        (
            person.clone(),
            Location::Local {
                region: region.id,
                food: region.food,
                people: tactical.as_ref().map_or_else(
                    || {
                        region
                            .people
                            .iter()
                            .filter(|person| person.health > 0)
                            .map(|person| person.id)
                            .collect()
                    },
                    |view| {
                        std::iter::once(id)
                            .chain(view.nearby.iter().map(|&(person, _)| person))
                            .collect()
                    },
                ),
            },
        )
    } else if let Some(traveler) = world
        .travelers
        .iter()
        .find(|traveler| traveler.person.id == id)
    {
        (
            traveler.person.clone(),
            Location::Traveling {
                source: traveler.source,
                destination: traveler.destination,
                ticks_remaining: traveler.ticks_remaining,
            },
        )
    } else {
        return Err("ruler is missing");
    };
    let mut outgoing_routes: Vec<_> = match &location {
        Location::Local { region, .. } => world
            .routes
            .iter()
            .filter(|route| route.source == *region)
            .cloned()
            .collect(),
        Location::Traveling { .. } => Vec::new(),
    };
    outgoing_routes.sort_unstable_by_key(|route| route.destination);
    let records = world
        .knowledge
        .as_ref()
        .and_then(|knowledge| knowledge.minds.iter().find(|mind| mind.person == id))
        .map_or_else(Vec::new, |mind| mind.records.clone());
    let fatigue = world.fatigue.as_ref().map(|state| {
        state
            .people
            .iter()
            .find(|person| person.id == id)
            .cloned()
            .unwrap_or(FatiguePerson {
                id,
                fatigue: 0,
                resting: false,
            })
    });
    let nutrition = world.nutrition.as_ref().map(|state| {
        state
            .people
            .iter()
            .find(|person| person.person == id)
            .cloned()
            .unwrap_or(NutritionPerson {
                person: id,
                reserves: state.rules.initial,
            })
    });
    let skills = world.skills.as_ref().map_or_else(Vec::new, |state| {
        state
            .records
            .iter()
            .filter(|record| record.person == id)
            .cloned()
            .collect()
    });
    let index = world.skills.as_ref().map(|skills| skills.index());
    let skill_rates = world.skills.as_ref().map_or_else(Vec::new, |skills| {
        let index = index.as_ref().expect("enabled skills");
        skills
            .rules
            .iter()
            .filter_map(|rule| {
                let rate = index.rate(id, rule.recipe);
                (rate > 1).then_some(SkillRate {
                    recipe: rule.recipe,
                    rate,
                })
            })
            .collect()
    });
    let borrowed_skills = match (
        world.skills.as_ref(),
        world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.invocation.as_ref()),
    ) {
        (Some(skills), Some(invocation))
            if invocation.owner == Some(id) && !invocation.active.is_empty() =>
        {
            let index = index.as_ref().expect("enabled skills");
            let mut rows: Vec<_> = invocation
                .active
                .iter()
                .flat_map(|&life| {
                    let index = &index;
                    skills.rules.iter().filter_map(move |rule| {
                        let rate = index.rate(life, rule.recipe);
                        (rate > 1).then_some(BorrowedSkill {
                            life,
                            recipe: rule.recipe,
                            rate,
                        })
                    })
                })
                .collect();
            rows.sort_unstable_by_key(|row| (row.life, row.recipe));
            rows
        }
        _ => Vec::new(),
    };
    let institutions = civic::institutions(world, id, &location);
    Ok(View {
        tactical,
        tick: world.tick,
        season: world
            .environment
            .as_ref()
            .and_then(|environment| environment.season(world.tick))
            .map(|season| season.name.clone()),
        ruler: person,
        location,
        outgoing_routes,
        institutions,
        membership_requests: world.institutions.as_ref().map_or_else(Vec::new, |owner| {
            owner
                .requests
                .iter()
                .filter(|request| request.applicant == id)
                .map(|request| MembershipRequestView {
                    body: request.body,
                    applicant: request.applicant,
                    requested_at: request.requested_at,
                })
                .collect()
        }),
        membership_decisions: world.institutions.as_ref().map_or_else(Vec::new, |owner| {
            owner
                .admissions
                .iter()
                .filter(|event| event.applicant == id)
                .cloned()
                .collect()
        }),
        property_claims: world.property.as_ref().map_or_else(Vec::new, |property| {
            property
                .claims
                .iter()
                .filter(|claim| claim.owner == id || claim.delegates.contains(&id))
                .cloned()
                .collect()
        }),
        property_offers: world.property.as_ref().map_or_else(Vec::new, |property| {
            property
                .offers
                .iter()
                .filter(|offer| offer.proposer == id || offer.recipient == id)
                .cloned()
                .collect()
        }),
        records,
        fatigue,
        nutrition,
        skills,
        skill_rates,
        lessons: world.skills.as_ref().map_or_else(Vec::new, |skills| {
            skills
                .lessons
                .iter()
                .filter(|lesson| lesson.teacher == id || lesson.student == id)
                .cloned()
                .collect()
        }),
        borrowed_skills,
        missions: world.missions.as_ref().map_or_else(Vec::new, |missions| {
            missions
                .entries
                .iter()
                .filter(|mission| mission.person == id)
                .cloned()
                .collect()
        }),
        dreams: world.dreams.as_ref().map_or_else(Vec::new, |dreams| {
            dreams
                .people
                .iter()
                .filter(|dream| dream.person == id)
                .cloned()
                .collect()
        }),
        goals: world.goals.as_ref().map_or_else(Vec::new, |goals| {
            goals
                .people
                .iter()
                .filter(|goal| goal.person == id)
                .cloned()
                .collect()
        }),
        caregiving: world.caregiving.as_ref().map_or_else(Vec::new, |state| {
            state
                .events
                .iter()
                .filter(|event| event.child == id || event.caregiver == Some(id))
                .cloned()
                .collect()
        }),
        personality: world
            .personality
            .as_ref()
            .and_then(|state| state.people.iter().find(|row| row.person == id).cloned()),
        purpose: world
            .purpose
            .as_ref()
            .and_then(|state| state.people.iter().find(|row| row.person == id).cloned()),
        estates: world.property.as_ref().map_or_else(Vec::new, |state| {
            state
                .estates
                .iter()
                .filter(|event| event.previous_owner == id || event.heir == id)
                .cloned()
                .collect()
        }),
        thefts: world.property.as_ref().map_or_else(Vec::new, |state| {
            state
                .thefts
                .iter()
                .filter(|event| event.actor == id || (event.owner == id && event.witnessed))
                .cloned()
                .collect()
        }),
        opinions: world.opinions.as_ref().map_or_else(Vec::new, |state| {
            state
                .records
                .iter()
                .filter(|row| row.observer == id)
                .cloned()
                .collect()
        }),
        isolation: world
            .social
            .as_ref()
            .filter(|social| {
                social
                    .participants
                    .iter()
                    .any(|participant| participant.person == id)
            })
            .and_then(|social| social.needs.as_ref())
            .map(|needs| {
                needs
                    .people
                    .iter()
                    .find(|person| person.person == id)
                    .map_or(0, |person| person.isolation)
            }),
        illness_symptoms: world.disease.as_ref().is_some_and(|disease| {
            disease
                .cases
                .iter()
                .filter(|case| case.person == id)
                .any(|case| {
                    disease
                        .strains
                        .iter()
                        .find(|strain| strain.id == case.strain)
                        .is_some_and(|strain| {
                            strain.damage_per_tick > 0
                                && world
                                    .tick
                                    .checked_sub(case.infected_at)
                                    .is_some_and(|age| strain.contagious(age))
                        })
                })
        }),
        carried_food: world
            .cargo
            .as_ref()
            .and_then(|cargo| cargo.holdings.iter().find(|holding| holding.person == id))
            .map_or(0, |holding| holding.food),
        carried_goods: world
            .goods_cargo
            .as_ref()
            .and_then(|cargo| cargo.holdings.iter().find(|holding| holding.person == id))
            .map_or_else(Vec::new, |holding| holding.items.to_vec()),
        jobs: world
            .production
            .as_ref()
            .map_or_else(Vec::new, |production| {
                production
                    .jobs
                    .iter()
                    .filter(|job| job.worker == id)
                    .cloned()
                    .collect()
            }),
        disposition: world
            .autonomy
            .as_ref()
            .and_then(|state| state.actors.iter().find(|actor| actor.person == id))
            .cloned(),
    })
}
/// Search spectating exposes only the guardian's own journey, and never teaches
/// the ruler or guardian the contents of a remote region.
pub fn guardian_search(world: &World, id: u64) -> Result<GuardianView, &'static str> {
    let student = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.current)
        .ok_or("no current ruler")?;
    let guardian = world
        .guardians
        .as_ref()
        .ok_or("guardians are disabled")?
        .guardians
        .iter()
        .find(|guardian| guardian.id == id)
        .ok_or("unknown guardian")?;
    if guardian.student != Some(student) || (guardian.journey.is_none() && guardian.mentorship > 0)
    {
        return Err("guardian is not searching for the ruler");
    }
    Ok(GuardianView {
        tick: world.tick,
        guardian: id,
        region: guardian.region,
        destination: guardian.journey.as_ref().map(|journey| journey.destination),
        ticks_remaining: guardian.journey.as_ref().map(|journey| journey.remaining),
        student,
    })
}

#[cfg(test)]
#[path = "../tests/support/observation.rs"]
mod tests;
