//! Attach the transaction boundary to the composed host.
#[macro_export]
macro_rules! prepare_methods {
    () => {
        pub fn step(&mut self) -> Result<(), &'static str> {
            let draft = self.prepare()?;
            self.commit(draft)
        }
        pub fn prepare(&mut self) -> Result<Draft, &'static str> {
            self.prepare_inner(None)
        }
        /// Attempt one consent-based conversation and advance one real tick.
        pub fn converse(&mut self, actor: u64, target: u64) -> Result<bool, &'static str> {
            if self.template.lineage.as_ref().is_some_and(|lineage| {
                lineage.current == Some(actor)
                    && lineage.controlled_person(self.template.tick) != Some(actor)
            }) {
                return Err("player control is suspended");
            }
            let draft = self.prepare_inner(Some((actor, target)))?;
            let accepted = draft
                .decisions
                .conversation_accepted
                .ok_or("conversation was not prepared")?;
            self.commit(draft)?;
            Ok(accepted)
        }
        fn prepare_inner(
            &mut self,
            conversation: Option<(u64, u64)>,
        ) -> Result<Draft, &'static str> {
            self.prepared = self
                .prepared
                .checked_add(1)
                .ok_or("prepare counter overflow")?;
            game_parallel_fused_departure::compact_if_needed(
                &mut self.blocks,
                &mut self.jobs,
                &mut self.waiting,
                self.live_count,
            )?;
            let next = self.template.tick.checked_add(1).ok_or("tick overflow")?;
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            let mut decisions = game_parallel_fused_decisions::prepare(
                &mut self.template,
                &self.blocks,
                &self.positions,
                &self.jobs,
                &self.food,
                (next, conversation),
                (
                    self.owners.conflict.as_mut(),
                    self.owners.knowledge.as_mut(),
                    self.owners.social.as_mut(),
                    self.owners.autonomy.as_mut(),
                    self.owners.family.as_ref(),
                ),
            )?;
            let state = game_parallel_fused_tick::Context {
                family: self.owners.family.as_mut(),
                world: &self.template,
                blocks: &mut self.blocks,
                positions: &self.positions,
                jobs: &self.jobs,
                waiting: &self.waiting,
                food: &self.food,
            }
            .prepare(next, &mut decisions)?;
            let live_count = self
                .live_count
                .checked_sub(state.delivered)
                .ok_or("invalid live job count")?
                .checked_add(decisions.autowork.as_ref().map_or(0, |draft| draft.count))
                .ok_or("job count overflow")?;
            Ok(Draft {
                tick: next,
                state,
                decisions,
                live_count,
                revision,
                instance: self.instance,
                prepared: self.prepared,
            })
        }
    };
}
