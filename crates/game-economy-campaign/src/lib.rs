//! Ruler continuity and guardians over the same personal and physical world.
pub mod elapsed;
pub mod embodied;
use game_economy_person::{Person, travel::Travel};
use game_families::Families;
pub use game_guardians::{Guardian, Guardians, ReleaseDecision};
pub use game_lineage::{
    Invocation, InvocationRules, InvocationWarning, InvocationWarningKind, Life, Lineage,
};
use game_lots::{Place, View};
use game_transport::Route;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Campaign {
    pub lineage: Lineage,
    #[serde(default)]
    pub guardians: Option<Guardians>,
    #[serde(default)]
    pub embodied_guardians: Option<embodied::Guardians>,
}

impl Campaign {
    /// Give old opening declarations a body in the common person and travel owners.
    pub fn embody_opening_guardians(
        &mut self,
        period_millis: u64,
        world_max_health: u32,
    ) -> Result<Vec<Person>, String> {
        let Some(opening) = self.guardians.as_ref() else {
            return Ok(Vec::new());
        };
        if self.embodied_guardians.is_some()
            || opening.max_health != world_max_health
            || opening.recovery_per_tick == 0
            || opening.mentorship_ticks == 0
            || period_millis == 0
        {
            return Err(
                "guardian opening needs one body model and the world's health limit".into(),
            );
        }
        if opening
            .guardians
            .par_iter()
            .any(|row| row.confined || row.journey.is_some())
        {
            return Err("author confinement and travel on the guardian's physical body".into());
        }
        let people = opening
            .guardians
            .par_iter()
            .map(|row| Person {
                id: row.id,
                region: Some(row.region),
                health: row.health,
                hunger: 0,
                hunger_remainder: 0,
                starvation_remainder: 0,
                immortal: Some(game_economy_person::Immortal {
                    recovery_per_tick: 0,
                    recovery_rate: Some(game_economy_person::RecoveryRate {
                        units: opening.recovery_per_tick,
                        per_millis: period_millis,
                    }),
                    recovery_remainder: 0,
                }),
                controlled: false,
                willing_to_work: false,
                seek_meals: false,
            })
            .collect();
        self.embodied_guardians = Some(embodied::Guardians {
            guardians: opening
                .guardians
                .par_iter()
                .map(|row| embodied::Guardian {
                    person: row.id,
                    mentorship_required: row
                        .mentorship_required
                        .unwrap_or(opening.mentorship_ticks),
                    student: row.student,
                    mentorship: row.mentorship,
                    released: row.released,
                })
                .collect(),
        });
        self.guardians = None;
        Ok(people)
    }
}

