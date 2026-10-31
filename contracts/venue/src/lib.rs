//! `venue` — oracle-priced perps on NEAR. The risk core is plain Rust so every rule is unit-tested; the NEAR contract
//! wrapper (storage, NEP-141 USDC, events, keeper calls) sits on top of it.
//!
//! Units: prices in micro-USD per coin (4.8605 → 4_860_500); contract sizes in micro-coins (500 NEAR → 500_000_000);
//! money in micro-USD (USDC has 6 decimals). Notional = qty × size × px / 1e6.
//!

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
pub fn fill_px(m: &Market, oracle: u64, buy: bool) -> u64 {
    let slip = core::cmp::max(m.tick as u128, oracle as u128 * m.slippage_bp as u128 / BP) as u64;
    let t = m.tick;
    if buy { (oracle + slip).div_ceil(t) * t } else { (oracle.saturating_sub(slip)) / t * t }
}
pub fn notional(m: &Market, qty: u32, px: u64) -> u128 {
    qty as u128 * m.contract_size as u128 * px as u128 / MICRO
}
pub fn taker_fee(m: &Market, notional: u128) -> u128 {
    notional * m.taker_fee_bp as u128 / BP
}
pub fn builder_fee(m: &Market, notional: u128, builder_bp: u32) -> Result<u128, VenueError> {
    if builder_bp > m.max_builder_bp {
        return Err(VenueError::BuilderFeeTooHigh);
    }
    Ok(notional * builder_bp as u128 / BP)
}
/// Initial margin on the oracle notional; Err(BelowInitialMargin) when collateral is short of it.
pub fn check_initial(m: &Market, qty: u32, oracle: u64, collateral: u128) -> Result<(), VenueError> {
    if collateral * BP >= notional(m, qty, oracle) * m.initial_margin_bp as u128 { Ok(()) } else { Err(VenueError::BelowInitialMargin) }
}
/// Maintenance rate for a position of `coins` whole coins.
pub fn maint_rate_bp(m: &Market, coins: u64) -> u32 {
    m.maint_tiers.iter().find(|(max, _)| coins <= *max).map(|(_, r)| *r).unwrap_or(10_000)
}
pub fn maintenance(m: &Market, p: &Position, oracle: u64) -> u128 {
    notional(m, p.qty, oracle) * maint_rate_bp(m, coins(m, p.qty)) as u128 / BP
}
pub fn unrealized(m: &Market, p: &Position, oracle: u64) -> i128 {
    let per_coin = oracle as i128 - p.entry_px as i128;
    let v = per_coin * coins(m, p.qty) as i128;
    match p.side { Side::Long => v, Side::Short => -v }
}
pub fn equity(m: &Market, p: &Position, oracle: u64) -> i128 {
    p.collateral as i128 + unrealized(m, p, oracle)
}
pub fn liquidatable(m: &Market, p: &Position, oracle: u64) -> bool {
    equity(m, p, oracle) < maintenance(m, p, oracle) as i128
}
/// Liquidation: closes the whole position at oracle ∓ slippage; bounty = 0.5% of maintenance; equity below zero → shortfall (paid by the pool).
pub fn liquidate(m: &Market, p: &Position, oracle: u64) -> Result<Settlement, VenueError> {
    if !liquidatable(m, p, oracle) {
        return Err(VenueError::NotLiquidatable);
    }
    let bounty = maintenance(m, p, oracle) * LIQ_BOUNTY_BP / BP;
    Ok(settle(m, p, oracle, bounty))
}
/// Stop or target held by the venue: fires only once the oracle has traded through the level (a condition on price, never time).
pub fn trigger(m: &Market, p: &Position, oracle: u64) -> Result<(Trigger, Settlement), VenueError> {
    let long = p.side == Side::Long;
    if let Some(stop) = p.stop_px {
        if (long && oracle <= stop) || (!long && oracle >= stop) {
            // a gapped stop fills at the market, never at a level the price skipped
            let at = if long { oracle.min(stop) } else { oracle.max(stop) };
            return Ok((Trigger::Stop, settle(m, p, at, 0)));
        }
    }
    if let Some(target) = p.target_px {
        if (long && oracle >= target) || (!long && oracle <= target) {
            return Ok((Trigger::Target, settle(m, p, target, 0)));
        }
    }
    Err(VenueError::NotTriggered)
}
/// Pool capacity: total open notional after this open ≤ 200% of pool equity.
pub fn check_capacity(open_notional: u128, new_notional: u128, pool_equity: u128) -> Result<(), VenueError> {
    if open_notional + new_notional <= pool_equity * MAX_OI_X { Ok(()) } else { Err(VenueError::PoolCapacity) }
}
/// Oracle guards: older than 60 s or confidence wider than 0.5% → reject; a replay price only in demo mode.
pub fn check_oracle(o: &Oracle, now: u64, demo: bool) -> Result<(), VenueError> {
    if now.saturating_sub(o.ts) > ORACLE_MAX_AGE_S {
        return Err(VenueError::OracleStale);
    }
    if o.conf as u128 * BP > o.px as u128 * ORACLE_MAX_CONF_BP {
        return Err(VenueError::OracleWide);
    }
    if o.replay && !demo {
        return Err(VenueError::ReplayNotAllowed);
    }
    Ok(())
}
/// Adding to a position averages the entry by size.
pub fn add_to(p: &Position, qty: u32, fill: u64, collateral: u128) -> Position {
    let total = p.qty + qty;
    let entry = (p.entry_px as u128 * p.qty as u128 + fill as u128 * qty as u128) / total as u128;
    Position { qty: total, entry_px: entry as u64, collateral: p.collateral + collateral, ..p.clone() }
}
/// A beneficiary may be named only by a whitelisted caller (the `guard` contract).
pub fn check_beneficiary(caller: &str, beneficiary: Option<&str>, whitelist: &[&str]) -> Result<(), VenueError> {
    match beneficiary {
        Some(_) if !whitelist.contains(&caller) => Err(VenueError::BeneficiaryNotAllowed),
        _ => Ok(()),
    }
}

pub const LIQ_BOUNTY_BP: u128 = 50;       // 0.5% of maintenance to whoever liquidates
pub const MAX_OI_X: u128 = 2;             // total open notional ≤ 200% of pool equity
pub const ORACLE_MAX_AGE_S: u64 = 60;
pub const ORACLE_MAX_CONF_BP: u128 = 50;  // 0.5%

/// Whole coins in a position.
fn coins(m: &Market, qty: u32) -> u64 {
    qty as u64 * m.contract_size / MICRO as u64
}

/// Close the whole position at `level` ∓ slippage: P&L, fee, bounty; what is left goes to the owner, a hole to the pool.
fn settle(m: &Market, p: &Position, level: u64, bounty: u128) -> Settlement {
    let exit_px = fill_px(m, level, p.side == Side::Short);
    let per_coin = exit_px as i128 - p.entry_px as i128;
    let pnl = match p.side { Side::Long => per_coin, Side::Short => -per_coin } * coins(m, p.qty) as i128;
    let fee = taker_fee(m, notional(m, p.qty, exit_px));
    let rest = p.collateral as i128 + pnl - fee as i128 - bounty as i128;
    Settlement { exit_px, pnl, fee, bounty, to_owner: rest.max(0) as u128, shortfall: (-rest).max(0) as u128 }
}

#[cfg(test)]
mod tests;
