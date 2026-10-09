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
    fn enforcement_world(&mut self) {
        if self.state.enforcement.is_none() {
            self.state.enforcement = Some(World {
                last_event: self.state.next_event - 1,
                ..World::default()
            });
        }
    }
    fn enforcement_area(&self, actor: PersonId) -> Option<AreaId> {
        let person = self.state.people.get(*self.person_index.get(&actor)?)?;
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || !self.controls_actor(actor)
            || self.combat_restricts(actor)
        {
            return None;
        }
        match person.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn enforcement_office(
        &self,
        institution: InstitutionId,
        office: u16,
    ) -> Option<&OfficeMandate> {
        let i = self
            .state
            .institutions
            .as_ref()?
            .institutions
            .get(&institution)?;
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
                .any(|id| !self.state.areas.iter().any(|a| a.id == *id))
            || !self.validate_subjects(&policy.subjects, &i.charter.offices)
            || policy.spending.len() > 16
            || policy.supplies.len() > 16
            || policy.remedies.contains(&RemedyKind::Confinement)
                && policy.maximum_confinement.0 == 0
            || policy.spending.iter().any(|(source, q)| {
                q.0 == 0
                    || !judge.inventories.contains(source)
                    || self
                        .state
                        .inventories
                        .get(source)
                        .is_none_or(|i| q > &i.capacity)
            })
            || policy.supplies.iter().any(|(source, q)| {
                q.0 == 0
                    || !enforcer.inventories.contains(source)
                    || self
                        .state
                        .inventories
                        .get(source)
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
                        if *until <= self.state.time
                            || until.0 - self.state.time.0 > case.policy.maximum_confinement.0
                            || self.state.combat.is_none()
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
                        if terms.is_empty() || terms.len()>8 || terms.iter().any(|term| !matches!(term,AgreementTerm::Delivery {debtor,quantity,deadline,..} if *debtor==case.target && quantity.0>0 && *deadline>self.state.time)) {return Err(Error::Unavailable);}
                        for term in terms {
                            if let AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } = term
                                && self
                                    .state
                                    .inventories
                                    .get(source)
                                    .zip(self.state.inventories.get(destination))
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
                .state
                .enforcement
                .as_ref()
                .and_then(|w| w.cases.get(case))
            else {
                return false;
            };
            self.state
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
    pub(crate) fn adopt_enforcement_change(
        &mut self,
        institution: InstitutionId,
        proposal: RuleProposalId,
    ) -> Result<(), Error> {
        let change = self.state.institutions.as_ref().unwrap().proposals[&proposal].clone();
        match change.change {
            InstitutionChange::EnforcementPolicy { office, policy } => {
                self.enforcement_world();
                let policies = self
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
                            adopted_at: self.state.time,
                            observed_at: self.state.time,
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
                let judgment = JudgmentId(allocate(&mut self.state)?);
                let now = self.state.time;
                let record = self
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
                record.explanation =
                    "The institution adopted a remedy; physical compliance remains unresolved"
                        .into();
                if let EnforcementRemedy::Depose {
                    office,
                    replacement,
                } = remedy
                {
                    let i = self
                        .state
                        .institutions
                        .as_mut()
                        .unwrap()
                        .institutions
                        .get_mut(&institution)
                        .unwrap();
                    i.charter
                        .offices
                        .iter_mut()
                        .find(|o| o.slot == office)
                        .unwrap()
                        .holder = replacement;
                    i.office_support.insert(
                        office,
                        change
                            .votes
                            .iter()
                            .filter(|(actor, yes)| **yes && i.members.contains(actor))
                            .map(|(actor, _)| *actor)
                            .collect(),
                    );
                    let case = self
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
            learned_at: self.state.time,
            source: if matches!(source, KnowledgeSource::Observed(_)) {
                KnowledgeSource::Observed(actor)
            } else {
                source
            },
        }
    }
    pub(crate) fn enforcement_view(&self, actor: PersonId) -> EnforcementView {
        let Some(world) = &self.state.enforcement else {
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
                let i = self
                    .state
                    .institutions
                    .as_ref()
                    .unwrap()
                    .institutions
                    .get(institution)
                    .ok_or(Error::Unavailable)?;
                let officer = self
                    .enforcement_office(*institution, *office)
                    .ok_or(Error::Unavailable)?;
                let recipient = officer.holder;
                if i.charter.jurisdiction != area
                    || self.enforcement_area(recipient) != Some(area)
                    || recipient == actor
                    || !officer.powers.contains(&OfficePower::Adjudicate)
                    || !known.policy.topics.contains(&topic)
                    || !self.subjects(i, &known.policy.subjects).contains(&target)
                    || contact
                        .as_ref()
                        .is_some_and(|c| !known.policy.areas.contains(&c.area))
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(MINUTE.0)
                        .ok_or(Error::Overflow)?,
                );
                self.ensure_event_capacity(2)?;
                let id = EnforcementCaseId(allocate(&mut self.state)?);
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
                    changed_at: self.state.time,
                };
                let view = self.case_view(&case, actor, KnowledgeSource::Observed(actor));
                let world = self.state.enforcement.as_mut().unwrap();
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
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(MINUTE.0)
                        .ok_or(Error::Overflow)?,
                );
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
                    .state
                    .enforcement
                    .as_ref()
                    .and_then(|w| w.known.get(&actor))
                    .and_then(|k| k.get(case))
                    .ok_or(Error::Unavailable)?;
                let actual = self
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
                    || self.state.institutions.as_ref().unwrap().institutions[&actual.institution]
                        .charter
                        .jurisdiction
                        != area
                {
                    return Err(Error::Unavailable);
                }
                let mut settlement = None;
                if *comply {
                    if let EnforcementRemedy::Settlement { terms } = &judgment.remedy {
                        if let EnforcementEvidence::Default(old) = &actual.evidence {
                            let debt = self
                                .state
                                .agreements
                                .as_ref()
                                .unwrap()
                                .obligations
                                .get(&old.id)
                                .ok_or(Error::Unavailable)?;
                            settlement = Some(debt.agreement);
                        } else {
                            let parties = terms
                                .iter()
                                .flat_map(|t| t.parties())
                                .collect::<BTreeSet<_>>();
                            let next = self.state.next_id;
                            self.agreement_command(
                                actor,
                                &AgreementAction::Propose {
                                    parties,
                                    terms: terms.clone(),
                                    procedure: None,
                                },
                            )?;
                            settlement = self
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
                record.changed_at = self.state.time;
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
        self.revision += 1;
        Ok(())
    }
    fn start_enforcement_talk(&mut self, actor: PersonId, talk: Talk) {
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::EnforcementTalk {
            case: talk.case,
            recipient: talk.recipient,
            completes: talk.completes,
        };
        self.state
            .due
            .entry(talk.completes)
            .or_default()
            .push(Due::Physical(actor));
        self.state
            .enforcement
            .as_mut()
            .unwrap()
            .talks
            .insert(actor, talk);
    }
    pub(crate) fn interrupt_enforcement(&mut self, actor: PersonId) {
        let Some(talk) = self
            .state
            .enforcement
            .as_mut()
            .and_then(|w| w.talks.remove(&actor))
        else {
            return;
        };
        if matches!(talk.conversation, Conversation::File) {
            let case = self
                .state
                .enforcement
                .as_mut()
                .unwrap()
                .cases
                .get_mut(&talk.case)
                .unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = self.state.time;
            case.explanation =
                "The complaint was interrupted before the officer received it".into();
        }
    }
    pub(crate) fn complete_enforcement_talk(&mut self, actor: PersonId) {
        let Some(talk) = self
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
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .cases
                        .get_mut(&talk.case)
                        .unwrap();
                    record.status = EnforcementStatus::Pending;
                    record.changed_at = self.state.time;
                    record.explanation =
                        "A complaint reached the officer; the accusation has not been decided"
                            .into();
                    let case = record.clone();
                    let listeners = self.local_enforcement_listeners(talk.area);
                    for listener in listeners {
                        let snapshot =
                            self.case_view(&case, listener, KnowledgeSource::Reported(actor));
                        self.state
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
                    snapshot.learned_at = self.state.time;
                    snapshot.source = KnowledgeSource::Reported(actor);
                    let known = self
                        .state
                        .enforcement
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(talk.recipient)
                        .or_default();
                    if known
                        .get(&snapshot.id)
                        .is_none_or(|old| old.observed_at <= snapshot.observed_at)
                    {
                        known.insert(snapshot.id, *snapshot);
                    }
                    if let Some(memory) = self
                        .state
                        .institutions
                        .as_mut()
                        .unwrap()
                        .memories
                        .get_mut(&talk.recipient)
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
                .state
                .enforcement
                .as_mut()
                .unwrap()
                .cases
                .get_mut(&talk.case)
                .unwrap();
            case.status = EnforcementStatus::Withdrawn;
            case.changed_at = self.state.time;
            case.explanation = "The officer could no longer hear the complaint".into();
        }
        if let Some(body) = self
            .state
            .physical
            .as_mut()
            .and_then(|w| w.actors.get_mut(&actor))
        {
            body.activity = Activity::Idle;
        }
    }
    fn local_enforcement_listeners(&self, area: AreaId) -> Vec<PersonId> {
        self.state
            .people
            .iter()
            .filter(|p| {
                p.location == Location::AtArea(area)
                    && self.state.physical.as_ref().is_some_and(|w| {
                        w.actors[&p.id].vitals.alive && w.actors[&p.id].vitals.health > 0
                    })
            })
            .map(|p| p.id)
            .take(64)
            .collect()
    }
    fn dispatch_enforcement(
        &mut self,
        _actor: PersonId,
        _case: EnforcementCaseId,
        _proposal: &MissionProposal,
    ) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
    pub(crate) fn observe_enforcement(&mut self) {
        let Some(world) = &self.state.enforcement else {
            return;
        };
        let policies = world
            .policies
            .values()
            .flat_map(|p| p.values())
            .cloned()
            .collect::<Vec<_>>();
        for actor in self.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            let Some(area) = self.enforcement_area(actor) else {
                continue;
            };
            for person in self
                .state
                .people
                .iter()
                .filter(|p| p.location == Location::AtArea(area))
            {
                self.state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .contacts
                    .entry(actor)
                    .or_default()
                    .insert(
                        person.id,
                        ReportedContact {
                            person: person.id,
                            area,
                            observed_at: self.state.time,
                        },
                    );
            }
            for policy in policies.iter().filter(|p| {
                self.state.institutions.as_ref().unwrap().institutions[&p.institution]
                    .charter
                    .jurisdiction
                    == area
            }) {
                let mut policy = policy.clone();
                policy.observed_at = self.state.time;
                self.state
                    .enforcement
                    .as_mut()
                    .unwrap()
                    .known_policies
                    .entry(actor)
                    .or_default()
                    .entry(policy.institution)
                    .or_default()
                    .insert(policy.office, policy);
            }
        }
    }
    pub(crate) fn review_enforcement(&mut self) -> Result<(), Error> {
        Ok(())
    }
    pub(crate) fn available_enforcement_commands(&self, _actor: PersonId) -> Vec<Command> {
        Vec::new()
    }
    pub(crate) fn validate_enforcement(&self) -> Result<(), Error> {
        let Some(world) = &self.state.enforcement else {
            return Ok(());
        };
        if self.state.institutions.is_none()
            || self.state.physical.is_none()
            || world.last_event >= self.state.next_event
        {
            return Err(Error::InvalidSave("invalid enforcement state".into()));
        }
        Ok(())
    }
    pub(crate) fn enforcement_allocation_bound(&self) -> u64 {
        self.state
            .enforcement
            .as_ref()
            .map_or(0, |_| self.state.people.len() as u64 * 128)
    }
    pub(crate) fn enforcement_vote_score(
        &self,
        actor: PersonId,
        _i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> i64 {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
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
