//! Venue risk-core tests, written before the code (SPEC.md §venue). V_13 (events), V_14 (end to end) and the pool's
//! book-keeping run against the NEAR contract wrapper in `tests/` from Nov 1.
use super::*;

const D: u128 = MICRO; // one dollar in micro-USD

fn long2() -> Position {
    Position { owner: "alice".into(), beneficiary: None, market: "NEAR".into(), side: Side::Long, qty: 2,
               entry_px: 4_861_000, collateral: 2_041 * D, stop_px: None, target_px: None }
}

#[test]
fn v01_open_fill_fee_initial_margin() {
    let m = Market::near();
    assert_eq!(fill_px(&m, 4_859_500, true), 4_861_000);          // + max(tick, 1 bp) = 4.8605, rounded up to the 0.001 tick
    assert_eq!(fill_px(&m, 4_859_500, false), 4_858_000);         // sells: down a tick, floored to the tick
    assert_eq!(notional(&m, 2, 4_861_000), 4_861_000_000);        // 2 × 500 NEAR × 4.861 = $4,861
    assert_eq!(taker_fee(&m, 4_861_000_000), 2_916_600);          // 6 bp = $2.9166
    assert_eq!(check_initial(&m, 2, 4_859_500, 2_041 * D), Ok(()));   // 42% of $4,859.50 = $2,040.99
    assert_eq!(check_initial(&m, 2, 4_859_500, 2_000 * D), Err(VenueError::BelowInitialMargin));
    assert_eq!(check_initial(&m, 2, 4_859_500, 2_040_990_000), Ok(()));
    assert_eq!(check_initial(&m, 2, 4_859_500, 2_040_989_999), Err(VenueError::BelowInitialMargin));
}

#[test]
fn v02_maintenance_tiers() {
    let m = Market::near();
    assert_eq!(maint_rate_bp(&m, 1_000), 100);
    assert_eq!(maint_rate_bp(&m, 4_000), 100);
    assert_eq!(maint_rate_bp(&m, 4_001), 150);
    assert_eq!(maint_rate_bp(&m, 50_000), 200);
    assert_eq!(maint_rate_bp(&m, 9_000_000), 10_000);
}

#[test]
fn v03_mark_equity_maintenance() {
    let m = Market::near();
    let p = long2();
    assert_eq!(unrealized(&m, &p, 4_700_000), -161_000_000);       // (4.70 − 4.861) × 1,000 NEAR = −$161
    assert_eq!(equity(&m, &p, 4_700_000), 1_880_000_000);
    assert_eq!(maintenance(&m, &p, 4_700_000), 47_000_000);         // 1% of $4,700
    assert!(!liquidatable(&m, &p, 4_700_000));
    let mut s = p.clone(); s.side = Side::Short;
    assert_eq!(unrealized(&m, &s, 4_700_000), 161_000_000);
}

#[test]
fn v04_liquidation_and_shortfall() {
    let m = Market::near();
    let p = long2();
    assert!(!liquidatable(&m, &p, 2_849_000));                     // equity $29 ≥ maintenance $28.49
    assert_eq!(liquidate(&m, &p, 2_849_000), Err(VenueError::NotLiquidatable));
    assert!(liquidatable(&m, &p, 2_847_000));                      // equity $27 < $28.47
    let s = liquidate(&m, &p, 2_847_000).unwrap();
    assert_eq!(s.exit_px, 2_846_000);
    assert_eq!(s.pnl, -2_015_000_000);
    assert_eq!(s.fee, 1_707_600);
    assert_eq!(s.bounty, 142_350);                                 // 0.5% of maintenance
    assert_eq!(s.to_owner, 24_150_050);
    assert_eq!(s.shortfall, 0);
    // a gap straight through: equity below zero → the pool pays the shortfall, the owner gets nothing
    let g = liquidate(&m, &p, 2_500_000).unwrap();
    assert_eq!(g.exit_px, 2_499_000);
    assert_eq!(g.to_owner, 0);
    assert_eq!(g.shortfall, 321_000_000 + 1_499_400 + 125_000);
}

