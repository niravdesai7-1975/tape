//! `venue` — oracle-priced perps on NEAR. The risk core is plain Rust so every rule is unit-tested; the NEAR contract
//! wrapper (storage, NEP-141 USDC, events, keeper calls) sits on top of it (Nov 1+).
//!
//! Units: prices in micro-USD per coin (4.8605 → 4_860_500); contract sizes in micro-coins (500 NEAR → 500_000_000);
//! money in micro-USD (USDC has 6 decimals). Notional = qty × size × px / 1e6.
//!
//! SCAFFOLDING ONLY before Nov 1: signatures and types are the spec; bodies are `todo!()` so the tests compile and fail.

pub const BP: u128 = 10_000;
pub const MICRO: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Long, Short }

#[derive(Clone, Debug)]
pub struct Market {
    pub id: String,
    pub contract_size: u64,          // micro-coins per contract
    pub tick: u64,                   // micro-USD
    pub initial_margin_bp: u32,      // flat, on the oracle notional
    pub maint_tiers: Vec<(u64, u32)>,// (max position in whole coins, rate bp); past the last → 10_000
    pub taker_fee_bp: u32,
    pub slippage_bp: u32,            // fill = oracle ± max(tick, oracle × slippage_bp), rounded to the tick
    pub max_builder_bp: u32,
}

impl Market {
    /// NEAR as Coinbase margins it (W262/W263): 500 NEAR per contract, 0.001 tick, 42% initial, tiered maintenance, 6 bp.
    pub fn near() -> Market {
        Market {
            id: "NEAR".into(), contract_size: 500 * MICRO as u64, tick: 1_000, initial_margin_bp: 4_200,
            maint_tiers: vec![(4_000, 100), (40_000, 150), (200_000, 200), (400_000, 250), (2_000_000, 500),
                              (4_000_000, 1_000), (5_000_000, 1_250), (6_000_000, 2_500), (8_000_000, 5_000)],
            taker_fee_bp: 6, slippage_bp: 1, max_builder_bp: 10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub owner: String,
    pub beneficiary: Option<String>,
    pub market: String,
    pub side: Side,
    pub qty: u32,
    pub entry_px: u64,
    pub collateral: u128,
    pub stop_px: Option<u64>,
    pub target_px: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Oracle { pub px: u64, pub conf: u64, pub ts: u64, pub replay: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VenueError { BelowInitialMargin, PoolCapacity, BadQty, OracleStale, OracleWide, ReplayNotAllowed,
                      BuilderFeeTooHigh, NotLiquidatable, NotTriggered, BeneficiaryNotAllowed }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger { Stop, Target }

/// What a close pays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settlement { pub exit_px: u64, pub pnl: i128, pub fee: u128, pub bounty: u128, pub to_owner: u128, pub shortfall: u128 }

/// Fill price for a taker: buys pay up, sells receive less; slippage = max(tick, px × slippage_bp), on the tick.
pub fn fill_px(m: &Market, oracle: u64, buy: bool) -> u64 { let _ = (m, oracle, buy); todo!() }
pub fn notional(m: &Market, qty: u32, px: u64) -> u128 { let _ = (m, qty, px); todo!() }
pub fn taker_fee(m: &Market, notional: u128) -> u128 { let _ = (m, notional); todo!() }
pub fn builder_fee(m: &Market, notional: u128, builder_bp: u32) -> Result<u128, VenueError> { let _ = (m, notional, builder_bp); todo!() }
/// Initial margin on the oracle notional; Err(BelowInitialMargin) when collateral is short of it.
pub fn check_initial(m: &Market, qty: u32, oracle: u64, collateral: u128) -> Result<(), VenueError> { let _ = (m, qty, oracle, collateral); todo!() }
/// Maintenance rate for a position of `coins` whole coins.
pub fn maint_rate_bp(m: &Market, coins: u64) -> u32 { let _ = (m, coins); todo!() }
pub fn maintenance(m: &Market, p: &Position, oracle: u64) -> u128 { let _ = (m, p, oracle); todo!() }
pub fn unrealized(m: &Market, p: &Position, oracle: u64) -> i128 { let _ = (m, p, oracle); todo!() }
pub fn equity(m: &Market, p: &Position, oracle: u64) -> i128 { let _ = (m, p, oracle); todo!() }
pub fn liquidatable(m: &Market, p: &Position, oracle: u64) -> bool { let _ = (m, p, oracle); todo!() }
/// Liquidation: closes the whole position at oracle ∓ slippage; bounty = 0.5% of maintenance; equity below zero → shortfall (paid by the pool).
pub fn liquidate(m: &Market, p: &Position, oracle: u64) -> Result<Settlement, VenueError> { let _ = (m, p, oracle); todo!() }
/// Stop or target held by the venue: fires only once the oracle has traded through the level (a condition on price, never time).
pub fn trigger(m: &Market, p: &Position, oracle: u64) -> Result<(Trigger, Settlement), VenueError> { let _ = (m, p, oracle); todo!() }
/// Pool capacity: total open notional after this open ≤ 200% of pool equity.
pub fn check_capacity(open_notional: u128, new_notional: u128, pool_equity: u128) -> Result<(), VenueError> { let _ = (open_notional, new_notional, pool_equity); todo!() }
/// Oracle guards: older than 60 s or confidence wider than 0.5% → reject; a replay price only in demo mode.
pub fn check_oracle(o: &Oracle, now: u64, demo: bool) -> Result<(), VenueError> { let _ = (o, now, demo); todo!() }
/// Adding to a position averages the entry by size.
pub fn add_to(p: &Position, qty: u32, fill: u64, collateral: u128) -> Position { let _ = (p, qty, fill, collateral); todo!() }
/// A beneficiary may be named only by a whitelisted caller (the `guard` contract).
pub fn check_beneficiary(caller: &str, beneficiary: Option<&str>, whitelist: &[&str]) -> Result<(), VenueError> { let _ = (caller, beneficiary, whitelist); todo!() }

#[cfg(test)]
mod tests;
