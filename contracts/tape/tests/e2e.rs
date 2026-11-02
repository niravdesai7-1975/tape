//! End to end on a local NEAR sandbox, written before the contract. Build first: `make wasm` (cargo near build for
//! `tape` and `usdc-mock`). Money in micro-USD (USDC has 6 decimals); prices in micro-USD per NEAR.
//!
//!   G_14 the demo beat — a member funds, buys a call, gives The Machine a policy (not her keys); it trades inside it; two
//!        stops; the third order is refused on chain, FROZEN; she revokes and withdraws to herself
//!   G_11 scope — the agent cannot touch the owner's own positions or anyone else's
//!   G_12 / G_01 money — the agent can withdraw only what it earned; the owner's money goes only to the owner
//!   G_13 refusals are events with a code, and leave no position
//!   C_10 calls end to end — commit, reveal, buy, grade publishing the levels that hash to the commit
//!   V_14 venue end to end — a direct position liquidated by a keeper; every dollar reconciles
use near_workspaces::network::Sandbox;
use near_workspaces::types::NearToken;
use near_workspaces::{Account, Contract, Worker};
use serde_json::{json, Value};

const D: u128 = 1_000_000; // $1 in micro-USD
const T0: u64 = 1_793_000_000; // the replay day's clock starts here (seconds)
use std::sync::atomic::Ordering::SeqCst;

fn wasm(name: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/near");
    let n = name.replace('-', "_");
    let p = root.join(&n).join(format!("{n}.wasm"));
    std::fs::read(&p).unwrap_or_else(|_| panic!("build first: make wasm (missing {})", p.display()))
}

struct Env { w: Worker<Sandbox>, clock: std::sync::atomic::AtomicU64, last_px: std::sync::atomic::AtomicU64, usdc: Contract, tape: Contract, keeper: Account, grader: Account, machine: Account,
             alice: Account, bob: Account, ops: Account }

async fn setup() -> anyhow::Result<Env> {
    let w = near_workspaces::sandbox().await?;
    let usdc = w.dev_deploy(&wasm("usdc-mock")).await?;
    usdc.call("new").transact().await?.into_result()?;
    let tape = w.dev_deploy(&wasm("tape")).await?;
    let mut acc = vec![];
    for _ in 0..6 { acc.push(w.dev_create_account().await?); }
    let (keeper, grader, machine, alice, bob, ops) = (acc[0].clone(), acc[1].clone(), acc[2].clone(), acc[3].clone(), acc[4].clone(), acc[5].clone());
    tape.call("new").args_json(json!({"usdc": usdc.id(), "keeper": keeper.id(), "grader": grader.id(), "demo": true}))
        .transact().await?.into_result()?;
    usdc.call("register").args_json(json!({"account_id": tape.id()})).transact().await?.into_result()?;
    for (a, amt) in [(&alice, 5_100 * D), (&bob, 3_000 * D), (&ops, 20_000 * D)] {
        usdc.call("mint").args_json(json!({"account_id": a.id(), "amount": amt.to_string()})).transact().await?.into_result()?;
    }
    for a in [&machine, &keeper] {
        usdc.call("register").args_json(json!({"account_id": a.id()})).transact().await?.into_result()?;
    }
    let e = Env { w, clock: T0.into(), last_px: 4_859_500u64.into(), usdc, tape, keeper, grader, machine, alice, bob, ops };
    fund(&e, &e.ops, 20_000 * D, "pool").await?;
    price(&e, 4_859_500).await?;                                  // starts the replay clock
    Ok(e)
}

async fn fund(e: &Env, from: &Account, amount: u128, msg: &str) -> anyhow::Result<()> {
    from.call(e.usdc.id(), "ft_transfer_call")
        .args_json(json!({"receiver_id": e.tape.id(), "amount": amount.to_string(), "msg": msg}))
        .deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.into_result()?;
    Ok(())
}

/// The keeper pushes the next recorded price; in demo mode its timestamp is the contract's clock (+60 s per tick).
async fn price(e: &Env, px: u64) -> anyhow::Result<()> {
    let ts = e.clock.fetch_add(60, SeqCst) + 60;
    e.last_px.store(px, SeqCst);
    e.keeper.call(e.tape.id(), "push_price").args_json(json!({"px": px, "conf": 1_000, "ts": ts})).transact().await?.into_result()?;
    Ok(())
}