fn routes(travel: Option<&Travel>) -> Result<Vec<Route>, String> {
    travel.map_or(Ok(Vec::new()), |travel| {
        travel
            .routes
            .par_iter()
            .map(|r| {
                Ok(Route {
                    source: r.source,
                    destination: r.destination,
                    ticks: u32::try_from(r.ticks).map_err(|_| "guardian route is too long")?,
                })
            })
            .collect()
    })
}
pub fn validate(
    campaign: Option<&Campaign>,
    family: Option<&Families>,
    people: &[Person],
    travel: Option<&Travel>,
    view: &View,
    tick: u64,
) -> Result<(), String> {
    let Some(campaign) = campaign else {
        return Ok(());
    };
    campaign
        .lineage
        .validate(family.ok_or("campaign requires family")?, tick)?;
    if campaign.guardians.is_some() && campaign.embodied_guardians.is_some() {
        return Err("campaign cannot mix guardian body models".into());
    }
    if let Some(guardians) = &campaign.embodied_guardians {
        guardians.validate(people, campaign.lineage.current)?;
    }
    if let Some(guardians) = &campaign.guardians {
        let mut regions: Vec<_> = view
            .containers
            .par_iter()
            .filter_map(|c| match c.place {
                Place::Stock { region, .. } => Some(region),
                _ => None,
            })
            .collect();
        regions.par_sort_unstable();
        regions.dedup();
        guardians.validate(&regions, &routes(travel)?)?;
    }
    Ok(())
}
/// Only the lineage's previous and current people change control state.
pub fn sync(campaign: &Campaign, people: &mut [Person], old: Option<u64>, tick: u64) {
    let current = campaign.lineage.current;
    if let Some(id) = old.filter(|id| Some(*id) != current)
        && let Ok(i) = people.binary_search_by_key(&id, |p| p.id)
    {
        people[i].controlled = false;
    }
    if let Some(id) = current
        && let Ok(i) = people.binary_search_by_key(&id, |p| p.id)
    {
        people[i].controlled = campaign.lineage.controlled_person(tick) == Some(id);
    }
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $physical:expr, $view:expr) => {{
        $crate::validate(
            $state.campaign.as_ref(),
            $state.family.as_ref(),
            &$state.people,
            $state.travel.as_ref(),
            &$view,
            $physical.tick(),
        )?;
        if let Some(campaign) = &$state.campaign {
            $crate::sync(campaign, &mut $state.people, None, $physical.tick());
        }
    }};
}
pub fn warning(owner: Option<&Campaign>, now: u64) -> Result<Option<Campaign>, String> {
    let Some(owner) = owner.filter(|c| c.lineage.invocation.is_some()) else {
        return Ok(None);
    };
    let mut next = owner.clone();
    if !next
        .lineage
        .invocation
        .as_mut()
        .expect("checked invocation")
        .prepare(now)?
    {
        return Ok(Some(next));
    }
    Ok(None)
}
pub struct Draft {
    next: Campaign,
    strain: Option<(u64, u32)>,
    lifespan: Option<(u64, u64)>,
}
impl Draft {
    pub fn prepare_guardians(
        &mut self,
        people: &[Person],
        travel: Option<&Travel>,
        busy: &[u64],
    ) -> (Vec<game_economy_person::travel::Departure>, Vec<u64>) {
        self.next.embodied_guardians.as_mut().map_or_else(
            || (Vec::new(), Vec::new()),
            |guardians| guardians.prepare(self.next.lineage.current, people, travel, busy),
        )
    }
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr) => {
        $crate::prepare(
            $world.campaign.as_ref(),
            &$world.people,
            $world.travel.as_ref(),
            $world.tick(),
        )?
    };
}
pub fn prepare(
    owner: Option<&Campaign>,
    people: &[Person],
    travel: Option<&Travel>,
    now: u64,
) -> Result<Option<Draft>, String> {
    let Some(owner) = owner else {
        return Ok(None);
    };
    let mut next = owner.clone();
    if let Some(invocation) = next
        .lineage
        .prepare_invocation(now)
        .map_err(str::to_owned)?
    {
        next.lineage.invocation = Some(invocation);
    }
    let invocation = next.lineage.invocation.as_ref();
    let strain = invocation.and_then(|i| {
        (!i.active.is_empty()
            && i.burden >= i.rules.strain_threshold
            && i.rules.strain_damage_per_tick > 0)
            .then_some((i.owner?, i.rules.strain_damage_per_tick))
    });
    let lifespan = invocation.and_then(|i| {
        (!i.active.is_empty()
            && i.rules.lifespan_loss_per_tick > 0
            && i.exposure >= i.rules.permanent_exposure_threshold)
            .then_some((i.owner?, i.rules.lifespan_loss_per_tick))
    });
    if let Some(guardians) = &owner.guardians {
        let target = owner.lineage.current.map(|id| {
            (
                id,
                people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .and_then(|i| people[i].region),
            )
        });
        next.guardians = Some(guardians.advance(target, &routes(travel)?));
    }
    Ok(Some(Draft {
        next,
        strain,
        lifespan,
    }))
}
pub fn apply_strain(draft: Option<&Draft>, people: &mut [Person]) {
    if let Some((id, damage)) = draft.and_then(|d| d.strain)
        && let Ok(index) = people.binary_search_by_key(&id, |p| p.id)
    {
        people[index].health = people[index].health.saturating_sub(damage);
    }
}
/// Copy family columns only when an actual permanent cost needs staging.
pub fn family_with_cost(
    draft: Option<&Draft>,
    family: Option<&Families>,
) -> Result<Option<Families>, String> {
    let Some((id, loss)) = draft.and_then(|d| d.lifespan) else {
        return Ok(None);
    };
    let old = family.ok_or("invocation cost requires family")?;
    if old.elapsed_lifetimes.is_some() {
        return Err("invocation lifespan costs require boundary family rules".into());
    }
    let mut next = Families {
        rules: old.rules.clone(),
        traits: old.traits.clone(),
        members: old.members.par_iter().cloned().collect(),
        next_id: old.next_id,
        last_tick: old.last_tick,
        elapsed_lifetimes: None,
        totals: old.totals.clone(),
        events: old.events.par_iter().cloned().collect(),
    };
    let member = next
        .members
        .par_iter_mut()
        .find_any(|m| m.id == id)
        .ok_or("unknown invocation owner")?;
    member.lifespan_loss = member
        .lifespan_loss
        .checked_add(loss)
        .ok_or("lifespan loss overflow")?;
    Ok(Some(next))
}
pub fn finish(
    draft: Option<Draft>,
    family: Option<&Families>,
    people: &mut [Person],
    tick: u64,
) -> Option<Campaign> {
    let mut next = draft?.next;
    let old = next.lineage.current;
    if let Some(family) = family {
        next.lineage.advance(family, tick);
    }
    sync(&next, people, old, tick);
    Some(next)
}
#[macro_export]
macro_rules! preflight {
    ($world:expr) => {
        if let Some(pending) = $crate::warning($world.campaign.as_ref(), $world.tick())? {
            if $world.campaign.as_ref() != Some(&pending) {
                let epoch = $world
                    .epoch
                    .checked_add(1)
                    .ok_or("economic generation overflow")?;
                $world.campaign = Some(pending);
                $world.epoch = epoch;
            }
            return Err("invocation warning needs a choice".into());
        }
    };
}
#[macro_export]
macro_rules! campaign_methods {
    () => {
        pub fn invoke_lives(&mut self, lives: &[u64]) -> Result<(), String> {
            let now = self.tick();
            let mut campaign = self.campaign.clone().ok_or("campaign is disabled")?;
            let history: Vec<_> = campaign
                .lineage
                .history
                .iter()
                .map(|life| life.person)
                .collect();
            campaign
                .lineage
                .invocation
                .as_mut()
                .ok_or("invocation is disabled")?
                .activate(lives, &history, now)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            $crate::sync(&campaign, &mut self.people, None, now);
            self.campaign = Some(campaign);
            self.epoch = epoch;
            Ok(())
        }
        pub fn dismiss_invocation(&mut self) -> Result<(), String> {
            let now = self.tick();
            let mut campaign = self.campaign.clone().ok_or("campaign is disabled")?;
            campaign
                .lineage
                .invocation
                .as_mut()
                .ok_or("invocation is disabled")?
                .dismiss(now)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            $crate::sync(&campaign, &mut self.people, None, now);
            self.campaign = Some(campaign);
            self.epoch = epoch;
            Ok(())
        }
        pub fn accept_invocation_warning(&mut self, revision: u64) -> Result<(), String> {
            let now = self.tick();
            let mut campaign = self.campaign.clone().ok_or("campaign is disabled")?;
            campaign
                .lineage
                .invocation
                .as_mut()
                .ok_or("invocation is disabled")?
                .accept(revision, now)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            $crate::sync(&campaign, &mut self.people, None, now);
            self.campaign = Some(campaign);
            self.epoch = epoch;
            Ok(())
        }
        pub fn request_guardian_release(
            &mut self,
            guardian: u64,
        ) -> Result<$crate::ReleaseDecision, String> {
            let mut campaign = self.campaign.clone().ok_or("campaign is disabled")?;
            let target = campaign.lineage.current.map(|id| {
                (
                    id,
                    self.people
                        .binary_search_by_key(&id, |p| p.id)
                        .ok()
                        .and_then(|i| self.people[i].region),
                )
            });
            let result = if let Some(guardians) = campaign.embodied_guardians.as_mut() {
                guardians.request_release(guardian, campaign.lineage.current, &self.people)?
            } else {
                campaign
                    .guardians
                    .as_mut()
                    .ok_or("guardians are disabled")?
                    .request_release(guardian, target)?
            };
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.campaign = Some(campaign);
            self.epoch = epoch;
            Ok(result)
        }
    };
}
