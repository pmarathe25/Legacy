use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Join {
    pub institution: InstitutionId,
    pub person: PersonId,
    pub responses: BTreeSet<PersonId>,
}
impl Request for Join {
    const KEY: &'static str = "institutions.join";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Leave {
    pub institution: InstitutionId,
    pub person: PersonId,
}
impl Request for Leave {
    const KEY: &'static str = "institutions.leave";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MembershipChanged {
    pub institution: InstitutionId,
    pub person: PersonId,
    pub joined: bool,
}
impl Event for MembershipChanged {
    const KEY: &'static str = "institutions.membership-changed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ProcedureChanged {
    pub institution: InstitutionId,
    pub previous: Procedure,
    pub procedure: Procedure,
}
impl Event for ProcedureChanged {
    const KEY: &'static str = "institutions.procedure-changed";
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Join>(|tx, r| {
        let group = tx
            .get::<Institutions>()?
            .groups
            .get(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(
            !group.members.is_empty()
                && !group.members.contains(&r.person)
                && r.responses.contains(&r.person),
            "new member must accept",
        )?;
        let sponsors: BTreeSet<_> = r.responses.intersection(&group.members).copied().collect();
        let accepted = match &group.procedure {
            Procedure::Consultation(rule) => rule.passed(
                &group.members,
                &sponsors.iter().map(|p| (*p, true)).collect(),
            ),
            Procedure::Leader(p) => sponsors.contains(p),
            Procedure::Majority => sponsors.len() > group.members.len() / 2,
            Procedure::Personal | Procedure::Unanimity => sponsors == group.members,
        };
        require(accepted, "admission procedure has not accepted")?;
        tx.edit::<Institutions>()?
            .groups
            .get_mut(&r.institution)
            .unwrap()
            .members
            .insert(r.person);
        tx.emit(
            MembershipChanged {
                institution: r.institution,
                person: r.person,
                joined: true,
            },
            r.responses.into_iter().collect(),
        )
    })?;
    b.request::<Leave>(|tx, r| {
        let group = tx
            .edit::<Institutions>()?
            .groups
            .get_mut(&r.institution)
            .ok_or_else(|| Error::Missing("institution".into()))?;
        require(group.members.remove(&r.person), "not a member")?;
        group.offices.retain(|_, p| *p != r.person);
        group.rules.retain(|rule| rule.custodian != r.person);
        for rule in &mut group.rules {
            rule.users.remove(&r.person);
        }
        let previous = group.procedure.clone();
        match &mut group.procedure {
            Procedure::Consultation(rule) => {
                rule.influence.remove(&r.person);
            }
            Procedure::Leader(p) if *p == r.person => group.procedure = Procedure::Personal,
            _ => {}
        }
        let procedure = group.procedure.clone();
        let witnesses = group.members.iter().copied().collect();
        if previous != procedure {
            tx.emit(
                ProcedureChanged {
                    institution: r.institution,
                    previous,
                    procedure,
                },
                witnesses,
            )?;
        }
        tx.emit(
            MembershipChanged {
                institution: r.institution,
                person: r.person,
                joined: false,
            },
            vec![r.person],
        )
    })?;
    Ok(())
}
