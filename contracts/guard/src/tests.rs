//! Guard policy-core tests, written before the code (SPEC.md §guard). G_11 (scope), G_12 (funds path), G_13 (events)
//! and G_14 (the demo beat, end to end) run against the NEAR contract wrapper in `tests/` from Nov 1.
use super::*;

const D: u128 = 1_000_000;

fn policy() -> Policy {
    Policy { markets: vec!["NEAR".into()], max_qty: 4, max_collateral: 3_000 * D, max_exposure: 12_000 * D,
             max_leverage_x10: 20, stop_required: true, max_stop_bp: None, drawdown_freeze_bp: 1_500,
             min_calls: 30, min_right_bp: 5_500, window_n: 30, require_call: true }
}
fn order() -> Order {
    Order { market: "NEAR".into(), side: Side::Long, qty: 2, collateral: 2_500 * D, notional: 4_861_000_000,
            fill_px: 4_861_000, stop_px: Some(4_500_000), call_id: Some(7) }
}
fn call() -> CallView {
    CallView { id: 7, agent: "machine".into(), market: "NEAR".into(), side: Side::Long, revealed: true, graded: false }
}
fn ctx(c: &CallView) -> Ctx<'_> {
    Ctx { agent: "machine", revoked: false, frozen: false, open_exposure: 0, record: Record { n: 30, right: 17 },
          call: Some(c), call_used_by_owner: false }
}

#[test]
fn g00_a_good_order_passes() {
    let c = call();
    assert_eq!(check(&policy(), &order(), &ctx(&c)), Ok(()));
}

#[test]
fn g01_owner_only_money() {
    let mut a = Account::new("alice");
    a.deposit(5_000 * D);
    assert_eq!((a.balance, a.peak, a.frozen), (5_000 * D, 5_000 * D, false));
    assert_eq!(a.withdraw("machine", "machine", 1 * D), Err(WalletError::NotOwner));
    assert_eq!(a.withdraw("alice", "bob", 1 * D), Err(WalletError::ToNotOwner));
    a.locked = 2_500 * D; a.balance = 2_500 * D;                   // collateral out on the venue is not withdrawable
    assert_eq!(a.withdraw("alice", "alice", 2_501 * D), Err(WalletError::Insufficient));
    assert_eq!(a.withdraw("alice", "alice", 2_500 * D), Ok(()));
    assert_eq!(a.balance, 0);
}

#[test]
fn g02_revoke_comes_first() {
    let c = call();
    let mut x = ctx(&c); x.revoked = true; x.frozen = true;
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Revoked));
}

#[test]
fn g03_markets() {
    let c = call();
    let mut o = order(); o.market = "ZEC".into();
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::Market));
}

#[test]
fn g04_size_and_collateral() {
    let c = call();
    let mut o = order(); o.qty = 5;
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::Size));
    let mut o = order(); o.collateral = 3_001 * D;
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::Collateral));
}

#[test]
fn g05_leverage() {
    let c = call();
    let mut o = order(); o.collateral = 2_041 * D;                 // $4,861 ÷ $2,041 = 2.38x > 2.0x
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::Leverage));
    let mut o = order(); o.collateral = 2_430_500_000;             // exactly 2.0x passes
    assert_eq!(check(&policy(), &o, &ctx(&c)), Ok(()));
}

#[test]
fn g06_exposure() {
    let c = call();
    let mut x = ctx(&c); x.open_exposure = 8_000 * D;              // $8,000 + $4,861 > $12,000
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Exposure));
    x.open_exposure = 7_139_000_000;                               // lands exactly on $12,000
    assert_eq!(check(&policy(), &order(), &x), Ok(()));
}

#[test]
fn g07_stops() {
    let c = call();
    let mut o = order(); o.stop_px = None;
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::NoStop));
    let mut o = order(); o.stop_px = Some(4_900_000);              // a long's stop above the fill
    assert_eq!(check(&policy(), &o, &ctx(&c)), Err(Refusal::StopSide));
    let mut p = policy(); p.max_stop_bp = Some(500);
    let mut o = order(); o.stop_px = Some(4_472_120);              // 8% away
    assert_eq!(check(&p, &o, &ctx(&c)), Err(Refusal::StopFar));
    o.stop_px = Some(4_620_000);                                   // 4.96% away
    assert_eq!(check(&p, &o, &ctx(&c)), Ok(()));
    let mut o = order(); o.stop_px = Some(1_000_000);              // no max set: a far stop is the owner's choice
    assert_eq!(check(&policy(), &o, &ctx(&c)), Ok(()));
    let mut p = policy(); p.stop_required = false;
    let mut o = order(); o.stop_px = None;
    assert_eq!(check(&p, &o, &ctx(&c)), Ok(()));
    let mut sc = call(); sc.side = Side::Short;
    let mut o = order(); o.side = Side::Short; o.stop_px = Some(4_700_000);   // a short's stop below the fill
    assert_eq!(check(&policy(), &o, &ctx(&sc)), Err(Refusal::StopSide));
}

#[test]
fn g08_drawdown_freeze_on_equity_not_time() {
    let mut a = Account::new("alice");
    a.deposit(5_000 * D);
    a.mark(4_251 * D, 1_500);                                      // threshold: $5,000 × 85% = $4,250
    assert!(!a.frozen);
    a.mark(4_249 * D, 1_500);
    assert!(a.frozen);
    let c = call();
    let mut x = ctx(&c); x.frozen = a.frozen;
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Frozen));
    assert_eq!(a.unfreeze("machine", 4_249 * D), Err(WalletError::NotOwner));
    assert_eq!(a.unfreeze("alice", 4_249 * D), Ok(()));
    assert_eq!((a.frozen, a.peak), (false, 4_249 * D));
    a.mark(4_400 * D, 1_500);
    assert_eq!(a.peak, 4_400 * D);                                 // the peak follows equity up
}

#[test]
fn g09_reputation_gate_moves_with_the_record() {
    let c = call();
    let mut x = ctx(&c);
    x.record = Record { n: 30, right: 16 };                        // 53.3% < 55%
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Record));
    x.record = Record { n: 30, right: 17 };                        // 56.7%: works again, no timer
    assert_eq!(check(&policy(), &order(), &x), Ok(()));
    x.record = Record { n: 29, right: 29 };                        // too few graded calls
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Record));
}

#[test]
fn g10_every_trade_cites_a_published_call() {
    let ok = call();
    let mut o = order(); o.call_id = None;
    let mut x = ctx(&ok); x.call = None;
    assert_eq!(check(&policy(), &o, &x), Err(Refusal::Call));
    for bad in [CallView { agent: "other".into(), ..call() }, CallView { revealed: false, ..call() },
                CallView { graded: true, ..call() }, CallView { market: "ZEC".into(), ..call() },
                CallView { side: Side::Short, ..call() }] {
        assert_eq!(check(&policy(), &order(), &ctx(&bad)), Err(Refusal::Call));
    }
    let mut x = ctx(&ok); x.call_used_by_owner = true;             // one call funds one open per owner
    assert_eq!(check(&policy(), &order(), &x), Err(Refusal::Call));
    let mut p = policy(); p.require_call = false;
    let mut x = ctx(&ok); x.call = None;
    assert_eq!(check(&p, &o, &x), Ok(()));
}