/// Move the replay clock forward: the keeper pushes the same price `secs` later.
async fn skip(e: &Env, secs: u64) -> anyhow::Result<()> {
    let ts = e.clock.fetch_add(secs, SeqCst) + secs;
    let px = e.last_px.load(SeqCst);
    e.keeper.call(e.tape.id(), "push_price").args_json(json!({"px": px, "conf": 1_000, "ts": ts})).transact().await?.into_result()?;
    Ok(())
}

async fn view(e: &Env, m: &str, args: Value) -> anyhow::Result<Value> {
    Ok(e.tape.view(m).args_json(args).await?.json()?)
}

fn u(v: &Value) -> u128 { v.as_str().map(|s| s.parse().unwrap()).unwrap_or_else(|| v.as_u64().unwrap() as u128) }

async fn usdc_of(e: &Env, a: &near_workspaces::AccountId) -> anyhow::Result<u128> {
    let v: Value = e.usdc.view("ft_balance_of").args_json(json!({"account_id": a})).await?.json()?;
    Ok(u(&v))
}

/// The agent commits a call with these levels and reveals it (sends it out). Returns (id, payload).
async fn call_out(e: &Env, side: &str, levels: &str) -> anyhow::Result<(u64, String)> {
    let payload = format!(r#"{{"market":"NEAR","side":"{side}",{levels}}}"#);
    let hash = tape_calls::hash_hex(&payload);
    let id: u64 = e.machine.call(e.tape.id(), "commit")
        .args_json(json!({"hash": hash, "market": "NEAR", "side": side, "price": (5 * D).to_string(), "note": null}))
        .transact().await?.json()?;
    e.machine.call(e.tape.id(), "reveal").args_json(json!({"id": id})).transact().await?.into_result()?;
    Ok((id, payload))
}

/// Three graded calls for The Machine — two right, one wrong — so its record clears a 55% bar over 3.
async fn seed_record(e: &Env) -> anyhow::Result<()> {
    let mut out = vec![];
    for i in 0..3 { out.push(call_out(e, "Long", &format!(r#""seed":{i}"#)).await?); }
    skip(&e, 4 * 3600).await?;
    for (i, (id, p)) in out.iter().enumerate() {
        let px_4h = if i == 1 { 3_900_000 } else { 4_100_000 };
        e.grader.call(e.tape.id(), "grade").args_json(json!({"id": id, "payload": p, "px_entry": 4_000_000u64, "px_4h": px_4h}))
            .transact().await?.into_result()?;
    }
    Ok(())
}

fn policy() -> Value {
    json!({"markets": ["NEAR"], "max_qty": 4, "max_collateral": 3_000 * D, "max_exposure": 12_000 * D, "max_leverage_x10": 20,
           "stop_required": true, "max_stop_bp": null, "drawdown_freeze_bp": 1_500,
           "min_calls": 3, "min_right_bp": 5_500, "window_n": 3, "require_call": true})
}

async fn agent_open(e: &Env, id: u64, collateral: u128, stop: u64, target: Option<u64>) -> anyhow::Result<Value> {
    Ok(e.machine.call(e.tape.id(), "agent_open")
        .args_json(json!({"owner": e.alice.id(), "market": "NEAR", "side": "Long", "qty": 2, "collateral": collateral.to_string(),
                          "stop_px": stop, "target_px": target, "call_id": id}))
        .max_gas().transact().await?.json()?)
}

#[tokio::test]
async fn t00_the_replay_clock_only_moves_forward_and_only_by_the_keeper() -> anyhow::Result<()> {
    let e = setup().await?;
    let back = e.keeper.call(e.tape.id(), "push_price").args_json(json!({"px": 4_000_000u64, "conf": 1_000, "ts": T0})).transact().await?;
    assert!(back.is_failure(), "the replay clock went backwards");
    let other = e.alice.call(e.tape.id(), "push_price").args_json(json!({"px": 4_000_000u64, "conf": 1_000, "ts": T0 + 999})).transact().await?;
    assert!(other.is_failure(), "someone other than the keeper set the price");
    Ok(())
}

#[tokio::test]
async fn g14_the_demo_beat() -> anyhow::Result<()> {
    let e = setup().await?;
    seed_record(&e).await?;
    assert_eq!(view(&e, "record", json!({"agent": e.machine.id(), "window_n": 3})).await?, json!([3, 2]));

    fund(&e, &e.alice, 5_000 * D, "deposit").await?;
    let (a, _) = call_out(&e, "Long", r#""entry":4.861,"stop":4.50,"target":5.20"#).await?;
    fund(&e, &e.alice, 5 * D, &format!("buy:{a}")).await?;                       // $5 for the call → The Machine
    assert_eq!(view(&e, "has_bought", json!({"member": e.alice.id(), "id": a})).await?, json!(true));
    e.alice.call(e.tape.id(), "grant").args_json(json!({"agent": e.machine.id(), "policy": policy()})).transact().await?.into_result()?;

    // call A: opens inside the policy; the stop is hit on a gap
    price(&e, 4_859_500).await?;
    let r = agent_open(&e, a, 2_500 * D, 4_500_000, Some(5_200_000)).await?;
    let pa = r["position"].as_u64().expect("A opens");
    assert!(r["refused"].is_null());
    price(&e, 4_450_000).await?;
    let t: Value = e.keeper.call(e.tape.id(), "trigger").args_json(json!({"position_id": pa})).max_gas().transact().await?.json()?;
    assert_eq!(t, json!("Stop"));
    let acct = view(&e, "account", json!({"owner": e.alice.id()})).await?;
    assert_eq!(u(&acct["balance"]), 4_582_414_000);               // 5,000 − 2,500 − 2.9166 + 2,085.3306
    assert_eq!(acct["frozen"], json!(false));

    // call B: a second loss
    let (b, _) = call_out(&e, "Long", r#""entry":4.401,"stop":3.95"#).await?;
    price(&e, 4_400_000).await?;
    let pb = agent_open(&e, b, 2_500 * D, 3_950_000, None).await?["position"].as_u64().expect("B opens");
    price(&e, 3_900_000).await?;
    e.keeper.call(e.tape.id(), "trigger").args_json(json!({"position_id": pb})).max_gas().transact().await?.into_result()?;
    let acct = view(&e, "account", json!({"owner": e.alice.id()})).await?;
    assert_eq!(u(&acct["balance"]), 4_075_434_000);               // below $4,250 = 85% of the $5,000 peak

    // call C: refused on chain, FROZEN — nothing opens, nothing moves
    let (c, _) = call_out(&e, "Long", r#""entry":3.901,"stop":3.70"#).await?;
    let r = agent_open(&e, c, 2_000 * D, 3_700_000, None).await?;
    assert_eq!(r["refused"], json!("FROZEN"));
    assert!(r["position"].is_null());
    // the app reads the story straight from the chain: the newest events, newest first
    let recent = view(&e, "recent", json!({"n": 3})).await?;
    assert_eq!(recent[0]["event"], json!("refused"));
    assert_eq!(recent[0]["data"]["code"], json!("FROZEN"));
    assert!(recent.as_array().unwrap().iter().any(|v| v["event"] == json!("frozen")));
    assert_eq!(u(&view(&e, "account", json!({"owner": e.alice.id()})).await?["balance"]), 4_075_434_000);

    // she revokes and takes her money home; The Machine keeps only the $5 she paid it
    e.alice.call(e.tape.id(), "revoke").args_json(json!({"agent": e.machine.id()})).transact().await?.into_result()?;
    let before = usdc_of(&e, e.alice.id()).await?;
    e.alice.call(e.tape.id(), "withdraw").args_json(json!({"amount": "4075434000"})).deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.into_result()?;
    assert_eq!(usdc_of(&e, e.alice.id()).await? - before, 4_075_434_000);
    e.machine.call(e.tape.id(), "withdraw").args_json(json!({"amount": (5 * D).to_string()})).deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.into_result()?;
    assert_eq!(usdc_of(&e, e.machine.id()).await?, 5 * D);
    let r = agent_open(&e, c, 2_000 * D, 3_700_000, None).await?;
    assert_eq!(r["refused"], json!("REVOKED"));
    Ok(())
}

/// The agent asks the chain before an order goes to her Coinbase account, reporting her venue equity.
async fn ask(e: &Env, id: u64, qty: u32, collateral: u128, notional: u128, stop: Option<u64>, equity: u128) -> anyhow::Result<Value> {
    Ok(e.machine.call(e.tape.id(), "ask")
        .args_json(json!({"owner": e.alice.id(), "market": "NEAR", "side": "Long", "qty": qty, "collateral": collateral.to_string(),
                          "notional": notional.to_string(), "stop_px": stop, "call_id": id, "equity": equity.to_string()}))
        .max_gas().transact().await?.json()?)
}

/// The receipt of a Coinbase fill, posted against the ticket the ask returned.
async fn fill(e: &Env, by: &Account, ticket: u64, order_ref: &str, kind: &str, px: u64, pnl: Option<i128>, equity: u128) -> anyhow::Result<near_workspaces::result::ExecutionFinalResult> {
    Ok(by.call(e.tape.id(), "fill")
        .args_json(json!({"owner": e.alice.id(), "ticket": ticket, "venue": "coinbase", "order_ref": order_ref, "kind": kind, "px": px,
                          "pnl": pnl.map(|p| p.to_string()), "equity": equity.to_string()}))
        .max_gas().transact().await?)
}

#[tokio::test]
async fn g15_her_money_stays_on_coinbase_the_chain_holds_the_rules_the_asks_and_the_receipts() -> anyhow::Result<()> {
    let e = setup().await?;
    seed_record(&e).await?;
    // a $20 fee float is all of hers that comes to NEAR; she pays $5 for the call from it
    fund(&e, &e.alice, 20 * D, "deposit").await?;
    let (a, _) = call_out(&e, "Long", r#""entry":3.99,"stop":3.80,"target":4.30"#).await?;
    e.alice.call(e.tape.id(), "buy").args_json(json!({"id": a})).transact().await?.into_result()?;
    assert_eq!(view(&e, "has_bought", json!({"member": e.alice.id(), "id": a})).await?, json!(true));
    assert_eq!(u(&view(&e, "account", json!({"owner": e.alice.id()})).await?["balance"]), 15 * D);
    assert_eq!(u(&view(&e, "account", json!({"owner": e.machine.id()})).await?["balance"]), 5 * D);
    assert!(e.bob.call(e.tape.id(), "buy").args_json(json!({"id": a})).transact().await?.is_failure(), "bought with no float");
    e.alice.call(e.tape.id(), "grant").args_json(json!({"agent": e.machine.id(), "policy": policy()})).transact().await?.into_result()?;

    // the ask: 2 contracts (1,000 NEAR) at 3.99 = $3,990 on $2,000 collateral (2.0x), stop 3.80, her Coinbase equity $5,000
    let r = ask(&e, a, 2, 2_000 * D, 3_990 * D, Some(3_800_000), 5_000 * D).await?;
    let t = r["ticket"].as_u64().expect("allowed");
    assert!(r["refused"].is_null());
    // only the agent that asked may post the receipt, and the same order ref only once
    assert!(fill(&e, &e.bob, t, "o-1", "Open", 3_990_000, None, 5_000 * D).await?.is_failure());
    fill(&e, &e.machine, t, "o-1", "Open", 3_990_000, None, 5_000 * D).await?.into_result()?;
    assert!(fill(&e, &e.machine, t, "o-1", "Open", 3_990_000, None, 5_000 * D).await?.is_failure(), "a receipt posted twice");
    // the target hits on Coinbase: the close receipt carries the P&L and her equity after it
    fill(&e, &e.machine, t, "o-2", "Close", 4_300_000, Some(454_540_000), 5_454_540_000).await?.into_result()?;
    let recent = view(&e, "recent", json!({"n": 4})).await?;
    assert_eq!(recent[0]["event"], json!("filled"));
    assert_eq!(recent[0]["data"]["kind"], json!("Close"));
    assert_eq!(recent[0]["data"]["pnl"], json!("454540000"));
    assert_eq!(recent[2]["event"], json!("asked"));
    assert_eq!(view(&e, "call", json!({"id": a})).await?["fills"], json!(["coinbase:o-1"]));
    // a second ask on the same call is refused: once per owner
    assert_eq!(ask(&e, a, 2, 2_000 * D, 3_990 * D, Some(3_800_000), 5_454_540_000).await?["refused"], json!("CALL"));
    // she revokes; the next ask is REVOKED; the agent withdraws only what she paid it
    e.alice.call(e.tape.id(), "revoke").args_json(json!({"agent": e.machine.id()})).transact().await?.into_result()?;
    let (b, _) = call_out(&e, "Long", r#""entry":4.29,"stop":4.10,"target":4.50"#).await?;
    assert_eq!(ask(&e, b, 2, 2_000 * D, 4_290 * D, Some(4_100_000), 5_454_540_000).await?["refused"], json!("REVOKED"));
    e.machine.call(e.tape.id(), "withdraw").args_json(json!({"amount": (5 * D).to_string()})).deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.into_result()?;
    assert_eq!(usdc_of(&e, e.machine.id()).await?, 5 * D);
    assert!(e.machine.call(e.tape.id(), "withdraw").args_json(json!({"amount": "1"})).deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.is_failure());
    Ok(())
}

#[tokio::test]
async fn g16_the_night_two_losing_closes_then_the_third_ask_is_refused_frozen() -> anyhow::Result<()> {
    let e = setup().await?;
    seed_record(&e).await?;
    e.alice.call(e.tape.id(), "grant").args_json(json!({"agent": e.machine.id(), "policy": policy()})).transact().await?.into_result()?;
    let mut eq = 5_000 * D;
    for (i, (entry, stop, loss)) in [(4_861_000u64, 4_500_000u64, 417_586_000u128), (4_401_000, 3_950_000, 506_980_000)].iter().enumerate() {
        let (c, _) = call_out(&e, "Long", &format!(r#""entry":{entry},"stop":{stop}"#)).await?;
        let t = ask(&e, c, 2, 2_500 * D, *entry as u128 * 1_000, Some(*stop), eq).await?["ticket"].as_u64().expect("allowed");
        fill(&e, &e.machine, t, &format!("o-{i}a"), "Open", *entry, None, eq).await?.into_result()?;
        eq -= loss;
        fill(&e, &e.machine, t, &format!("o-{i}b"), "Close", *stop, Some(-(*loss as i128)), eq).await?.into_result()?;
    }
    assert_eq!(eq, 4_075_434_000);                                                // under 85% of her $5,000 peak
    let (c, _) = call_out(&e, "Long", r#""entry":3.901,"stop":3.70"#).await?;
    let r = ask(&e, c, 2, 2_000 * D, 3_901 * D, Some(3_700_000), eq).await?;
    assert_eq!(r["refused"], json!("FROZEN"));
    assert!(r["ticket"].is_null());
    let recent = view(&e, "recent", json!({"n": 5})).await?;                   // refused · revealed · committed · filled · frozen
    assert_eq!(recent[0]["event"], json!("refused"));
    assert_eq!(recent[0]["data"]["code"], json!("FROZEN"));
    assert_eq!(recent[4]["event"], json!("frozen"));
    assert_eq!(view(&e, "account", json!({"owner": e.alice.id()})).await?["frozen"], json!(true));
    // only she unfreezes, and the peak resets to where her money is
    let _ = e.machine.call(e.tape.id(), "unfreeze_at").args_json(json!({"equity": eq.to_string()})).transact().await?;
    assert_eq!(view(&e, "account", json!({"owner": e.alice.id()})).await?["frozen"], json!(true));
    e.alice.call(e.tape.id(), "unfreeze_at").args_json(json!({"equity": eq.to_string()})).transact().await?.into_result()?;
    assert!(ask(&e, c, 2, 2_000 * D, 3_901 * D, Some(3_700_000), eq).await?["ticket"].is_u64());
    Ok(())
}

#[tokio::test]
async fn g11_g12_g13_scope_money_and_refusal_events() -> anyhow::Result<()> {
    let e = setup().await?;
    seed_record(&e).await?;
    fund(&e, &e.alice, 5_000 * D, "deposit").await?;
    fund(&e, &e.bob, 3_000 * D, "deposit").await?;
    price(&e, 4_859_500).await?;
    // her own direct position and Bob's are out of the agent's reach
    let own: u64 = e.alice.call(e.tape.id(), "open").args_json(json!({"market": "NEAR", "side": "Long", "qty": 1, "collateral": (1_100 * D).to_string(),
        "stop_px": null, "target_px": null})).max_gas().transact().await?.json()?;
    let bobs: u64 = e.bob.call(e.tape.id(), "open").args_json(json!({"market": "NEAR", "side": "Long", "qty": 1, "collateral": (1_100 * D).to_string(),
        "stop_px": null, "target_px": null})).max_gas().transact().await?.json()?;
    e.alice.call(e.tape.id(), "grant").args_json(json!({"agent": e.machine.id(), "policy": policy()})).transact().await?.into_result()?;
    for (owner, id) in [(e.alice.id(), own), (e.bob.id(), bobs)] {
        let r = e.machine.call(e.tape.id(), "agent_close").args_json(json!({"owner": owner, "position_id": id})).max_gas().transact().await?;
        assert!(r.is_failure(), "the agent closed a position it did not open");
    }
    // the agent cannot withdraw the owner's money
    let r = e.machine.call(e.tape.id(), "withdraw").args_json(json!({"amount": (1 * D).to_string()})).deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?;
    assert!(r.is_failure(), "the agent withdrew money it never had");
    // a refusal is an event with its code, and leaves no position
    let (cid, _) = call_out(&e, "Long", r#""entry":4.861,"stop":4.90"#).await?;
    let out = e.machine.call(e.tape.id(), "agent_open")
        .args_json(json!({"owner": e.alice.id(), "market": "NEAR", "side": "Long", "qty": 2, "collateral": (2_500 * D).to_string(),
                          "stop_px": 4_900_000u64, "target_px": null, "call_id": cid})).max_gas().transact().await?;
    assert!(out.logs().iter().any(|l| l.starts_with("EVENT_JSON:") && l.contains("\"refused\"") && l.contains("STOP_SIDE")), "{:?}", out.logs());
    assert_eq!(out.json::<Value>()?["refused"], json!("STOP_SIDE"));
    let ps = view(&e, "positions_of", json!({"owner": e.alice.id()})).await?;
    assert_eq!(ps.as_array().unwrap().len(), 1);                  // only her own
    Ok(())
}

#[tokio::test]
async fn c10_calls_end_to_end() -> anyhow::Result<()> {
    let e = setup().await?;
    fund(&e, &e.alice, 100 * D, "deposit").await?;
    let (id, payload) = call_out(&e, "Short", r#""entry":4.20,"stop":4.40"#).await?;
    assert_eq!(view(&e, "published", json!({"id": id})).await?, Value::Null);   // nothing readable before the grade
    fund(&e, &e.alice, 5 * D, &format!("buy:{id}")).await?;
    assert_eq!(u(&view(&e, "account", json!({"owner": e.machine.id()})).await?["balance"]), 5 * D);
    let early = e.grader.call(e.tape.id(), "grade").args_json(json!({"id": id, "payload": payload, "px_entry": 4_200_000u64, "px_4h": 4_000_000u64})).transact().await?;
    assert!(early.is_failure(), "graded before 4 hours");
    skip(&e, 4 * 3600).await?;
    let forged = e.grader.call(e.tape.id(), "grade").args_json(json!({"id": id, "payload": "{\"side\":\"Long\"}", "px_entry": 4_200_000u64, "px_4h": 4_000_000u64})).transact().await?;
    assert!(forged.is_failure(), "graded with levels that were not committed");
    let o: Value = e.grader.call(e.tape.id(), "grade").args_json(json!({"id": id, "payload": payload, "px_entry": 4_200_000u64, "px_4h": 4_000_000u64})).transact().await?.json()?;
    assert_eq!(o, json!("Right"));
    assert_eq!(view(&e, "published", json!({"id": id})).await?, json!(payload));
    assert_eq!(view(&e, "record", json!({"agent": e.machine.id(), "window_n": 30})).await?, json!([1, 1]));
    Ok(())
}

#[tokio::test]
async fn v14_liquidation_and_every_dollar_reconciles() -> anyhow::Result<()> {
    let e = setup().await?;
    fund(&e, &e.bob, 3_000 * D, "deposit").await?;
    price(&e, 4_859_500).await?;
    let id: u64 = e.bob.call(e.tape.id(), "open").args_json(json!({"market": "NEAR", "side": "Long", "qty": 2, "collateral": (2_041 * D).to_string(),
        "stop_px": null, "target_px": null})).max_gas().transact().await?.json()?;
    price(&e, 2_849_000).await?;
    let early = e.keeper.call(e.tape.id(), "liquidate").args_json(json!({"position_id": id})).max_gas().transact().await?;
    assert!(early.is_failure(), "liquidated above maintenance");
    price(&e, 2_847_000).await?;
    e.keeper.call(e.tape.id(), "liquidate").args_json(json!({"position_id": id})).max_gas().transact().await?.into_result()?;
    let bob = view(&e, "account", json!({"owner": e.bob.id()})).await?;
    assert_eq!(u(&bob["balance"]), 3_000 * D - 2_041 * D - 2_916_600 + 24_150_050);
    assert_eq!(u(&view(&e, "account", json!({"owner": e.keeper.id()})).await?["balance"]), 142_350);   // the bounty
    // every dollar the contract holds is someone's balance, someone's collateral, or the pool
    let mut held = u(&view(&e, "pool", json!({})).await?);
    for a in [&e.alice, &e.bob, &e.machine, &e.keeper, &e.ops] {
        let v = view(&e, "account", json!({"owner": a.id()})).await?;
        held += u(&v["balance"]) + u(&v["locked"]);
    }
    assert_eq!(held, usdc_of(&e, e.tape.id()).await?);
    Ok(())
}
