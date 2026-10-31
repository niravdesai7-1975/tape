//! `guard` — the agent wallet. "Give an agent your money, not your keys."
//! An owner deposits USDC and grants an agent a Policy. The agent can only ask `guard` to trade; `guard` checks the
//! policy, the owner's account and the agent's graded record (from `calls`), and only then calls the venue with
//! `beneficiary = owner`. Money leaves `guard` only to the owner. The policy core is plain Rust and unit-tested; the
//! NEAR contract wrapper (NEP-141 deposits, cross-contract reads of `calls`, venue calls, events) sits on top.
//!
//! Units: micro-USD for money, micro-USD per coin for prices (as in `venue`).

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
pub fn check(p: &Policy, o: &Order, c: &Ctx) -> Result<(), Refusal> {
    if c.revoked {
        return Err(Refusal::Revoked);
    }
    if c.frozen {
        return Err(Refusal::Frozen);
    }
    if !p.markets.contains(&o.market) {
        return Err(Refusal::Market);
    }
    if o.qty > p.max_qty {
        return Err(Refusal::Size);
    }
    if o.collateral > p.max_collateral {
        return Err(Refusal::Collateral);
    }
    if o.notional * 10 > o.collateral * p.max_leverage_x10 as u128 {
        return Err(Refusal::Leverage);
    }
    if c.open_exposure + o.notional > p.max_exposure {
        return Err(Refusal::Exposure);
    }
    check_stop(p, o)?;
    if p.require_call {
        check_call(o, c)?;
    }
    let r = c.record;
    if r.n < p.min_calls || (r.right as u128) * BP < p.min_right_bp as u128 * r.n as u128 {
        return Err(Refusal::Record);
    }
    Ok(())
}

/// The owner's money in `guard`: free balance, collateral out on the venue, the equity peak and the freeze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account { pub owner: String, pub balance: u128, pub locked: u128, pub peak: u128, pub frozen: bool }

impl Account {
    pub fn new(owner: &str) -> Account {
        Account { owner: owner.to_string(), balance: 0, locked: 0, peak: 0, frozen: false }
    }
    /// NEP-141 `ft_on_transfer` from the owner; raises the peak with the equity.
    pub fn deposit(&mut self, amount: u128) {
        self.balance += amount;
        self.peak = self.peak.max(self.balance + self.locked);
    }
    /// Only the owner, only to the owner, only from the free balance.
    pub fn withdraw(&mut self, caller: &str, to: &str, amount: u128) -> Result<(), WalletError> {
        if caller != self.owner {
            return Err(WalletError::NotOwner);
        }
        if to != self.owner {
            return Err(WalletError::ToNotOwner);
        }
        if amount > self.balance {
            return Err(WalletError::Insufficient);
        }
        self.balance -= amount;
        Ok(())
    }
    /// Mark to the current equity (free + collateral + unrealized): the peak follows it up; below
    /// peak × (1 − freeze_bp) the account freezes. A condition on equity, never on a clock.
    pub fn mark(&mut self, equity: u128, freeze_bp: u32) {
        self.peak = self.peak.max(equity);
        if equity * BP < self.peak * (BP - freeze_bp as u128) {
            self.frozen = true;
        }
    }
    /// Only the owner; the peak resets to the current equity.
    pub fn unfreeze(&mut self, caller: &str, equity: u128) -> Result<(), WalletError> {
        if caller != self.owner {
            return Err(WalletError::NotOwner);
        }
        self.frozen = false;
        self.peak = equity;
        Ok(())
    }
}

/// A stop is required (if the owner says so), on the losing side of the fill, and no farther than the owner's max.
fn check_stop(p: &Policy, o: &Order) -> Result<(), Refusal> {
    let Some(stop) = o.stop_px else {
        return if p.stop_required { Err(Refusal::NoStop) } else { Ok(()) };
    };
    let right_side = match o.side { Side::Long => stop < o.fill_px, Side::Short => stop > o.fill_px };
    if !right_side {
        return Err(Refusal::StopSide);
    }
    if let Some(max_bp) = p.max_stop_bp {
        let dist = (o.fill_px as i128 - stop as i128).unsigned_abs();
        if dist * BP > o.fill_px as u128 * max_bp as u128 {
            return Err(Refusal::StopFar);
        }
    }
    Ok(())
}

/// The order must cite this agent's revealed, ungraded call on the same market and side, not yet used by this owner.
fn check_call(o: &Order, c: &Ctx) -> Result<(), Refusal> {
    let Some(call) = c.call else { return Err(Refusal::Call) };
    let ok = o.call_id == Some(call.id) && call.agent == c.agent && call.revealed && !call.graded
        && call.market == o.market && call.side == o.side && !c.call_used_by_owner;
    if ok { Ok(()) } else { Err(Refusal::Call) }
}

#[cfg(test)]
mod tests;
