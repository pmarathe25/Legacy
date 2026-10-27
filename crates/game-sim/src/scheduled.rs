//! Atomic owner phases over a contiguous scheduled prefix.
use crate::*;
pub(crate) type Wave = Box<dyn Fn(&mut Transaction<'_>, &[Pending]) -> Result<()> + Send + Sync>;
impl Builder {
    /// Complete captured jobs atomically before new work, including urgent wakes.
    /// Owners must preserve events and define deterministic conflict arbitration.
    /// Before-hooks disable the wave and retain ordinary ordered dispatch.
    pub fn scheduled_wave<R: Request<Response = ()>>(
        &mut self,
        limit: usize,
        handler: impl Fn(&mut Transaction<'_>, &[R]) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        require((1..=16384).contains(&limit), "wave limit must be 1..=16384")?;
        require(
            self.registry.request_owners.get(R::KEY) == Some(&self.owner),
            "wave belongs to request owner",
        )?;
        let owner = self.owner;
        self.registry.batches.insert(R::KEY.into(), limit);
        self.registry.waves.insert(
            R::KEY.into(),
            Box::new(move |tx, requests| {
                let requests = requests
                    .iter()
                    .map(|r| r.request::<R>().expect("registered wave type").clone())
                    .collect::<Vec<_>>();
                tx.with_owner(owner, |tx| handler(tx, &requests))
            }),
        );
        Ok(())
    }
}
