//! A court's decision is a supported request. Officers still need consent, food,
//! equipment, time, personal information and the same physical force as others.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub id: EnforcementCaseId,
    pub institution: InstitutionId,
    pub office: u16,
    pub filer: PersonId,
    pub target: PersonId,
    pub topic: EnforcementTopic,
    pub evidence: EnforcementEvidence,
    pub contact: Option<ReportedContact>,
    pub policy: EnforcementPolicy,
    pub judgment: Option<EnforcementJudgmentView>,
    pub mission: Option<MissionId>,
    pub settlement: Option<AgreementId>,
    pub status: EnforcementStatus,
    pub explanation: String,
    pub changed_at: SimTime,
    pub notice_area: Option<AreaId>,
    pub witnesses: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversation {
    File,
    Report {
        snapshot: Box<EnforcementCaseView>,
        institution: Box<InstitutionView>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Talk {
    pub case: EnforcementCaseId,
    pub recipient: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub conversation: Conversation,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub policies: BTreeMap<InstitutionId, BTreeMap<u16, EnforcementPolicyView>>,
    pub cases: BTreeMap<EnforcementCaseId, Case>,
    pub talks: BTreeMap<PersonId, Talk>,
    pub known: BTreeMap<PersonId, BTreeMap<EnforcementCaseId, EnforcementCaseView>>,
    pub known_policies:
        BTreeMap<PersonId, BTreeMap<InstitutionId, BTreeMap<u16, EnforcementPolicyView>>>,
    pub contacts: BTreeMap<PersonId, BTreeMap<PersonId, ReportedContact>>,
    pub assaults: BTreeMap<PersonId, BTreeMap<EventId, WitnessedAssault>>,
    pub reconsider_after: BTreeMap<PersonId, SimTime>,
    pub last_event: u64,
}
impl Simulation {
    pub fn enforcement_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::property_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "enforcement-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let judge = sim.host.state.ruler;
        let filer = sim.host.state.people[3].id;
        let officer = sim.host.state.people[4].id;
        let target = sim.host.state.people[5].id;
        let court = AreaId(1);
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.debug_actor_command(actor, Command::Cancel)?;
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
        }
        // This opening supplies a court and watch rather than the autonomy
        // fixture's recipe jobs. Old private plans must not pull witnesses away.
        let sites: Vec<_> = game_workshop::sites(&sim.host.storage)
            .map(|(id, _)| id)
            .collect();
        for site in sites {
            game_workshop::remove_site(&mut sim.host.storage, site)?;
        }
        let workers: Vec<_> = game_workshop::jobs(&sim.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in workers {
            game_workshop::remove_job(&mut sim.host.storage, actor).map(|_| ())?;
        }
        let actors: Vec<_> =
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
        for actor in actors {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.sites.clear();
            mind.plan = None;
        }
        if let Some(world) = &mut sim.host.state.development {
            world.rules = development::Rules::human();
            let adulthood = world.rules.adulthood;
            let actors: Vec<_> = game_lifecycle::all_lives(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
            for actor in actors {
                let life = game_lifecycle::life_mut(&mut sim.host.storage, actor).unwrap();
                life.initial_age = adulthood;
                life.stage = LifeStage::Adult;
            }
            for due in sim.host.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Age(_)));
            }
        }
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.schedule_age(actor);
        }
        let rules = &mut sim.host.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 3 * 86_400_000;
        rules.meal_relief = 86_400_000;
        rules.fatigue_limit = 16 * 3_600_000;
        for actor in [judge, filer, officer, target] {
            *game_spatial::location_mut(
                &mut sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0),
            )
            .expect("location exists") = Location::AtArea(court);
        }
        sim.host.state.missions = Some(missions::World {
            missions: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            known: BTreeMap::new(),
            talks: BTreeMap::new(),
            last_event: sim.host.next_event() - 1,
            last_review: sim.host.clock().time,
        });
        sim.host.state.institutions = Some(institutions::Runtime {
            last_event: sim.host.next_event() - 1,
        });
        for person in &sim.host.state.people {
            game_society::insert_knowledge(
                &mut sim.host.storage,
                person.id,
                game_society::Knowledge::default(),
            )?;
        }
        let treasury = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(court),
            Quantity(20_000),
            Quantity(30_000),
            BTreeSet::from([judge, officer]),
        )?;
        let provisions = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(court),
            Quantity(10_000),
            Quantity(10_000),
            BTreeSet::from([officer]),
        )?;
        let binding = sim.debug_add_inventory(
            Good::Binding,
            InventoryLocation::Area(court),
            Quantity(3_000),
            Quantity(3_000),
            BTreeSet::from([officer]),
        )?;
        let tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(court),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([officer]),
        )?;
        for (source, custodian) in [
            (treasury, judge),
            (provisions, officer),
            (binding, officer),
            (tool, officer),
        ] {
            sim.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(source, custodian);
        }
        let forest_store = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(2_000),
            Quantity(10_000),
            BTreeSet::from([filer]),
        )?;
        sim.host
            .state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(forest_store, filer);
        sim.debug_set_property_attitude(
            filer,
            property::Attitude {
                exclusive_claims: 900,
                sharing: 100,
                willingness_to_defend: 0,
            },
        )?;
        let founders = BTreeSet::from([judge, filer, officer, target]);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Found {
                charter: InstitutionCharter {
                    purpose: "A locally chosen court and funded watch".into(),
                    jurisdiction: court,
                    founders: founders.clone(),
                    open_membership: true,
                    procedure: DecisionProcedure {
                        proposers: Subjects::members(),
                        voters: Subjects::members(),
                        minimum_support: 2,
                    },
                    offices: vec![
                        OfficeMandate {
                            slot: 0,
                            holder: judge,
                            inventories: BTreeSet::from([treasury]),
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([OfficePower::Adjudicate]),
                            minimum_support: 2,
                        },
                        OfficeMandate {
                            slot: 1,
                            holder: officer,
                            inventories: BTreeSet::from([provisions, binding, tool]),
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([OfficePower::Enforce]),
                            minimum_support: 1,
                        },
                    ],
                    rules: Vec::new(),
                    contributions: Vec::new(),
                    evidence: BTreeSet::new(),
                },
                origin: None,
            },
        )?;
        let institution = game_society::institutions(&sim.host.storage)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for actor in founders.iter().copied().filter(|p| *p != judge) {
            sim.debug_institution_action(actor, InstitutionAction::Join { institution })?;
        }
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::EnforcementPolicy {
                    office: 0,
                    policy: Some(EnforcementPolicy {
                        topics: BTreeSet::from([
                            EnforcementTopic::Taking,
                            EnforcementTopic::Assault,
                            EnforcementTopic::Default,
                        ]),
                        areas: BTreeSet::from([AreaId(1), AreaId(2), AreaId(3)]),
                        subjects: Subjects::members(),
                        enforcer_office: 1,
                        remedies: BTreeSet::from([
                            RemedyKind::Dismiss,
                            RemedyKind::Settlement,
                            RemedyKind::Confinement,
                            RemedyKind::Deposition,
                        ]),
                        spending: BTreeMap::from([(treasury, Quantity(2_000))]),
                        supplies: BTreeMap::from([
                            (provisions, Quantity(3_000)),
                            (binding, Quantity(1_000)),
                            (tool, Quantity(1_000)),
                        ]),
                        maximum_confinement: SimDuration(1_200_000),
                    }),
                },
            },
        )?;
        let proposal = game_society::proposals(&sim.host.storage)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for actor in [judge, filer] {
            sim.debug_institution_action(
                actor,
                InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            )?;
        }
        let mut willing = decisions::Personality::restful();
        willing.discipline = 900;
        willing.honesty = 900;
        willing.aggression = 900;
        willing.risk_tolerance = 900;
        willing.empathy = 100;
        sim.debug_set_personality(officer, willing, decisions::Aspiration::SupportOthers)?;
        sim.debug_set_relationship(
            officer,
            judge,
            decisions::Relationship {
                trust: 800,
                affection: 500,
            },
        )?;
        sim.review_decisions()?;
        sim.observe_enforcement();
        sim.update_property_events()?;
        Ok(sim)
    }
    fn enforcement_world(&mut self) {
        if self.host.state.enforcement.is_none() {
            self.host.state.enforcement = Some(World {
                last_event: self.host.next_event() - 1,
                ..World::default()
            });
        }
    }
    fn enforcement_area(&self, actor: PersonId) -> Option<AreaId> {
        let person = self
            .host
            .state
            .people
            .get(*self.person_index.get(&actor)?)?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.combat_restricts(actor)
        {
            return None;
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((person.id).0))
            .expect("location exists")
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn enforcement_office(
        &self,
        institution: InstitutionId,
        office: u16,
    ) -> Option<&OfficeMandate> {
        let i = game_society::institution(&self.host.storage, institution)?;
        let mandate = i.charter.offices.iter().find(|o| o.slot == office)?;
        self.office_effective(i, mandate).then_some(mandate)
    }
    pub(crate) fn validate_enforcement_policy(
        &self,
        i: &institutions::Institution,
        office: u16,
        policy: &EnforcementPolicy,
    ) -> Result<(), Error> {
        let judge = i
            .charter
            .offices
            .iter()
            .find(|o| o.slot == office)
            .ok_or(Error::Unavailable)?;
        let enforcer = i
            .charter
            .offices
            .iter()
            .find(|o| o.slot == policy.enforcer_office)
            .ok_or(Error::Unavailable)?;
        if !judge.powers.contains(&OfficePower::Adjudicate)
            || !enforcer.powers.contains(&OfficePower::Enforce)
            || policy.topics.is_empty()
            || policy.remedies.is_empty()
            || policy.areas.is_empty()
            || policy.areas.len() > 16
            || !policy.areas.contains(&i.charter.jurisdiction)
            || policy
                .areas
                .iter()
                .any(|id| !self.host.state.areas.iter().any(|a| a.id == *id))
            || !self.validate_subjects(&policy.subjects, &i.charter.offices)
            || policy.spending.len() > 16
            || policy.supplies.len() > 16
            || policy.remedies.contains(&RemedyKind::Confinement)
                && policy.maximum_confinement.0 == 0
            || policy.spending.iter().any(|(source, q)| {
                q.0 == 0
                    || !judge.inventories.contains(source)
                    || game_cargo::inventory(&self.host.storage, *(source))
                        .is_none_or(|i| q > &i.capacity)
            })
            || policy.supplies.iter().any(|(source, q)| {
                q.0 == 0
                    || !enforcer.inventories.contains(source)
                    || game_cargo::inventory(&self.host.storage, *(source))
                        .is_none_or(|i| q > &i.capacity)
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn validate_enforcement_change(
        &self,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        match change {
            InstitutionChange::EnforcementPolicy {
                office,
                policy: Some(policy),
            } => self.validate_enforcement_policy(i, *office, policy),
            InstitutionChange::EnforcementPolicy {
                office,
                policy: None,
            } => i
                .charter
                .offices
                .iter()
                .any(|o| o.slot == *office)
                .then_some(())
                .ok_or(Error::Unavailable),
            InstitutionChange::Adjudicate { case, remedy } => {
                let case = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.cases.get(case))
                    .ok_or(Error::Unavailable)?;
                if case.institution != i.id
                    || case.status != EnforcementStatus::Pending
                    || case.judgment.is_some()
                    || !case.policy.remedies.contains(&remedy.kind())
                    || self
                        .enforcement_office(i.id, case.office)
                        .is_none_or(|m| !m.powers.contains(&OfficePower::Adjudicate))
                    || self
                        .host
                        .state
                        .enforcement
                        .as_ref()
                        .and_then(|w| w.policies.get(&i.id))
                        .and_then(|w| w.get(&case.office))
                        .is_none()
                {
                    return Err(Error::Unavailable);
                }
                match remedy {
                    EnforcementRemedy::Dismiss => {}
                    EnforcementRemedy::Confine { until } => {
                        if *until <= self.host.clock().time
                            || until.0 - self.host.clock().time.0
                                > case.policy.maximum_confinement.0
                            || self.host.state.combat.is_none()
                        {
                            return Err(Error::Unavailable);
                        }
                    }
                    EnforcementRemedy::Depose {
                        office,
                        replacement,
                    } => {
                        if !i.members.contains(replacement)
                            || *replacement == case.target
                            || i.charter
                                .offices
                                .iter()
                                .find(|m| m.slot == *office)
                                .is_none_or(|m| m.holder != case.target)
                        {
                            return Err(Error::Unavailable);
                        }
                    }
                    EnforcementRemedy::Settlement { terms } => {
                        if terms.is_empty() || terms.len()>8 || terms.iter().any(|term| !matches!(term,AgreementTerm::Delivery {debtor,quantity,deadline,..} if *debtor==case.target && quantity.0>0 && *deadline>self.host.clock().time)) {return Err(Error::Unavailable);}
                        for term in terms {
                            if let AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } = term
                                && game_cargo::inventory(&self.host.storage, *(source))
                                    .zip(game_cargo::inventory(&self.host.storage, *(destination)))
                                    .is_none_or(|(a, b)| {
                                        a.good != b.good
                                            || a.id == b.id
                                            || quantity.0 > b.capacity.0
                                    })
                            {
                                return Err(Error::Unavailable);
                            }
                        }
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn enforcement_change_actor_allowed(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
        proposing: bool,
    ) -> bool {
        if let InstitutionChange::Adjudicate { case, .. } = change {
            let Some(case) = self
                .host
                .state
                .enforcement
                .as_ref()
                .and_then(|w| w.cases.get(case))
            else {
                return false;
            };
            self.host
                .state
                .enforcement
                .as_ref()
                .and_then(|w| w.known.get(&actor))
                .is_some_and(|known| known.contains_key(&case.id))
                && (!proposing
                    || self
                        .enforcement_office(i.id, case.office)
                        .is_some_and(|o| o.holder == actor))
        } else {
            true
        }
    }
    pub(crate) fn enforcement_change_ids(change: &InstitutionChange) -> u64 {
        u64::from(matches!(change, InstitutionChange::Adjudicate { .. }))
    }
    pub(crate) fn preflight_enforcement_adoption(
        &self,
        institution: InstitutionId,
        proposal: RuleProposalId,
    ) -> Result<(), Error> {
        let record = self
            .host
            .state
            .institutions
            .as_ref()
            .and_then(|_| game_society::proposal(&self.host.storage, proposal))
            .ok_or(Error::Unavailable)?;
        let community =
            game_society::institution(&self.host.storage, institution).ok_or(Error::Unavailable)?;
        if record.institution != institution {
            return Err(Error::Unavailable);
        }
        self.validate_enforcement_change(community, &record.change)?;
        if matches!(record.change, InstitutionChange::Adjudicate { .. }) {
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        }
        self.ensure_event_capacity(2)
    }
    pub(crate) fn adopt_enforcement_change(
        &mut self,
        institution: InstitutionId,
        proposal: RuleProposalId,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let change = game_society::proposal(&self.host.storage, proposal)
            .expect("validated Society reference")
            .clone();
        match change.change {
            InstitutionChange::EnforcementPolicy { office, policy } => {
                self.enforcement_world();
                let policies = self
                    .host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .policies
                    .entry(institution)
                    .or_default();
                if let Some(policy) = policy {
                    policies.insert(
                        office,
                        EnforcementPolicyView {
                            institution,
                            office,
                            policy,
                            adopted_at: owner_now,
                            observed_at: owner_now,
                        },
                    );
                } else {
                    policies.remove(&office);
                }
                self.emit(EventKind::EnforcementPolicyChanged {
                    institution,
                    office,
                });
            }
            InstitutionChange::Adjudicate { case, remedy } => {
                let judgment = JudgmentId(self.host.allocate_id()?.0);
                let now = owner_now;
                let record = self
                    .host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .cases
                    .get_mut(&case)
                    .unwrap();
                record.judgment = Some(EnforcementJudgmentView {
                    id: judgment,
                    proposer: change.proposer,
                    proposal,
                    remedy: remedy.clone(),
                    adopted_at: now,
                });
                record.status = if matches!(remedy, EnforcementRemedy::Dismiss) {
                    EnforcementStatus::Dismissed
                } else {
                    EnforcementStatus::Decided
                };
                record.changed_at = now;
                record.notice_area = Some(
                    game_society::institution(&self.host.storage, institution)
                        .expect("validated Society reference")
                        .charter
                        .jurisdiction,
                );
                record.witnesses = BTreeSet::from([change.proposer]);
                record.explanation =
                    "The institution adopted a remedy; physical compliance remains unresolved"
                        .into();
                if matches!(remedy, EnforcementRemedy::Depose { .. }) {
                    let case = self
                        .host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .cases
                        .get_mut(&case)
                        .unwrap();
                    case.status = EnforcementStatus::Completed;
                    case.explanation="The chosen procedure replaced the officeholder; their body, possessions and debts remain".into();
                }
                self.emit(EventKind::EnforcementDecided {
                    case,
                    judgment,
                    proposer: change.proposer,
                });
            }
            _ => {}
        }
        self.observe_enforcement();
        Ok(())
    }
    fn evidence_for(
        &self,
        actor: PersonId,
        evidence: EnforcementEvidenceRef,
    ) -> Option<(
        EnforcementTopic,
        PersonId,
        EnforcementEvidence,
        Option<ReportedContact>,
    )> {
        let (topic, target, basis, contact) = match evidence {
            EnforcementEvidenceRef::Incident(id) => {
                let incident = self
                    .property_view(actor)
                    .incidents
                    .into_iter()
                    .find(|i| i.id == id)?;
                (
                    EnforcementTopic::Taking,
                    incident.actor,
                    EnforcementEvidence::Incident(incident.clone()),
                    Some(ReportedContact {
                        person: incident.actor,
                        area: incident.area,
                        observed_at: incident.changed_at,
                    }),
                )
            }
            EnforcementEvidenceRef::Assault(id) => {
                let assault = self
                    .host
                    .state
                    .enforcement
                    .as_ref()?
                    .assaults
                    .get(&actor)?
                    .get(&id)?
                    .clone();
                (
                    EnforcementTopic::Assault,
                    assault.attacker,
                    EnforcementEvidence::Assault(assault.clone()),
                    Some(ReportedContact {
                        person: assault.attacker,
                        area: assault.area,
                        observed_at: assault.observed_at,
                    }),
                )
            }
            EnforcementEvidenceRef::Default(id) => {
                let obligation = self
                    .agreement_view(actor)
                    .into_iter()
                    .flat_map(|a| a.obligations)
                    .find(|o| o.id == id && o.defaulted && o.performed < o.amount())?;
                let target = obligation.term.parties()[0];
                (
                    EnforcementTopic::Default,
                    target,
                    EnforcementEvidence::Default(obligation),
                    None,
                )
            }
            EnforcementEvidenceRef::Social(id) => {
                let case = self
                    .social_case_view(actor)
                    .into_iter()
                    .find(|c| c.id == id && c.kind == SocialCaseKind::Exclusion)?;
                (
                    EnforcementTopic::Exclusion,
                    case.counterpart,
                    EnforcementEvidence::Social(case.clone()),
                    Some(ReportedContact {
                        person: case.counterpart,
                        area: case.area,
                        observed_at: case.observed_at,
                    }),
                )
            }
        };
        let newer = self
            .host
            .state
            .enforcement
            .as_ref()
            .and_then(|w| w.contacts.get(&actor))
            .and_then(|k| k.get(&target))
            .cloned();
        let contact = match (contact, newer) {
            (Some(old), Some(new)) => Some(if new.observed_at > old.observed_at {
                new
            } else {
                old
            }),
            (old, new) => old.or(new),
        };
        Some((topic, target, basis, contact))
    }
    fn case_view(
        &self,
        case: &Case,
        actor: PersonId,
        source: KnowledgeSource,
    ) -> EnforcementCaseView {
        EnforcementCaseView {
            id: case.id,
            institution: case.institution,
            office: case.office,
            filer: case.filer,
            target: case.target,
            topic: case.topic,
            evidence: case.evidence.clone(),
            contact: case.contact.clone(),
            judgment: case.judgment.clone(),
            mission: case.mission,
            settlement: case.settlement,
            status: case.status,
            explanation: case.explanation.clone(),
            observed_at: case.changed_at,
            learned_at: self.host.clock().time,
            source: if matches!(source, KnowledgeSource::Observed(_)) {
                KnowledgeSource::Observed(actor)
            } else {
                source
            },
        }
    }
    pub(crate) fn enforcement_view(&self, actor: PersonId) -> EnforcementView {
        let Some(world) = &self.host.state.enforcement else {
            return EnforcementView::default();
        };
        EnforcementView {
            policies: world
                .known_policies
                .get(&actor)
                .into_iter()
                .flat_map(|institutions| institutions.values())
                .flat_map(|offices| offices.values())
                .take(32)
                .cloned()
                .collect(),
            cases: world
                .known
                .get(&actor)
                .into_iter()
                .flat_map(|cases| cases.values().rev())
                .take(64)
                .cloned()
                .collect(),
            assaults: world
                .assaults
                .get(&actor)
                .into_iter()
                .flat_map(|cases| cases.values().rev())
                .take(32)
                .cloned()
                .collect(),
        }
    }
    pub fn debug_enforcement_action(
        &mut self,
        actor: PersonId,
        action: EnforcementAction,
    ) -> Result<(), Error> {
        self.enforcement_command(actor, &action)
    }
    pub(crate) fn enforcement_command(
        &mut self,
        actor: PersonId,
        action: &EnforcementAction,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        let area = self.enforcement_area(actor).ok_or(Error::Unavailable)?;
        if self.in_combat(actor) {
            return Err(Error::Unavailable);
        }
        match action {
            EnforcementAction::File {
                institution,
                office,
                evidence,
            } => {
                let known = self
                    .enforcement_view(actor)
                    .policies
                    .into_iter()
                    .find(|p| p.institution == *institution && p.office == *office)
                    .ok_or(Error::Unavailable)?;
                let actual = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.policies.get(institution))
                    .and_then(|w| w.get(office))
                    .ok_or(Error::Unavailable)?;
                if known.policy != actual.policy {
                    return Err(Error::Unavailable);
                }
                let (topic, target, basis, contact) = self
                    .evidence_for(actor, *evidence)
                    .ok_or(Error::Unavailable)?;
                let i = game_society::institution(&self.host.storage, institution)
                    .ok_or(Error::Unavailable)?;
                let officer = self
                    .enforcement_office(*institution, *office)
                    .ok_or(Error::Unavailable)?;
                let recipient = officer.holder;
                if i.charter.jurisdiction != area
                    || self.enforcement_area(recipient) != Some(area)
                    || !officer.powers.contains(&OfficePower::Adjudicate)
                    || !known.policy.topics.contains(&topic)
                    || !self.subjects(i, &known.policy.subjects).contains(&target)
                    || contact
                        .as_ref()
                        .is_some_and(|c| !known.policy.areas.contains(&c.area))
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(owner_now.0.checked_add(MINUTE.0).ok_or(Error::Overflow)?);
                self.ensure_event_capacity(2)?;
                let id = EnforcementCaseId(self.host.allocate_id()?.0);
                self.interrupt_actor_activity(actor);
                let case = Case {
                    id,
                    institution: *institution,
                    office: *office,
                    filer: actor,
                    target,
                    topic,
                    evidence: basis,
                    contact,
                    policy: known.policy,
                    judgment: None,
                    mission: None,
                    settlement: None,
                    status: EnforcementStatus::Filing,
                    explanation: "Preparing a complaint from remembered observations or reports"
                        .into(),
                    changed_at: owner_now,
                    notice_area: None,
                    witnesses: BTreeSet::from([actor]),
                };
                let view = self.case_view(&case, actor, KnowledgeSource::Observed(actor));
                let world = self.host.state.enforcement.as_mut().unwrap();
                world.cases.insert(id, case);
                world.known.entry(actor).or_default().insert(id, view);
                self.start_enforcement_talk(
                    actor,
                    Talk {
                        case: id,
                        recipient,
                        area,
                        completes,
                        conversation: Conversation::File,
                    },
                );
                self.emit(EventKind::EnforcementFiling {
                    case: id,
                    actor,
                    recipient,
                });
            }
            EnforcementAction::Report { case, recipient } => {
                let snapshot = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.known.get(&actor))
                    .and_then(|k| k.get(case))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                let institution = self
                    .institution_view(actor)
                    .into_iter()
                    .find(|i| i.id == snapshot.institution)
                    .ok_or(Error::Unavailable)?;
                if *recipient == actor
                    || self.enforcement_area(*recipient) != Some(area)
                    || matches!(snapshot.status, EnforcementStatus::Filing)
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(owner_now.0.checked_add(MINUTE.0).ok_or(Error::Overflow)?);
                self.ensure_event_capacity(1)?;
                self.interrupt_actor_activity(actor);
                self.start_enforcement_talk(
                    actor,
                    Talk {
                        case: *case,
                        recipient: *recipient,
                        area,
                        completes,
                        conversation: Conversation::Report {
                            snapshot: Box::new(snapshot),
                            institution: Box::new(institution),
                        },
                    },
                );
            }
            EnforcementAction::Respond { case, comply } => {
                let known = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.known.get(&actor))
                    .and_then(|k| k.get(case))
                    .ok_or(Error::Unavailable)?;
                let actual = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.cases.get(case))
                    .ok_or(Error::Unavailable)?
                    .clone();
                let judgment = actual.judgment.as_ref().ok_or(Error::Unavailable)?;
                if actual.target != actor
                    || known.judgment.as_ref() != Some(judgment)
                    || !matches!(
                        actual.status,
                        EnforcementStatus::Decided | EnforcementStatus::Refused
                    )
                    || game_society::institution(&self.host.storage, actual.institution)
                        .unwrap()
                        .charter
                        .jurisdiction
                        != area
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(2)?;
                let mut settlement = None;
                if *comply {
                    if let EnforcementRemedy::Settlement { terms } = &judgment.remedy {
                        if let Some(old) = self.existing_settlement_debt(&actual) {
                            let debt = self
                                .host
                                .state
                                .agreements
                                .as_ref()
                                .unwrap()
                                .obligations
                                .get(&old)
                                .ok_or(Error::Unavailable)?;
                            settlement = Some(debt.agreement);
                        } else {
                            let parties = terms
                                .iter()
                                .flat_map(|t| t.parties())
                                .collect::<BTreeSet<_>>();
                            let next = self.host.next_id();
                            self.agreement_command(
                                actor,
                                &AgreementAction::Propose {
                                    parties,
                                    terms: terms.clone(),
                                    procedure: None,
                                },
                            )?;
                            settlement = self
                                .host
                                .state
                                .agreements
                                .as_ref()
                                .unwrap()
                                .agreements
                                .values()
                                .find(|a| a.id.0 >= next && a.proposer == actor)
                                .map(|a| a.id);
                        }
                    } else {
                        return Err(Error::Unavailable);
                    }
                }
                self.ensure_event_capacity(1)?;
                let record = self
                    .host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .cases
                    .get_mut(case)
                    .unwrap();
                record.settlement = settlement;
                record.status = if *comply {
                    EnforcementStatus::Offered
                } else {
                    EnforcementStatus::Refused
                };
                record.changed_at = owner_now;
                record.notice_area = Some(area);
                record.witnesses = BTreeSet::from([actor]);
                record.explanation = if *comply {
                    "The person offered physical settlement; goods are still owed"
                } else {
                    "The person refused the court's request"
                }
                .into();
                self.emit(EventKind::EnforcementResponded {
                    case: *case,
                    actor,
                    comply: *comply,
                });
            }
            EnforcementAction::Dispatch { case, proposal } => {
                self.dispatch_enforcement(actor, *case, proposal)?
            }
        }
        self.observe_enforcement();
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn start_enforcement_talk(&mut self, actor: PersonId, talk: Talk) {
        *self.host.storage.activity_mut(actor).unwrap() = Activity::EnforcementTalk {
            case: talk.case,
            recipient: talk.recipient,
            completes: talk.completes,
        };
        self.host
            .state
            .due
            .entry(talk.completes)
            .or_default()
            .push(Due::Physical(actor));
        self.host
            .state
            .enforcement
            .as_mut()
            .unwrap()
            .talks
            .insert(actor, talk);
    }
    pub(crate) fn interrupt_enforcement(&mut self, actor: PersonId) {
        let owner_now = self.host.clock().time;
        let Some(talk) = self
            .host
            .state
            .enforcement
            .as_mut()
            .and_then(|w| w.talks.remove(&actor))
        else {
            return;
        };
        if let Some(change) = game_society::enforcement::complaint_interruption(
            owner_now,
            matches!(talk.conversation, Conversation::File),
        ) {
            let case = self
                .host
                .state
                .enforcement
                .as_mut()
                .unwrap()
                .cases
                .get_mut(&talk.case)
                .unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = change.changed_at;
            case.explanation = change.explanation.into();
        }
    }
    pub(crate) fn complete_enforcement_talk(&mut self, actor: PersonId) {
        let owner_now = self.host.clock().time;
        let Some(talk) = self
            .host
            .state
            .enforcement
            .as_mut()
            .and_then(|w| w.talks.remove(&actor))
        else {
            return;
        };
        if self.enforcement_area(actor) == Some(talk.area)
            && self.enforcement_area(talk.recipient) == Some(talk.area)
        {
            match talk.conversation {
                Conversation::File => {
                    let record = self
                        .host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .cases
                        .get_mut(&talk.case)
                        .unwrap();
                    record.status = EnforcementStatus::Pending;
                    record.changed_at = owner_now;
                    record.notice_area = Some(talk.area);
                    record.witnesses = BTreeSet::from([actor, talk.recipient]);
                    record.explanation =
                        "A complaint reached the officer; the accusation has not been decided"
                            .into();
                    let case = record.clone();
                    let listeners = self
                        .local_enforcement_listeners(talk.area)
                        .into_iter()
                        .chain([actor, talk.recipient])
                        .collect::<BTreeSet<_>>();
                    for listener in listeners {
                        let snapshot =
                            self.case_view(&case, listener, KnowledgeSource::Reported(actor));
                        self.host
                            .state
                            .enforcement
                            .as_mut()
                            .unwrap()
                            .known
                            .entry(listener)
                            .or_default()
                            .insert(case.id, snapshot);
                    }
                    self.emit(EventKind::EnforcementFiled {
                        case: case.id,
                        actor,
                        institution: case.institution,
                    });
                }
                Conversation::Report {
                    mut snapshot,
                    institution,
                } => {
                    snapshot.learned_at = owner_now;
                    snapshot.source = KnowledgeSource::Reported(actor);
                    let known = self
                        .host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(talk.recipient)
                        .or_default();
                    if let Some(old) = known.get(&snapshot.id).cloned() {
                        let contact = match (old.contact.clone(), snapshot.contact.clone()) {
                            (Some(a), Some(b)) => {
                                Some(if a.observed_at > b.observed_at { a } else { b })
                            }
                            (a, b) => a.or(b),
                        };
                        if old.observed_at > snapshot.observed_at {
                            *snapshot = old;
                        }
                        snapshot.contact = contact;
                    }
                    known.insert(snapshot.id, *snapshot);
                    if let Some(memory) =
                        game_society::knowledge_mut(&mut self.host.storage, talk.recipient)
                        && memory
                            .institutions
                            .get(&institution.id)
                            .is_none_or(|old| old.observed_at <= institution.observed_at)
                    {
                        memory.institutions.insert(institution.id, *institution);
                    }
                    self.emit(EventKind::EnforcementReported {
                        case: talk.case,
                        sender: actor,
                        recipient: talk.recipient,
                    });
                }
            }
        } else if matches!(talk.conversation, Conversation::File) {
            let case = self
                .host
                .state
                .enforcement
                .as_mut()
                .unwrap()
                .cases
                .get_mut(&talk.case)
                .unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = owner_now;
            case.explanation = "The officer could no longer hear the complaint".into();
        }
        if self.host.state.physical.is_some()
            && let Some(activity) = self.host.storage.activity_mut(actor)
        {
            *activity = Activity::Idle;
        }
    }
    fn local_enforcement_listeners(&self, area: AreaId) -> Vec<PersonId> {
        self.person_index
            .iter()
            .filter(|(_, index)| {
                let p = &self.host.state.people[**index];
                *game_spatial::location(&self.host.storage, game_ecs::StableId((p.id).0))
                    .expect("location exists")
                    == Location::AtArea(area)
                    && self.host.state.physical.as_ref().is_some_and(|_| {
                        game_physiology::body(&self.host.storage, p.id)
                            .expect("body exists")
                            .vitals
                            .alive
                            && game_physiology::body(&self.host.storage, p.id)
                                .expect("body exists")
                                .vitals
                                .health
                                > 0
                    })
            })
            .map(|(id, _)| *id)
            .take(64)
            .collect()
    }
    fn dispatch_enforcement(
        &mut self,
        actor: PersonId,
        id: EnforcementCaseId,
        proposal: &MissionProposal,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let area = self.enforcement_area(actor).ok_or(Error::Unavailable)?;
        let known = self
            .host
            .state
            .enforcement
            .as_ref()
            .and_then(|w| w.known.get(&actor))
            .and_then(|k| k.get(&id))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let case = self
            .host
            .state
            .enforcement
            .as_ref()
            .and_then(|w| w.cases.get(&id))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let judgment = case.judgment.as_ref().ok_or(Error::Unavailable)?;
        let EnforcementRemedy::Confine { until } = judgment.remedy else {
            return Err(Error::Unavailable);
        };
        let judge = self
            .enforcement_office(case.institution, case.office)
            .ok_or(Error::Unavailable)?;
        let officer = self
            .enforcement_office(case.institution, case.policy.enforcer_office)
            .ok_or(Error::Unavailable)?;
        let contact = known.contact.as_ref().ok_or(Error::Unavailable)?;
        if judge.holder != actor
            || !judge.powers.contains(&OfficePower::Adjudicate)
            || !officer.powers.contains(&OfficePower::Enforce)
            || known.judgment.as_ref() != Some(judgment)
            || !matches!(
                case.status,
                EnforcementStatus::Decided | EnforcementStatus::Refused
            )
            || case.mission.is_some()
            || contact.person != case.target
            || contact.observed_at > owner_now
            || !case.policy.areas.contains(&contact.area)
            || until <= owner_now
            || proposal.leader != officer.holder
            || proposal.home != area
            || proposal.objective
                != (MissionObjective::Confine {
                    person: case.target,
                    until,
                })
            || proposal.deadline < until
            || game_society::institution(&self.host.storage, case.institution)
                .expect("validated Society reference")
                .charter
                .jurisdiction
                != area
            || self
                .host
                .state
                .enforcement
                .as_ref()
                .unwrap()
                .policies
                .get(&case.institution)
                .and_then(|p| p.get(&case.office))
                .is_none_or(|p| p.policy != case.policy)
        {
            return Err(Error::Unavailable);
        }
        let mut budget = game_society::enforcement::DispatchBudget::default();
        for supply in &proposal.supplies {
            *budget.supplies.entry(supply.source).or_default() += u128::from(supply.quantity.0);
        }
        for terms in proposal.compensation.values() {
            for term in terms {
                match *term {
                    AgreementTerm::Delivery {
                        debtor,
                        source,
                        quantity,
                        ..
                    } if debtor == actor => {
                        *budget.spending.entry(source).or_default() += u128::from(quantity.0)
                    }
                    _ => return Err(Error::Unavailable),
                }
            }
        }
        budget.validate(&case.policy)?;
        self.preflight_mission_target_report(
            proposal,
            actor,
            contact.person,
            contact.area,
            contact.observed_at,
        )?;
        self.ensure_event_capacity(128)?;
        let next = self.host.next_id();
        self.mission_command(
            actor,
            &MissionAction::Propose {
                proposal: proposal.clone(),
            },
        )?;
        let mission = self
            .host
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .values()
            .find(|m| m.id.0 >= next && m.sponsor == actor)
            .unwrap()
            .id;
        self.seed_mission_target_report(
            mission,
            actor,
            contact.person,
            contact.area,
            contact.observed_at,
        )?;
        let participants = proposal
            .recruits
            .iter()
            .copied()
            .chain([proposal.leader])
            .collect::<BTreeSet<_>>();
        let record = self
            .host
            .state
            .enforcement
            .as_mut()
            .unwrap()
            .cases
            .get_mut(&id)
            .unwrap();
        record.mission = Some(mission);
        record.status = EnforcementStatus::Offered;
        record.changed_at = owner_now;
        record.notice_area = Some(area);
        record.witnesses = BTreeSet::from([actor]);
        record.explanation="The officer received a funded expedition offer; volunteers and supplies are still required".into();
        let snapshot = self.case_view(
            &self.host.state.enforcement.as_ref().unwrap().cases[&id],
            actor,
            KnowledgeSource::Reported(actor),
        );
        for person in participants {
            self.host
                .state
                .enforcement
                .as_mut()
                .unwrap()
                .known
                .entry(person)
                .or_default()
                .insert(id, snapshot.clone());
        }
        self.emit(EventKind::EnforcementDispatched {
            case: id,
            actor,
            mission,
        });
        Ok(())
    }
    pub(crate) fn observe_enforcement(&mut self) {
        let Some(world) = &self.host.state.enforcement else {
            return;
        };
        let cursor = world.last_event;
        let policies = world
            .policies
            .values()
            .flat_map(|p| p.values())
            .cloned()
            .collect::<Vec<_>>();
        let cases = world.cases.values().cloned().collect::<Vec<_>>();
        let events = self
            .host
            .events()
            .iter()
            .filter(|e| e.sequence > cursor && e.at == self.host.clock().time)
            .cloned()
            .collect::<Vec<_>>();
        for event in events {
            if let EventKind::Struck {
                encounter,
                actor,
                target,
                damage,
            } = event.event
                && let Some(area) = self
                    .host
                    .state
                    .combat
                    .as_ref()
                    .and_then(|w| w.encounters.get(&encounter))
                    .map(|e| e.area)
            {
                for observer in self.local_enforcement_listeners(area) {
                    self.host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .assaults
                        .entry(observer)
                        .or_default()
                        .insert(
                            EventId(event.sequence),
                            WitnessedAssault {
                                event: EventId(event.sequence),
                                attacker: actor,
                                target,
                                area,
                                damage,
                                observed_at: event.at,
                            },
                        );
                }
            }
        }
        self.host.state.enforcement.as_mut().unwrap().last_event = self.host.next_event() - 1;
        let mut by_area = BTreeMap::<AreaId, Vec<PersonId>>::new();
        for person in &self.host.state.people {
            if let Location::AtArea(area) =
                *game_spatial::location(&self.host.storage, game_ecs::StableId((person.id).0))
                    .expect("location exists")
                && self.host.state.physical.as_ref().is_some_and(|_| {
                    game_physiology::body(&self.host.storage, person.id)
                        .expect("body exists")
                        .vitals
                        .alive
                        && game_physiology::body(&self.host.storage, person.id)
                            .expect("body exists")
                            .vitals
                            .health
                            > 0
                })
            {
                by_area.entry(area).or_default().push(person.id);
            }
        }
        for (area, mut people) in by_area {
            people.sort_unstable();
            for actor in people.iter().copied() {
                let observations = people
                    .iter()
                    .copied()
                    .take(64)
                    .map(|person| {
                        (
                            person,
                            ReportedContact {
                                person,
                                area,
                                observed_at: self.host.clock().time,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                for (person, contact) in observations {
                    self.host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .contacts
                        .entry(actor)
                        .or_default()
                        .insert(person, contact);
                }
                for policy in policies.iter().filter(|p| {
                    game_society::institution(&self.host.storage, p.institution)
                        .expect("validated Society reference")
                        .charter
                        .jurisdiction
                        == area
                }) {
                    let mut observed = policy.clone();
                    observed.observed_at = self.host.clock().time;
                    self.host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .known_policies
                        .entry(actor)
                        .or_default()
                        .entry(policy.institution)
                        .or_default()
                        .insert(policy.office, observed);
                }
                // Announced formal decisions are local. Internal expedition outcomes
                // are never copied into a remote court's memory by this observer.
                for case in &cases {
                    if case.changed_at == self.host.clock().time
                        && (case.witnesses.contains(&actor) || case.notice_area == Some(area))
                        && case.status != EnforcementStatus::Filing
                    {
                        let view = self.case_view(case, actor, KnowledgeSource::Observed(actor));
                        self.host
                            .state
                            .enforcement
                            .as_mut()
                            .unwrap()
                            .known
                            .entry(actor)
                            .or_default()
                            .insert(case.id, view);
                    }
                }
                let contacts = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .unwrap()
                    .contacts
                    .get(&actor)
                    .cloned()
                    .unwrap_or_default();
                if let Some(known) = self
                    .host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .known
                    .get_mut(&actor)
                {
                    for view in known.values_mut() {
                        if let Some(contact) = contacts.get(&view.target)
                            && view
                                .contact
                                .as_ref()
                                .is_none_or(|old| old.observed_at < contact.observed_at)
                        {
                            view.contact = Some(contact.clone());
                        }
                    }
                }
                let obsolete = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .unwrap()
                    .known_policies
                    .get(&actor)
                    .into_iter()
                    .flat_map(|institutions| institutions.iter())
                    .filter(|(id, _)| {
                        game_society::institution(&self.host.storage, *id)
                            .expect("validated Society reference")
                            .charter
                            .jurisdiction
                            == area
                    })
                    .map(|(id, known)| {
                        (
                            *id,
                            known
                                .keys()
                                .copied()
                                .filter(|office| {
                                    !self
                                        .host
                                        .state
                                        .enforcement
                                        .as_ref()
                                        .unwrap()
                                        .policies
                                        .get(id)
                                        .is_some_and(|p| p.contains_key(office))
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>();
                for (id, offices) in obsolete {
                    for office in offices {
                        self.host
                            .state
                            .enforcement
                            .as_mut()
                            .unwrap()
                            .known_policies
                            .get_mut(&actor)
                            .unwrap()
                            .get_mut(&id)
                            .unwrap()
                            .remove(&office);
                    }
                }
            }
        }
    }
    fn reconcile_enforcement(&mut self) {
        let owner_now = self.host.clock().time;
        let Some(world) = &self.host.state.enforcement else {
            return;
        };
        let cases = world.cases.values().cloned().collect::<Vec<_>>();
        for case in cases {
            let outcome = if let Some(mission) = case.mission {
                self.host
                    .state
                    .missions
                    .as_ref()
                    .and_then(|w| w.missions.get(&mission))
                    .and_then(|m| match m.status {
                        MissionStatus::Completed => Some(EnforcementStatus::Completed),
                        MissionStatus::Failed | MissionStatus::Cancelled => {
                            Some(EnforcementStatus::Failed)
                        }
                        MissionStatus::Traveling
                        | MissionStatus::Operating
                        | MissionStatus::Returning => Some(EnforcementStatus::Enforcing),
                        MissionStatus::Preparing
                            if m.members.values().all(|p| {
                                matches!(
                                    p.status,
                                    CompanionStatus::Refused | CompanionStatus::Withdrawn
                                )
                            }) =>
                        {
                            Some(EnforcementStatus::Refused)
                        }
                        _ => None,
                    })
            } else if let Some(agreement) = case.settlement {
                if let Some(old) = self.existing_settlement_debt(&case) {
                    self.host
                        .state
                        .agreements
                        .as_ref()
                        .and_then(|w| w.obligations.get(&old))
                        .filter(|o| o.performed >= o.amount())
                        .map(|_| EnforcementStatus::Completed)
                } else {
                    self.host
                        .state
                        .agreements
                        .as_ref()
                        .and_then(|w| w.agreements.get(&agreement))
                        .and_then(|a| match a.status {
                            AgreementStatus::Fulfilled => Some(EnforcementStatus::Completed),
                            AgreementStatus::Rejected | AgreementStatus::Withdrawn => {
                                Some(EnforcementStatus::Refused)
                            }
                            _ => None,
                        })
                }
            } else {
                None
            };
            let Some(outcome) = outcome else {
                continue;
            };
            let changed = outcome != case.status;
            if changed {
                let record = self
                    .host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .cases
                    .get_mut(&case.id)
                    .unwrap();
                record.status = outcome;
                record.changed_at = owner_now;
                record.notice_area = None;
                record.witnesses.clear();
                record.explanation=match outcome {EnforcementStatus::Completed=>"Actual agents or delivery fulfilled the requested task; assent to the court is separate",EnforcementStatus::Refused=>"Named people refused the proposed service or settlement",EnforcementStatus::Failed=>"The physical expedition returned without fulfilling the chosen task",_=>"Consenting officers are undertaking the physical expedition"}.into();
            }
            let actors = self
                .host
                .state
                .enforcement
                .as_ref()
                .unwrap()
                .known
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for actor in actors {
                let informed = if let Some(mission) = case.mission {
                    self.mission_view(actor)
                        .into_iter()
                        .find(|m| m.id == mission)
                        .is_some_and(|m| match outcome {
                            EnforcementStatus::Completed => m.status == MissionStatus::Completed,
                            EnforcementStatus::Failed => {
                                matches!(m.status, MissionStatus::Failed | MissionStatus::Cancelled)
                            }
                            EnforcementStatus::Enforcing => matches!(
                                m.status,
                                MissionStatus::Traveling
                                    | MissionStatus::Operating
                                    | MissionStatus::Returning
                            ),
                            EnforcementStatus::Refused => m.recruitments.iter().all(|r| {
                                matches!(
                                    r.status,
                                    CompanionStatus::Refused | CompanionStatus::Withdrawn
                                )
                            }),
                            _ => false,
                        })
                } else if let Some(agreement) = case.settlement {
                    self.agreement_view(actor)
                        .into_iter()
                        .find(|a| a.id == agreement)
                        .is_some_and(|a| {
                            if let Some(id) = self.existing_settlement_debt(&case) {
                                a.obligations
                                    .iter()
                                    .any(|o| o.id == id && o.performed >= o.amount())
                            } else {
                                a.status == AgreementStatus::Fulfilled
                            }
                        })
                } else {
                    false
                };
                if informed
                    && self.host.state.enforcement.as_ref().unwrap().known[&actor]
                        .get(&case.id)
                        .is_some_and(|known| known.status != outcome)
                {
                    let actual =
                        self.host.state.enforcement.as_ref().unwrap().cases[&case.id].clone();
                    let source = case
                        .mission
                        .and_then(|id| {
                            self.mission_view(actor)
                                .into_iter()
                                .find(|m| m.id == id)
                                .and_then(|m| m.reported_by)
                        })
                        .map_or(KnowledgeSource::Observed(actor), KnowledgeSource::Reported);
                    let mut snapshot = self.case_view(&actual, actor, source);
                    if let Some(old) = self
                        .host
                        .state
                        .enforcement
                        .as_ref()
                        .unwrap()
                        .known
                        .get(&actor)
                        .and_then(|k| k.get(&case.id))
                    {
                        snapshot.contact = old.contact.clone();
                    }
                    self.host
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(actor)
                        .or_default()
                        .insert(case.id, snapshot);
                }
            }
            if changed {
                self.emit(EventKind::EnforcementOutcome {
                    case: case.id,
                    status: outcome,
                });
            }
        }
    }
    fn existing_settlement_debt(&self, case: &Case) -> Option<ObligationId> {
        let EnforcementEvidence::Default(old) = &case.evidence else {
            return None;
        };
        let EnforcementRemedy::Settlement { terms } = &case.judgment.as_ref()?.remedy else {
            return None;
        };
        if terms.len() != 1 {
            return None;
        }
        match (&old.term, &terms[0]) {
            (
                AgreementTerm::Delivery {
                    debtor: a,
                    creditor: b,
                    source: s,
                    destination: d,
                    ..
                },
                AgreementTerm::Delivery {
                    debtor: x,
                    creditor: y,
                    source: u,
                    destination: v,
                    quantity,
                    ..
                },
            ) if a == x
                && b == y
                && s == u
                && d == v
                && quantity.0 == old.amount() - old.performed =>
            {
                Some(old.id)
            }
            _ => None,
        }
    }
    pub(crate) fn review_enforcement(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.reconcile_enforcement();
        self.observe_enforcement();
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.host.state.ruler)
            .collect::<Vec<_>>();
        for actor in actors {
            if self.enforcement_area(actor).is_none()
                || self.past_life_controls_actor(actor)
                || self.in_combat(actor)
                || self.host.state.enforcement.as_ref().is_none_or(|w| {
                    w.talks.contains_key(&actor)
                        || w.reconsider_after
                            .get(&actor)
                            .is_some_and(|t| *t > owner_now)
                })
            {
                continue;
            }
            if !matches!(
                *self.host.storage.activity(actor).expect("activity exists"),
                Activity::Idle | Activity::Rest
            ) {
                continue;
            }
            let commands = self.available_enforcement_commands(actor);
            let personality =
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .unwrap()
                    .personality;
            let chosen = commands.into_iter().find(|command| match command {
                Command::Enforcement {
                    action: EnforcementAction::File { .. },
                } => personality.honesty >= 700,
                Command::Enforcement {
                    action: EnforcementAction::Respond { comply, .. },
                } => *comply == (personality.honesty >= 500),
                Command::Enforcement {
                    action: EnforcementAction::Dispatch { .. },
                } => personality.discipline >= 600,
                _ => false,
            });
            if let Some(Command::Enforcement { action }) = chosen {
                let _ = self.enforcement_command(actor, &action);
                self.host
                    .state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .reconsider_after
                    .insert(actor, SimTime(owner_now.0.saturating_add(180_000)));
            }
        }
        self.propose_known_judgments()?;
        self.review_enforcement_support();
        Ok(())
    }
    fn default_dispatch_proposal(
        &self,
        actor: PersonId,
        case: &EnforcementCaseView,
    ) -> Option<MissionProposal> {
        let EnforcementRemedy::Confine { until } = case.judgment.as_ref()?.remedy else {
            return None;
        };
        let policy = self
            .enforcement_view(actor)
            .policies
            .into_iter()
            .find(|p| p.institution == case.institution && p.office == case.office)?
            .policy;
        let institution = self
            .institution_view(actor)
            .into_iter()
            .find(|i| i.id == case.institution)?;
        let judge = institution
            .offices
            .iter()
            .find(|o| o.mandate.slot == case.office && o.effective && o.mandate.holder == actor)?;
        let _ = judge;
        let leader = institution
            .offices
            .iter()
            .find(|o| o.mandate.slot == policy.enforcer_office && o.effective)?
            .mandate
            .holder;
        let area = self.enforcement_area(actor)?;
        if self.enforcement_area(leader) != Some(area) || self.host.state.missions.is_none() {
            return None;
        }
        let rules = &self.host.state.physical.as_ref()?.rules;
        let supplies = policy
            .supplies
            .iter()
            .filter_map(|(source, cap)| {
                let good = game_cargo::inventory(&self.host.storage, *(source))?.good;
                let desired = match good {
                    Good::Food => rules.meal_size.0.saturating_mul(2),
                    Good::Tool | Good::Binding => 1_000,
                    _ => return None,
                };
                let quantity = Quantity(desired.min(cap.0));
                (quantity.0 > 0).then_some(MissionSupply {
                    carrier: leader,
                    source: *source,
                    quantity,
                })
            })
            .collect();
        let payment = policy
            .spending
            .iter()
            .find(|(source, _)| {
                game_cargo::inventory(&self.host.storage, **source)
                    .is_some_and(|i| i.good == Good::Food)
            })
            .map(|(source, cap)| AgreementTerm::Delivery {
                debtor: actor,
                creditor: leader,
                source: *source,
                destination: self.host.state.people[self.person_index[&leader]].inventory,
                quantity: Quantity(cap.0.min(rules.meal_size.0)),
                deadline: SimTime(self.host.clock().time.0.saturating_add(300_000)),
            });
        let compensation = payment
            .map(|term| BTreeMap::from([(leader, vec![term])]))
            .unwrap_or_default();
        Some(MissionProposal {
            leader,
            recruits: BTreeSet::new(),
            objective: MissionObjective::Confine {
                person: case.target,
                until,
            },
            home: area,
            supplies,
            compensation,
            deadline: SimTime(until.0.checked_add(600_000)?),
        })
    }
    pub(crate) fn available_enforcement_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(area) = self.enforcement_area(actor) else {
            return Vec::new();
        };
        if self.in_combat(actor) {
            return Vec::new();
        }
        let view = self.enforcement_view(actor);
        let mut actions = Vec::new();
        let mut evidence = self
            .property_view(actor)
            .incidents
            .iter()
            .filter(|i| i.actor != actor)
            .take(8)
            .map(|i| EnforcementEvidenceRef::Incident(i.id))
            .collect::<Vec<_>>();
        evidence.extend(
            view.assaults
                .iter()
                .filter(|a| a.attacker != actor)
                .take(8)
                .map(|a| EnforcementEvidenceRef::Assault(a.event)),
        );
        evidence.extend(
            self.agreement_view(actor)
                .iter()
                .flat_map(|a| a.obligations.iter())
                .filter(|o| o.defaulted && o.performed < o.amount())
                .take(8)
                .map(|o| EnforcementEvidenceRef::Default(o.id)),
        );
        evidence.extend(
            self.social_case_view(actor)
                .iter()
                .filter(|c| c.kind == SocialCaseKind::Exclusion)
                .take(8)
                .map(|c| EnforcementEvidenceRef::Social(c.id)),
        );
        for policy in &view.policies {
            let Some(officer) = self.enforcement_office(policy.institution, policy.office) else {
                continue;
            };
            if self.enforcement_area(officer.holder) != Some(area) {
                continue;
            }
            for source in &evidence {
                let Some((topic, target, _, contact)) = self.evidence_for(actor, *source) else {
                    continue;
                };
                let duplicate = view.cases.iter().any(|c| {
                    c.filer == actor
                        && match (&c.evidence, source) {
                            (
                                EnforcementEvidence::Incident(i),
                                EnforcementEvidenceRef::Incident(id),
                            ) => i.id == *id,
                            (
                                EnforcementEvidence::Assault(a),
                                EnforcementEvidenceRef::Assault(id),
                            ) => a.event == *id,
                            (
                                EnforcementEvidence::Default(o),
                                EnforcementEvidenceRef::Default(id),
                            ) => o.id == *id,
                            (
                                EnforcementEvidence::Social(c),
                                EnforcementEvidenceRef::Social(id),
                            ) => c.id == *id,
                            _ => false,
                        }
                });
                let institution =
                    &game_society::institution(&self.host.storage, policy.institution).unwrap();
                if !duplicate
                    && policy.policy.topics.contains(&topic)
                    && self
                        .subjects(institution, &policy.policy.subjects)
                        .contains(&target)
                    && contact.is_none_or(|c| policy.policy.areas.contains(&c.area))
                {
                    actions.push(EnforcementAction::File {
                        institution: policy.institution,
                        office: policy.office,
                        evidence: *source,
                    });
                }
            }
        }
        for case in &view.cases {
            if case.target == actor
                && self
                    .institution_view(actor)
                    .iter()
                    .any(|i| i.id == case.institution && i.charter.jurisdiction == area)
                && let Some(judgment) = &case.judgment
                && matches!(
                    case.status,
                    EnforcementStatus::Decided | EnforcementStatus::Refused
                )
            {
                actions.push(EnforcementAction::Respond {
                    case: case.id,
                    comply: false,
                });
                if matches!(judgment.remedy, EnforcementRemedy::Settlement { .. }) {
                    actions.push(EnforcementAction::Respond {
                        case: case.id,
                        comply: true,
                    });
                }
            }
            if case.mission.is_none()
                && case.contact.is_some()
                && matches!(
                    case.status,
                    EnforcementStatus::Decided | EnforcementStatus::Refused
                )
                && let Some(proposal) = self.default_dispatch_proposal(actor, case)
            {
                actions.push(EnforcementAction::Dispatch {
                    case: case.id,
                    proposal,
                });
            }
            if case.status != EnforcementStatus::Filing {
                for recipient in self
                    .local_enforcement_listeners(area)
                    .into_iter()
                    .filter(|p| *p != actor)
                    .take(4)
                {
                    actions.push(EnforcementAction::Report {
                        case: case.id,
                        recipient,
                    });
                }
            }
        }
        actions
            .into_iter()
            .take(64)
            .map(|action| Command::Enforcement { action })
            .collect()
    }
    fn propose_known_judgments(&mut self) -> Result<(), Error> {
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.host.state.ruler)
            .collect::<Vec<_>>();
        for actor in actors {
            if self.past_life_controls_actor(actor) {
                continue;
            }
            let Some(area) = self.enforcement_area(actor) else {
                continue;
            };
            if self.in_combat(actor) {
                continue;
            }
            for case in self
                .enforcement_view(actor)
                .cases
                .into_iter()
                .filter(|c| c.status == EnforcementStatus::Pending)
                .take(8)
            {
                if self
                    .enforcement_office(case.institution, case.office)
                    .is_none_or(|o| o.holder != actor)
                {
                    continue;
                }
                let Some(institution) = self
                    .institution_view(actor)
                    .into_iter()
                    .find(|i| i.id == case.institution)
                else {
                    continue;
                };
                if institution.charter.jurisdiction!=area || institution.proposals.iter().any(|p|p.status==RuleProposalStatus::Proposed && matches!(p.change,InstitutionChange::Adjudicate {case:id,..} if id==case.id)) {continue;}
                let Some(policy) = self
                    .enforcement_view(actor)
                    .policies
                    .into_iter()
                    .find(|p| p.institution == case.institution && p.office == case.office)
                    .map(|p| p.policy)
                else {
                    continue;
                };
                let mind = &game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    actor,
                )
                .unwrap();
                let remedy = match &case.evidence {
                    EnforcementEvidence::Default(old)
                        if policy.remedies.contains(&RemedyKind::Settlement) =>
                    {
                        if let AgreementTerm::Delivery {
                            debtor,
                            creditor,
                            source,
                            destination,
                            ..
                        } = old.term
                        {
                            Some(EnforcementRemedy::Settlement {
                                terms: vec![AgreementTerm::Delivery {
                                    debtor,
                                    creditor,
                                    source,
                                    destination,
                                    quantity: Quantity(old.amount() - old.performed),
                                    deadline: SimTime(
                                        self.host.clock().time.0.saturating_add(600_000),
                                    ),
                                }],
                            })
                        } else {
                            None
                        }
                    }
                    EnforcementEvidence::Incident(incident)
                        if incident.outcome == IncidentOutcome::Taken
                            && policy.remedies.contains(&RemedyKind::Settlement)
                            && self.host.state.agreements.as_ref().is_some_and(|w| {
                                w.custodians.get(&incident.source) == Some(&case.filer)
                            }) =>
                    {
                        Some(EnforcementRemedy::Settlement {
                            terms: vec![AgreementTerm::Delivery {
                                debtor: case.target,
                                creditor: case.filer,
                                source: incident.destination,
                                destination: incident.source,
                                quantity: incident.quantity,
                                deadline: SimTime(self.host.clock().time.0.saturating_add(600_000)),
                            }],
                        })
                    }
                    _ if policy.remedies.contains(&RemedyKind::Confinement)
                        && mind.personality.aggression >= 600
                        && mind.personality.empathy < 700 =>
                    {
                        Some(EnforcementRemedy::Confine {
                            until: SimTime(self.host.clock().time.0.saturating_add(
                                (u128::from(policy.maximum_confinement.0)
                                    * u128::from(mind.personality.aggression)
                                    / 1_000) as u64,
                            )),
                        })
                    }
                    _ if policy.remedies.contains(&RemedyKind::Dismiss) => {
                        Some(EnforcementRemedy::Dismiss)
                    }
                    _ => None,
                };
                if let Some(remedy) = remedy {
                    let _ = self.institution_command(
                        actor,
                        &InstitutionAction::Propose {
                            institution: case.institution,
                            change: InstitutionChange::Adjudicate {
                                case: case.id,
                                remedy,
                            },
                        },
                    );
                }
            }
        }
        Ok(())
    }
    fn review_enforcement_support(&mut self) {
        let actors = self
            .person_index
            .keys()
            .copied()
            .filter(|p| *p != self.host.state.ruler)
            .collect::<Vec<_>>();
        for actor in actors {
            if self.past_life_controls_actor(actor) || self.enforcement_area(actor).is_none() {
                continue;
            }
            let unpaid = self
                .agreement_view(actor)
                .iter()
                .flat_map(|a| a.obligations.iter())
                .filter(|o| o.defaulted && o.term.parties()[1] == actor && o.performed < o.amount())
                .map(|o| o.term.parties()[0])
                .collect::<BTreeSet<_>>();
            if unpaid.is_empty()
                || game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .unwrap()
                    .personality
                    .honesty
                    < 500
            {
                continue;
            }
            for institution in self.institution_view(actor) {
                for office in institution.offices {
                    if unpaid.contains(&office.mandate.holder) && office.supporters.contains(&actor)
                    {
                        let _ = self.institution_command(
                            actor,
                            &InstitutionAction::SupportOffice {
                                institution: institution.id,
                                office: office.mandate.slot,
                                support: false,
                            },
                        );
                    }
                }
            }
        }
    }
    pub(crate) fn validate_enforcement(&self) -> Result<(), Error> {
        let Some(world) = &self.host.state.enforcement else {
            return Ok(());
        };
        let bad = || {
            Error::InvalidSave(
                "invalid enforcement procedures, evidence, work or personal knowledge".into(),
            )
        };
        self.host.state.institutions.as_ref().ok_or_else(bad)?;
        self.host.state.physical.as_ref().ok_or_else(bad)?;
        if world.last_event >= self.host.next_event() {
            return Err(bad());
        }
        let people = self.person_index.keys().copied().collect::<BTreeSet<_>>();
        let area = |id| self.host.state.areas.iter().any(|a| a.id == id);
        let evidence_valid = |e: &EnforcementEvidence| {
            match e {
            EnforcementEvidence::Incident(i)=>self.host.state.property.as_ref().is_some_and(|_|game_property::incident(&self.host.storage,i.id).is_some()) && people.contains(&i.actor) && game_cargo::inventory(&self.host.storage, i.source).is_some() && game_cargo::inventory(&self.host.storage, i.destination).is_some() && area(i.area) && i.started_at<=i.changed_at && i.changed_at<=i.learned_at && i.learned_at<=self.host.clock().time && i.quantity.0>0,
            EnforcementEvidence::Assault(a)=>people.contains(&a.attacker) && people.contains(&a.target) && area(a.area) && a.observed_at<=self.host.clock().time && self.host.events().iter().any(|e|EventId(e.sequence)==a.event && e.at==a.observed_at && matches!(e.event,EventKind::Struck {actor,target,damage,..} if actor==a.attacker && target==a.target && damage==a.damage)),
            EnforcementEvidence::Default(o)=>self.host.state.agreements.as_ref().is_some_and(|w|w.obligations.contains_key(&o.id)) && o.defaulted && o.performed<o.amount() && o.observed_at<=o.learned_at && o.learned_at<=self.host.clock().time && o.term.parties().iter().all(|p|people.contains(p)),
            EnforcementEvidence::Social(c)=>game_society::case(&self.host.storage,c.id)==Some(c),
        }
        };
        for (id, offices) in &world.policies {
            let institution = game_society::institution(&self.host.storage, id).ok_or_else(bad)?;
            for (office, policy) in offices {
                if policy.institution != *id
                    || policy.office != *office
                    || policy.adopted_at > self.host.clock().time
                    || self
                        .validate_enforcement_policy(institution, *office, &policy.policy)
                        .is_err()
                {
                    return Err(bad());
                }
            }
        }
        for (id, case) in &world.cases {
            let institution =
                game_society::institution(&self.host.storage, case.institution).ok_or_else(bad)?;
            if *id != case.id
                || id.0 >= self.host.next_id()
                || !people.contains(&case.filer)
                || !people.contains(&case.target)
                || !evidence_valid(&case.evidence)
                || case.changed_at > self.host.clock().time
                || case.notice_area.is_some_and(|a| !area(a))
                || !case.witnesses.is_subset(&people)
                || self
                    .validate_enforcement_policy(institution, case.office, &case.policy)
                    .is_err()
                || case.contact.as_ref().is_some_and(|c| {
                    c.person != case.target
                        || !area(c.area)
                        || c.observed_at > self.host.clock().time
                })
            {
                return Err(bad());
            }
            let (topic, target) = match &case.evidence {
                EnforcementEvidence::Incident(i) => (EnforcementTopic::Taking, i.actor),
                EnforcementEvidence::Assault(a) => (EnforcementTopic::Assault, a.attacker),
                EnforcementEvidence::Default(o) => (EnforcementTopic::Default, o.term.parties()[0]),
                EnforcementEvidence::Social(c) => (EnforcementTopic::Exclusion, c.counterpart),
            };
            if topic != case.topic || target != case.target {
                return Err(bad());
            }
            if let Some(judgment) = &case.judgment {
                let proposal = game_society::proposal(&self.host.storage, judgment.proposal)
                    .ok_or_else(bad)?;
                if judgment.id.0 >= self.host.next_id()
                    || judgment.adopted_at > self.host.clock().time
                    || !people.contains(&judgment.proposer)
                    || proposal.status != RuleProposalStatus::Adopted
                    || proposal.proposer != judgment.proposer
                    || !matches!(&proposal.change,InstitutionChange::Adjudicate {case:id,remedy} if *id==case.id && remedy==&judgment.remedy)
                {
                    return Err(bad());
                }
            } else if !matches!(
                case.status,
                EnforcementStatus::Filing
                    | EnforcementStatus::Pending
                    | EnforcementStatus::Withdrawn
            ) {
                return Err(bad());
            }
            if let Some(id) = case.mission {
                let mission = self
                    .host
                    .state
                    .missions
                    .as_ref()
                    .and_then(|w| w.missions.get(&id))
                    .ok_or_else(bad)?;
                if !matches!(case.judgment.as_ref().map(|j|&j.remedy),Some(EnforcementRemedy::Confine {until}) if mission.proposal.objective==MissionObjective::Confine {person:case.target,until:*until})
                {
                    return Err(bad());
                }
            }
            if case.settlement.is_some_and(|id| {
                self.host
                    .state
                    .agreements
                    .as_ref()
                    .is_none_or(|w| !w.agreements.contains_key(&id))
            }) {
                return Err(bad());
            }
        }
        for (actor, talk) in &world.talks {
            if !people.contains(actor)
                || !people.contains(&talk.recipient)
                || !world.cases.contains_key(&talk.case)
                || !area(talk.area)
                || talk.completes <= self.host.clock().time
                || !matches!(*self.host.storage.activity(*actor).expect("body exists"),Activity::EnforcementTalk {case,recipient,completes} if case==talk.case && recipient==talk.recipient && completes==talk.completes)
                || !self
                    .host
                    .state
                    .due
                    .get(&talk.completes)
                    .is_some_and(|due| due.contains(&Due::Physical(*actor)))
            {
                return Err(bad());
            }
            if let Conversation::Report {
                snapshot,
                institution,
            } = &talk.conversation
                && (snapshot.id != talk.case
                    || snapshot.observed_at > snapshot.learned_at
                    || snapshot.learned_at > self.host.clock().time
                    || !evidence_valid(&snapshot.evidence)
                    || institution.id != snapshot.institution
                    || institution.observed_at > self.host.clock().time)
            {
                return Err(bad());
            }
        }
        for (actor, known) in &world.known {
            if !people.contains(actor) {
                return Err(bad());
            }
            for (id, view) in known {
                let actual = world.cases.get(id).ok_or_else(bad)?;
                if *id != view.id
                    || view.institution != actual.institution
                    || view.office != actual.office
                    || view.filer != actual.filer
                    || view.target != actual.target
                    || view.topic != actual.topic
                    || view.evidence != actual.evidence
                    || view
                        .judgment
                        .as_ref()
                        .is_some_and(|j| actual.judgment.as_ref() != Some(j))
                    || view.mission.is_some_and(|id| actual.mission != Some(id))
                    || view
                        .settlement
                        .is_some_and(|id| actual.settlement != Some(id))
                    || match view.source {
                        KnowledgeSource::Observed(observer) => observer != *actor,
                        KnowledgeSource::Reported(speaker) => !people.contains(&speaker),
                        KnowledgeSource::Recorded(_) => true,
                    }
                    || !evidence_valid(&view.evidence)
                    || view.observed_at > view.learned_at
                    || view.learned_at > self.host.clock().time
                    || view.contact.as_ref().is_some_and(|c| {
                        c.person != view.target
                            || !area(c.area)
                            || c.observed_at > self.host.clock().time
                    })
                {
                    return Err(bad());
                }
            }
        }
        if world.contacts.iter().any(|(actor, known)| {
            !people.contains(actor)
                || known.iter().any(|(person, c)| {
                    *person != c.person
                        || !people.contains(person)
                        || !area(c.area)
                        || c.observed_at > self.host.clock().time
                })
        }) || world.assaults.iter().any(|(actor, known)| {
            !people.contains(actor)
                || known.iter().any(|(id, a)| {
                    *id != a.event || !evidence_valid(&EnforcementEvidence::Assault(a.clone()))
                })
        }) || world.reconsider_after.keys().any(|p| !people.contains(p))
        {
            return Err(bad());
        }
        for (actor, known) in &world.known_policies {
            if !people.contains(actor) {
                return Err(bad());
            }
            for (id, offices) in known {
                if !game_society::institution(&self.host.storage, id).is_some()
                    || offices.iter().any(|(slot, p)| {
                        p.institution != *id
                            || p.office != *slot
                            || p.adopted_at > p.observed_at
                            || p.observed_at > self.host.clock().time
                            || self
                                .validate_enforcement_policy(
                                    game_society::institution(&self.host.storage, id)
                                        .expect("validated Society reference"),
                                    *slot,
                                    &p.policy,
                                )
                                .is_err()
                    })
                {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn enforcement_allocation_bound(&self) -> u64 {
        self.host
            .state
            .enforcement
            .as_ref()
            .map_or(0, |_| self.host.state.people.len() as u64 * 128)
    }
    pub(crate) fn enforcement_vote_score(
        &self,
        actor: PersonId,
        _i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> i64 {
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .unwrap();
        match change {
            InstitutionChange::EnforcementPolicy {
                policy: Some(policy),
                ..
            } => {
                i64::from(mind.personality.honesty) - 500
                    + if policy.remedies.contains(&RemedyKind::Confinement) {
                        i64::from(mind.personality.aggression) - i64::from(mind.personality.empathy)
                    } else {
                        0
                    }
            }
            InstitutionChange::EnforcementPolicy { policy: None, .. } => {
                500 - i64::from(mind.personality.honesty)
            }
            InstitutionChange::Adjudicate { case, remedy } => {
                let Some(known) = self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.known.get(&actor))
                    .and_then(|k| k.get(case))
                else {
                    return -10_000;
                };
                let trust = if known.filer == actor {
                    1_000
                } else {
                    mind.relationships.get(&known.filer).map_or(0, |r| r.trust)
                };
                let confidence = match known.evidence {
                    EnforcementEvidence::Default(ref o) if o.defaulted => 500,
                    EnforcementEvidence::Incident(ref i) if i.outcome == IncidentOutcome::Taken => {
                        300
                    }
                    EnforcementEvidence::Assault(_) => 300,
                    _ => -200,
                } + i64::from(trust) / 2;
                match remedy {
                    EnforcementRemedy::Dismiss => -confidence,
                    EnforcementRemedy::Settlement { .. } => {
                        confidence + i64::from(mind.personality.honesty) / 2
                    }
                    EnforcementRemedy::Confine { .. } => {
                        confidence + i64::from(mind.personality.aggression)
                            - i64::from(mind.personality.empathy)
                    }
                    EnforcementRemedy::Depose { replacement, .. } => {
                        confidence
                            + i64::from(mind.relationships.get(replacement).map_or(0, |r| r.trust))
                            - i64::from(
                                mind.relationships.get(&known.target).map_or(0, |r| r.trust),
                            )
                    }
                }
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    fn actor(sim: &Simulation, index: usize) -> PersonId {
        sim.canonical_state().state.people[index].id
    }
    fn court(sim: &Simulation) -> InstitutionId {
        game_society::institutions(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next()
            .unwrap()
    }
    fn role(sim: &Simulation) -> PersonId {
        sim.canonical_state().state.ruler
    }
    fn travel(sim: &mut Simulation, who: PersonId, destination: AreaId) {
        sim.debug_move_actor(who, destination).unwrap();
        let Location::InTransit { arrives, .. } = *game_spatial::location(
            sim.canonical_state().components,
            game_ecs::StableId((who).0),
        )
        .expect("location exists") else {
            panic!("traveler did not depart")
        };
        sim.advance(SimDuration(arrives.0 - sim.canonical_state().time.0))
            .unwrap();
    }
    fn theft(sim: &mut Simulation) -> IncidentId {
        let filer = actor(sim, 3);
        let target = actor(sim, 5);
        travel(sim, filer, AreaId(3));
        travel(sim, target, AreaId(3));
        let source = game_cargo::inventories(sim.canonical_state().components)
            .map(|(_, row)| row)
            .find(|i| {
                i.good == Good::Food
                    && i.location == InventoryLocation::Area(AreaId(3))
                    && i.access == BTreeSet::from([filer])
            })
            .unwrap()
            .id;
        let destination = sim
            .canonical_state()
            .state
            .people
            .iter()
            .find(|p| p.id == target)
            .unwrap()
            .inventory;
        sim.debug_property_action(
            target,
            PropertyAction::Take {
                source,
                destination,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        sim.property_view(filer).incidents.last().unwrap().id
    }
    fn file(sim: &mut Simulation, incident: IncidentId) -> EnforcementCaseId {
        let filer = actor(sim, 3);
        travel(sim, filer, AreaId(1));
        sim.debug_enforcement_action(
            filer,
            EnforcementAction::File {
                institution: court(sim),
                office: 0,
                evidence: EnforcementEvidenceRef::Incident(incident),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        *sim.canonical_state()
            .state
            .enforcement
            .as_ref()
            .unwrap()
            .cases
            .keys()
            .last()
            .unwrap()
    }
    fn decide(sim: &mut Simulation, case: EnforcementCaseId, remedy: EnforcementRemedy) {
        let judge = role(sim);
        let filer = actor(sim, 3);
        let institution = court(sim);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::Adjudicate { case, remedy },
            },
        )
        .unwrap();
        let proposal = game_society::proposals(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for who in [judge, filer] {
            sim.debug_institution_action(
                who,
                InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            )
            .unwrap();
        }
    }

    fn exhausted_command_revision(sim: &Simulation) -> Simulation {
        let mut copy = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        copy.host.publish_clock(game_sim::Clock {
            revision: u64::MAX,
            ..copy.host.clock()
        });
        copy
    }

    #[test]
    fn exhausted_command_revision_cannot_commit_a_vote_without_its_judgment() {
        let mut sim = Simulation::enforcement_fixture(36, Execution::default()).unwrap();
        let incident = theft(&mut sim);
        let case = file(&mut sim, incident);
        let judge = role(&sim);
        let institution = court(&sim);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::Adjudicate {
                    case,
                    remedy: EnforcementRemedy::Dismiss,
                },
            },
        )
        .unwrap();
        let proposal = game_society::proposals(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        sim.debug_institution_action(
            judge,
            InstitutionAction::Vote {
                proposal,
                support: true,
            },
        )
        .unwrap();
        let mut sim = exhausted_command_revision(&sim);
        let before = sim.canonical_snapshot().unwrap();
        let filer = actor(&sim, 3);
        assert_eq!(
            sim.debug_actor_command(
                filer,
                Command::Institution {
                    action: InstitutionAction::Vote {
                        proposal,
                        support: true
                    }
                }
            ),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn exhausted_command_revision_cannot_create_a_promise_without_its_response() {
        let mut sim = Simulation::enforcement_fixture(37, Execution::default()).unwrap();
        let incident = theft(&mut sim);
        let case = file(&mut sim, incident);
        let target = actor(&sim, 5);
        let filer = actor(&sim, 3);
        travel(&mut sim, target, AreaId(1));
        let source = sim
            .canonical_state()
            .state
            .people
            .iter()
            .find(|p| p.id == target)
            .unwrap()
            .inventory;
        let destination = sim
            .canonical_state()
            .state
            .people
            .iter()
            .find(|p| p.id == filer)
            .unwrap()
            .inventory;
        let deadline = SimTime(sim.canonical_state().time.0 + 600_000);
        decide(
            &mut sim,
            case,
            EnforcementRemedy::Settlement {
                terms: vec![AgreementTerm::Delivery {
                    debtor: target,
                    creditor: filer,
                    source,
                    destination,
                    quantity: Quantity(1_000),
                    deadline,
                }],
            },
        );
        let mut sim = exhausted_command_revision(&sim);
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.debug_enforcement_action(target, EnforcementAction::Respond { case, comply: true }),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
}
