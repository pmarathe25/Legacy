//! Command forwarding to receiver-owned block adapters.
use super::Engine;
use game_parallel::{
    GoodsExchange, GoodsTransfer, Shipment, StartWork, TradeDecision, TradeOutcome, TradeProposal,
};
impl Engine {
    pub fn institutions(&mut self) -> game_parallel_block_institutions::Context<'_> {
        game_parallel_block_institutions::Context::new(&mut self.template, &self.people)
    }
    pub fn property(&mut self) -> game_parallel_block_property::Context<'_> {
        game_parallel_block_property::Context::new(&mut self.template, &self.people)
    }
    /// Ask a local guardian to release the current incarnation from mentorship.
    pub fn request_guardian_release(
        &mut self,
        guardian: u64,
    ) -> Result<game_parallel::ReleaseDecision, String> {
        self.actions().request_guardian_release(guardian)
    }
    fn actions(&mut self) -> game_parallel_block_actions::Context<'_> {
        game_parallel_block_actions::Context {
            world: &mut self.template,
            people: &self.people,
        }
    }
    fn production_actions(&mut self) -> game_parallel_block_production::Context<'_> {
        game_parallel_block_production::Context {
            world: &mut self.template,
            people: &self.people,
            packed: &mut self.packed_jobs,
        }
    }
    fn goods_actions(&mut self) -> game_parallel_block_goods::Context<'_> {
        game_parallel_block_goods::Context {
            world: &mut self.template,
            people: &self.people,
            packed: &mut self.packed_goods,
        }
    }
    pub fn invoke_lives(&mut self, lives: &[u64]) -> Result<(), String> {
        self.actions().invoke(lives)
    }
    pub fn dismiss_invocation(&mut self) -> Result<(), String> {
        self.actions().dismiss_invocation()
    }
    pub fn accept_invocation_warning(&mut self, revision: u64) -> Result<(), String> {
        self.actions().accept_invocation_warning(revision)
    }
    pub fn borrowed_memories(
        &mut self,
    ) -> Result<Vec<(u64, game_parallel::KnowledgeRecord)>, String> {
        self.actions().borrowed_memories()
    }
    pub fn start_work(&mut self, requests: &[StartWork]) -> Result<(), String> {
        self.production_actions().start(requests)
    }
    pub fn cancel_work(&mut self, id: u64) -> Result<(), String> {
        self.production_actions().cancel(id)
    }
    pub fn reassign_work(&mut self, id: u64, worker: u64) -> Result<(), String> {
        self.production_actions().reassign(id, worker)
    }
    pub fn transfer_goods(&mut self, requests: &[GoodsTransfer]) -> Result<(), String> {
        let busy = self.production_actions().busy_people();
        self.goods_actions().transfer(requests, &busy)
    }
    pub fn exchange_goods(&mut self, requests: &[GoodsExchange]) -> Result<(), String> {
        let busy = self.production_actions().busy_people();
        self.goods_actions().exchange(requests, &busy)
    }
    pub fn recover_goods(
        &mut self,
        actor: u64,
        deceased: u64,
        good: u32,
        quantity: u64,
    ) -> Result<(), String> {
        let busy = self.production_actions().busy_people();
        self.goods_actions()
            .recover(actor, deceased, good, quantity, &busy)
    }
    pub fn propose_trade(&mut self, proposal: TradeProposal) -> Result<u64, String> {
        self.production_actions().propose(proposal)
    }
    pub fn propose_trades(&mut self, proposals: &[TradeProposal]) -> Result<Vec<u64>, String> {
        self.production_actions().propose_batch(proposals)
    }
    pub fn decide_trade(
        &mut self,
        id: u64,
        actor: u64,
        outcome: TradeOutcome,
    ) -> Result<(), String> {
        self.production_actions().decide(id, actor, outcome)
    }
    pub fn decide_trades(&mut self, decisions: &[TradeDecision]) -> Result<(), String> {
        self.production_actions().decide_batch(decisions)
    }
    pub fn launch_shipments(&mut self, requests: &[Shipment]) -> Result<(), String> {
        game_parallel_block_logistics::launch(&mut self.template, &mut self.food, requests)
    }
    fn food_actions(&mut self) -> game_parallel_block_food_cargo::Context<'_> {
        game_parallel_block_food_cargo::Context {
            world: &mut self.template,
            food: &mut self.food,
            blocks: &mut self.blocks,
            people: &mut self.people,
            positions: &self.positions,
        }
    }
    pub fn pickup_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        self.food_actions().pickup(person, amount)
    }
    pub fn deposit_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        self.food_actions().deposit(person, person, amount, false)
    }
    pub fn recover_food(&mut self, actor: u64, deceased: u64, amount: u32) -> Result<(), String> {
        self.food_actions().deposit(actor, deceased, amount, true)
    }
    pub fn eat_carried_food(&mut self, person: u64) -> Result<(), String> {
        self.food_actions().eat(person)
    }
}
