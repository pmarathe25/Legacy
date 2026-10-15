mod support;
use game_agreements::*;
use game_core::*;
use support::*;

fn offer(f: &Fixture) -> Offer {
    Offer {
        proposer: f.ruler,
        parties: std::collections::BTreeSet::from([f.ruler, f.other]),
        terms: vec![Term {
            debtor: f.ruler,
            creditor: f.other,
            from: f.store,
            to: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
            due: SimTime(1000),
        }],
    }
}
#[test]
fn acceptance_and_default_do_not_teleport_payment() {
    let mut f = setup();
    let id = f.host.dispatch(offer(&f)).unwrap();
    f.host
        .dispatch(Accept {
            person: f.other,
            agreement: id,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.pack), 0);
    run(&mut f.host, SimTime(1000));
    assert_eq!(food(&f.host, f.pack), 0);
    assert_eq!(
        f.host.get::<Agreements>().unwrap().0[&id].overdue,
        std::collections::BTreeSet::from([0])
    );
    f.host
        .dispatch(Fulfill {
            person: f.ruler,
            agreement: id,
            term: 0,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.pack), 10);
    assert_eq!(
        f.host.get::<Agreements>().unwrap().0[&id].status,
        Status::Complete
    );
    assert!(
        f.host
            .dispatch(Fulfill {
                person: f.ruler,
                agreement: id,
                term: 0
            })
            .is_err()
    );
}
#[test]
fn rejection_transfers_nothing() {
    let mut f = setup();
    let id = f.host.dispatch(offer(&f)).unwrap();
    f.host
        .dispatch(Reject {
            person: f.other,
            agreement: id,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 1000);
    assert_eq!(
        f.host.get::<Agreements>().unwrap().0[&id].status,
        Status::Rejected
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
