//! `guard` — the agent wallet. "Give an agent your money, not your keys."
//! An owner deposits USDC and grants an agent a Policy. The agent can only ask `guard` to trade; `guard` checks the
//! policy, the owner's account and the agent's graded record (from `calls`), and only then calls the venue with
//! `beneficiary = owner`. Money leaves `guard` only to the owner. The policy core is plain Rust and unit-tested; the
//! NEAR contract wrapper (NEP-141 deposits, cross-contract reads of `calls`, venue calls, events) sits on top (Nov 1+).
//!
//! Units: micro-USD for money, micro-USD per coin for prices (as in `venue`).
//! SCAFFOLDING ONLY before Nov 1: signatures and types are the spec; bodies are `todo!()` so the tests compile and fail.

pub const BP: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Long, Short }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub markets: Vec<String>,
    pub max_qty: u32,
    pub max_collateral: u128,
    pub max_exposure: u128,          // open notional across the grant, including this order
    pub max_leverage_x10: u32,       // notional ÷ collateral, ×10 (20 = 2.0x)
    pub stop_required: bool,
    pub max_stop_bp: Option<u32>,    // the owner's choice; None = any stop on the right side
    pub drawdown_freeze_bp: u32,     // freeze when equity < peak × (1 − this)
    pub min_calls: u32,
    pub min_right_bp: u32,
    pub window_n: u32,
    pub require_call: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order { pub market: String, pub side: Side, pub qty: u32, pub collateral: u128, pub notional: u128,
                   pub fill_px: u64, pub stop_px: Option<u64>, pub call_id: Option<u64> }

/// The agent's graded record over its last `window_n` calls, as `calls.record(agent, window_n)` returns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record { pub n: u32, pub right: u32 }

/// What `guard` needs to know about a cited call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallView { pub id: u64, pub agent: String, pub market: String, pub side: Side, pub revealed: bool, pub graded: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal { Revoked, Frozen, Market, Size, Collateral, Leverage, Exposure, NoStop, StopSide, StopFar, Call, Record }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletError { NotOwner, ToNotOwner, Insufficient }

/// Everything the check reads, gathered by the contract before it decides.
pub struct Ctx<'a> {
    pub agent: &'a str,
    pub revoked: bool,
    pub frozen: bool,
    pub open_exposure: u128,
    pub record: Record,
    pub call: Option<&'a CallView>,
    pub call_used_by_owner: bool,
}

/// The policy engine. Checks in a fixed order and returns the FIRST refusal:
/// revoked, frozen, market, size, collateral, leverage, exposure, stop (none / wrong side / too far), call, record.
pub fn check(p: &Policy, o: &Order, c: &Ctx) -> Result<(), Refusal> { let _ = (p, o, c); todo!() }

/// The owner's money in `guard`: free balance, collateral out on the venue, the equity peak and the freeze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account { pub owner: String, pub balance: u128, pub locked: u128, pub peak: u128, pub frozen: bool }

impl Account {
    pub fn new(owner: &str) -> Account { let _ = owner; todo!() }
    /// NEP-141 `ft_on_transfer` from the owner; raises the peak with the equity.
    pub fn deposit(&mut self, amount: u128) { let _ = amount; todo!() }
    /// Only the owner, only to the owner, only from the free balance.
    pub fn withdraw(&mut self, caller: &str, to: &str, amount: u128) -> Result<(), WalletError> { let _ = (caller, to, amount); todo!() }
    /// Mark to the current equity (free + collateral + unrealized): the peak follows it up; below
    /// peak × (1 − freeze_bp) the account freezes. A condition on equity, never on a clock.
    pub fn mark(&mut self, equity: u128, freeze_bp: u32) { let _ = (equity, freeze_bp); todo!() }
    /// Only the owner; the peak resets to the current equity.
    pub fn unfreeze(&mut self, caller: &str, equity: u128) -> Result<(), WalletError> { let _ = (caller, equity); todo!() }
}

#[cfg(test)]
mod tests;
