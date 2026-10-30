//! Typed commands for property ownership.
use game_parallel::World;
use game_parallel_production_model::PersonSnapshot;
#[derive(Clone, Copy)]
pub enum Command {
    Offer {
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    },
    Decide {
        id: u64,
        actor: u64,
        accept: bool,
    },
    Grant {
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    },
    Revoke {
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    },
    Withdraw {
        id: u64,
        actor: u64,
    },
}
impl Command {
    pub fn actor(&self) -> u64 {
        match *self {
            Self::Offer { owner, .. } | Self::Grant { owner, .. } | Self::Revoke { owner, .. } => {
                owner
            }
            Self::Decide { actor, .. } | Self::Withdraw { actor, .. } => actor,
        }
    }
}
pub fn execute(
    world: &mut World,
    people: &[PersonSnapshot],
    command: Command,
) -> Result<Option<u64>, String> {
    let mut ctx = crate::Context::new(world, people);
    match command {
        Command::Offer {
            owner,
            recipient,
            region,
            good,
        } => ctx.offer(owner, recipient, region, good).map(Some),
        Command::Decide { id, actor, accept } => ctx.decide(id, actor, accept).map(|()| None),
        Command::Grant {
            owner,
            recipient,
            region,
            good,
        } => ctx.grant(owner, recipient, region, good).map(|()| None),
        Command::Revoke {
            owner,
            recipient,
            region,
            good,
        } => ctx.revoke(owner, recipient, region, good).map(|()| None),
        Command::Withdraw { id, actor } => ctx.withdraw(id, actor).map(|()| None),
    }
}