#[test]
fn v06_stop_and_target_on_price_only() {
    let m = Market::near();
    let mut p = long2(); p.stop_px = Some(4_500_000); p.target_px = Some(5_200_000);
    assert_eq!(trigger(&m, &p, 4_501_000), Err(VenueError::NotTriggered));
    assert_eq!(trigger(&m, &p, 5_199_000), Err(VenueError::NotTriggered));
    let (t, s) = trigger(&m, &p, 4_500_000).unwrap();
    assert_eq!(t, Trigger::Stop);
    assert_eq!(s.exit_px, 4_499_000);
    assert_eq!(s.pnl, -362_000_000);
    assert_eq!(s.fee, 2_699_400);
    assert_eq!(s.to_owner, 1_676_300_600);
    assert_eq!(s.bounty, 0);
    let (_, gap) = trigger(&m, &p, 4_400_000).unwrap();            // gapped through the stop: fills at the market, not the level
    assert_eq!(gap.exit_px, 4_399_000);
    let (t, s) = trigger(&m, &p, 5_200_000).unwrap();
    assert_eq!(t, Trigger::Target);
    assert_eq!(s.exit_px, 5_199_000);
    let mut sh = long2(); sh.side = Side::Short; sh.stop_px = Some(5_000_000);
    let (t, s) = trigger(&m, &sh, 5_000_000).unwrap();
    assert_eq!(t, Trigger::Stop);
    assert_eq!(s.exit_px, 5_001_000);
}

#[test]
fn v08_pool_capacity() {
    let pool = 10_000 * D;
    assert_eq!(check_capacity(15_000 * D, 4_861_000_000, pool), Ok(()));
    assert_eq!(check_capacity(15_000 * D, 5_100 * D, pool), Err(VenueError::PoolCapacity));
}

#[test]
fn v10_fees_and_builder() {
    let m = Market::near();
    assert_eq!(builder_fee(&m, 4_861_000_000, 10), Ok(4_861_000));
    assert_eq!(builder_fee(&m, 4_861_000_000, 0), Ok(0));
    assert_eq!(builder_fee(&m, 4_861_000_000, 11), Err(VenueError::BuilderFeeTooHigh));
}

#[test]
fn v11_oracle_guards() {
    let ok = Oracle { px: 4_000_000, conf: 20_000, ts: 940, replay: false };
    assert_eq!(check_oracle(&ok, 1_000, false), Ok(()));
    assert_eq!(check_oracle(&Oracle { ts: 939, ..ok }, 1_000, false), Err(VenueError::OracleStale));
    assert_eq!(check_oracle(&Oracle { conf: 20_001, ..ok }, 1_000, false), Err(VenueError::OracleWide));
    assert_eq!(check_oracle(&Oracle { replay: true, ..ok }, 1_000, false), Err(VenueError::ReplayNotAllowed));
    assert_eq!(check_oracle(&Oracle { replay: true, ..ok }, 1_000, true), Ok(()));
}

#[test]
fn v12_adding_averages_the_entry() {
    let p = add_to(&long2(), 2, 4_701_000, 2_000 * D);
    assert_eq!(p.qty, 4);
    assert_eq!(p.entry_px, 4_781_000);
    assert_eq!(p.collateral, 4_041 * D);
}

#[test]
fn v15_beneficiary_only_from_guard() {
    let wl = ["guard.tape.testnet"];
    assert_eq!(check_beneficiary("guard.tape.testnet", Some("alice.testnet"), &wl), Ok(()));
    assert_eq!(check_beneficiary("eve.testnet", Some("alice.testnet"), &wl), Err(VenueError::BeneficiaryNotAllowed));
    assert_eq!(check_beneficiary("eve.testnet", None, &wl), Ok(()));
}
