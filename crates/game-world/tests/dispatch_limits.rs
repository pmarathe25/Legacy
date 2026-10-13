use game_core::{Error, SimTime};
use game_ecs::Schema;
use game_world::{
    GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
};

#[test]
fn sequential_readonly_joins_and_sibling_notifications_have_no_population_limit() {
    struct Sender;
    struct Recipient;
    struct ReadCount;
    impl Request for ReadCount {
        type Response = u64;
    }
    #[derive(Clone)]
    struct Tick;
    impl OwnerSystem<()> for Sender {
        type Runtime = ();
        type Input = usize;
        type Output = ();
        fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, ()>, count: usize) -> Result<(), Error> {
            for _ in 0..count {
                assert_eq!(tx.request(ReadCount)?, 0);
                tx.publish(Tick)?;
            }
            Ok(())
        }
    }
    impl OwnerSystem<()> for Recipient {
        type Runtime = u64;
        type Input = ();
        type Output = ();
        fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, ()>, _: ()) -> Result<(), Error> {
            Ok(())
        }
    }
    fn read_count(
        _: &Recipient,
        tx: &mut OwnerReadTxn<'_, '_, Recipient, ()>,
        _: ReadCount,
    ) -> Result<u64, Error> {
        Ok(*tx.runtime())
    }
    fn receive(
        _: &Recipient,
        tx: &mut OwnerTxn<'_, '_, Recipient, ()>,
        _: Tick,
    ) -> Result<(), Error> {
        *tx.runtime_mut() += 1;
        Ok(())
    }
    let mut host = GenericHost::new(Schema::new(), 1, 0_u64);
    let mut registry = Registry::new();
    registry.register_stateless(Sender).unwrap();
    registry
        .register(
            Recipient,
            RuntimeSlot::new(|state: &u64| state, |state| state),
        )
        .unwrap();
    registry
        .register_read_request::<Recipient, ReadCount>(read_count)
        .unwrap();
    registry
        .register_subscriber::<Recipient, Tick>(receive)
        .unwrap();
    // Every sibling is depth one, even though the total exceeds the old ceiling.
    registry.set_causal_depth_limit(1).unwrap();
    let count = 65_537;
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Sender>(count))
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(host.state, count as u64);
    assert!(host.events().is_empty());
}

#[test]
fn configured_causal_depth_rejects_deferred_and_topic_feedback_without_publication() {
    struct Looper;
    #[derive(Clone)]
    struct Feedback;
    impl Request for Feedback {
        type Response = ();
    }
    impl OwnerSystem<u8> for Looper {
        type Runtime = ();
        type Input = bool;
        type Output = ();
        fn execute(
            &self,
            tx: &mut OwnerTxn<'_, '_, Self, u8>,
            notification: bool,
        ) -> Result<(), Error> {
            if notification {
                tx.publish(Feedback)
            } else {
                tx.defer(Feedback)
            }
        }
    }
    fn feedback(
        _: &Looper,
        tx: &mut OwnerTxn<'_, '_, Looper, u8>,
        _: Feedback,
    ) -> Result<(), Error> {
        tx.emit(9)?;
        // Moving between the two queues must retain the same causal ancestry.
        if tx.next_event() % 2 == 0 {
            tx.defer(Feedback)
        } else {
            tx.publish(Feedback)
        }
    }
    for notification in [false, true] {
        let host = GenericHost::new(Schema::new(), 1, ());
        let mut registry = Registry::new();
        registry.register_stateless(Looper).unwrap();
        registry
            .register_request::<Looper, Feedback>(feedback)
            .unwrap();
        registry
            .register_subscriber::<Looper, Feedback>(feedback)
            .unwrap();
        registry.set_causal_depth_limit(4).unwrap();
        let before = host.snapshot().unwrap();
        let result = registry.prepare(&host, SimTime(1), None, |tx| {
            tx.call::<Looper>(notification)
        });
        assert!(
            matches!(result, Err(Error::InvalidConfiguration(message)) if message.contains("causal depth"))
        );
        assert_eq!(host.snapshot().unwrap(), before);
    }
}
