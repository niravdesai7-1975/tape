//! `make demo` — plays one recorded NEAR day against the Tape contract and narrates it, step by step.
//!
//!   cargo run -p tape-demo                 a fresh local NEAR sandbox (needs `make wasm` first)
//!   cargo run -p tape-demo -- testnet      the live testnet deployment (app.tape-nvibe.testnet), keys from
//!                                          ~/.near-credentials/testnet (or TAPE_KEYS); prints explorer links
//!
//! The prices are real (NEAR, Oct 29–30 2026, 2-minute marks). The calls are scripted demo calls, not The Machine's live
//! output — the demo shows the protocol: a record nobody can edit, and an agent that trades a member's money only inside
//! rules the chain enforces. The keeper replays the day: each price carries its recorded time, which is the contract's clock.
use anyhow::{anyhow, Context, Result};
use near_workspaces::network::{Sandbox, Testnet};
use near_workspaces::types::{NearToken, SecretKey};
use near_workspaces::{Account, AccountId, Worker};
use serde_json::{json, Value};
use std::str::FromStr;

const D: f64 = 1_000_000.0;

struct Cast {
    tape: AccountId,
    usdc: AccountId,
    keeper: Account,
    grader: Account,
    machine: Account,
    alice: Account,
    ops: Account,
    funder: Account, // anyone who can mint test USDC (the mock lets anyone)
    explorer: Option<&'static str>,
    shift: u64,      // added to every recorded time so the replay clock only moves forward on a reused contract
    last_ts: u64,
}

fn money(v: &Value) -> f64 {
    v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64()).unwrap_or(0.0) / D
}

fn hhmm(ts: u64) -> String {
    let s = ts % 86_400;
    let day = if ts < 1_793_318_400 { "Oct 29" } else { "Oct 30" }; // 2026-10-30T00:00Z
    format!("{day} {:02}:{:02}", s / 3600, (s % 3600) / 60)
}

impl Cast {
    fn link(&self, r: &near_workspaces::result::ExecutionFinalResult) -> String {
        let h = r.outcome().transaction_hash.to_string();
        match self.explorer { Some(base) => format!("{base}{h}"), None => format!("tx {}…", &h[..8]) }
    }

    async fn call(&self, who: &Account, to: &AccountId, m: &str, args: Value, yocto: u128) -> Result<(Value, String)> {
        let mut wait = 2;
        loop {
            if self.explorer.is_some() { tokio::time::sleep(std::time::Duration::from_millis(400)).await; } // public RPC: be polite
            let r = who.call(to, m).args_json(args.clone()).deposit(NearToken::from_yoctonear(yocto))
                .gas(near_workspaces::types::Gas::from_tgas(60)).transact().await;
            match r {
                Err(e) if transient(&e) && wait <= 64 => {
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await; wait *= 2; continue;
                }
                Err(e) => return Err(anyhow!("{m}: {e}")),
                Ok(r) => {
                    let link = self.link(&r);
                    let r = r.into_result().map_err(|e| anyhow!("{m} failed: {e:?}"))?;
                    return Ok((r.json::<Value>().unwrap_or(Value::Null), link));
                }
            }
        }
    }

    async fn view(&self, m: &str, args: Value) -> Result<Value> {
        let mut wait = 2;
        loop {
            match self.alice.view(&self.tape, m).args_json(args.clone()).await {
                Err(e) if transient(&e) && wait <= 64 => {
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await; wait *= 2;
                }
                Err(e) => return Err(anyhow!("{m}: {e}")),
                Ok(v) => return Ok(v.json()?),
            }
        }
    }

    /// The keeper pushes the recorded price at its (shifted) recorded time.
    async fn price(&mut self, ts: u64, px: u64) -> Result<()> {
        let t = ts + self.shift;
        if t <= self.last_ts { return Ok(()); }
        self.call(&self.keeper, &self.tape.clone(), "push_price", json!({"px": px, "conf": 1_000, "ts": t}), 0).await?;
        self.last_ts = t;
        Ok(())
    }

    async fn deposit(&self, from: &Account, amount: u64, msg: &str) -> Result<String> {
        let (_, l) = self.call(from, &self.usdc, "ft_transfer_call",
            json!({"receiver_id": self.tape, "amount": amount.to_string(), "msg": msg}), 1).await?;
        Ok(l)
    }
}

/// The public RPC refused or rate-limited the request before anything was sent — safe to ask again.
fn transient(e: &near_workspaces::error::Error) -> bool {
    let mut txt = format!("{e} {e:?}");
    let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e);
    while let Some(s) = src { txt.push_str(&s.to_string()); src = s.source(); }
    txt.contains("rate limit") || txt.contains("query request") || txt.contains("TooManyRequests") || txt.contains("429")
}

fn px_at(prices: &[(u64, u64)], ts: u64) -> u64 {
    prices.iter().rev().find(|(t, _)| *t <= ts + 59).map(|(_, p)| *p).unwrap_or(prices[0].1)
}

fn payload(side: &str, entry: u64, stop: u64, target: u64, tag: &str) -> String {
    format!(r#"{{"market":"NEAR","side":"{side}","entry":{:.4},"stop":{:.4},"target":{:.4},"id":"{tag}"}}"#,
            entry as f64 / D, stop as f64 / D, target as f64 / D)
}

#[tokio::main]
async fn main() -> Result<()> {
    let testnet = std::env::args().any(|a| a == "testnet");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let day: Value = serde_json::from_str(&std::fs::read_to_string(root.join("day.json"))?)?;
    let prices: Vec<(u64, u64)> = day["prices"].as_array().unwrap().iter().map(|p| (p["ts"].as_u64().unwrap(), p["px"].as_u64().unwrap())).collect();
    let run_tag = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());

    println!("\n  TAPE — give an agent your money, not your keys");
    println!("  {}", day["title"].as_str().unwrap());
    println!("  {}\n", day["note"].as_str().unwrap());

    let mut c = if testnet { testnet_cast().await? } else { sandbox_cast(&root).await? };
    if testnet {
        // a reused contract: start the replay clock after whatever it last saw
        let o = c.view("oracle", json!({})).await?;
        let last = o["ts"].as_u64().unwrap_or(0);
        // whole days, so the replayed clock still reads 17:01, 18:47 … on the page
        c.shift = if last >= prices[0].0 { ((last - prices[0].0) / 86_400 + 1) * 86_400 } else { 0 };
        // an interrupted earlier run may have left a position, Alice's money or a grant behind: hand it all back first
        let left_open = c.view("positions_of", json!({"owner": c.alice.id()})).await?;
        if left_open.as_array().map(|v| !v.is_empty()).unwrap_or(false) {
            c.price(prices[0].0, prices[0].1).await?;
            for p in left_open.as_array().unwrap() {
                c.call(&c.alice, &c.tape.clone(), "close", json!({"position_id": p["id"]}), 0).await?;
            }
        }
        let a = c.view("account", json!({"owner": c.alice.id()})).await?;
        let _ = c.call(&c.alice, &c.tape.clone(), "revoke", json!({"agent": c.machine.id()}), 0).await;
        let left = (money(&a["balance"]) * D).round() as u64;
        if left > 0 { c.call(&c.alice, &c.tape.clone(), "withdraw", json!({"amount": left.to_string()}), 1).await?; }
        let _ = c.call(&c.alice, &c.tape.clone(), "unfreeze", json!({}), 0).await;
        // and The Machine cashes out earlier runs' call income, so this run's number is this run's
        let m = (money(&c.view("account", json!({"owner": c.machine.id()})).await?["balance"]) * D).round() as u64;
        if m > 0 { c.call(&c.machine, &c.tape.clone(), "withdraw", json!({"amount": m.to_string()}), 1).await?; }
    }

    // stage: test USDC for Alice and the pool; the agent and keeper can hold USDC
    for a in [&c.machine, &c.keeper] {
        c.call(&c.funder, &c.usdc, "register", json!({"account_id": a.id()}), 0).await?;
    }
    let price = day["call_price"].as_u64().unwrap();
    let dep = day["member"]["deposit"].as_u64().unwrap();
    c.call(&c.funder, &c.usdc, "mint", json!({"account_id": c.alice.id(), "amount": (dep + 3 * price).to_string()}), 0).await?;
    if money(&c.view("pool", json!({})).await?) < 20_000.0 {
        c.call(&c.funder, &c.usdc, "mint", json!({"account_id": c.ops.id(), "amount": "20000000000"}), 0).await?;
        c.deposit(&c.ops, 20_000_000_000, "pool").await?;
    }
    c.price(prices[0].0, prices[0].1).await?;

    // 1. The Machine's record before the night: five calls, each locked before the move and graded 4 hours after
    println!("  ── before the night: The Machine's record ─────────────────────────────");
    let mut seeds = vec![];
    for (i, s) in day["seeds"].as_array().unwrap().iter().enumerate() {
        let at = s["at"].as_u64().unwrap();
        let side = s["side"].as_str().unwrap();
        let e = px_at(&prices, at);
        let p = payload(side, e, e * 97 / 100, e * 103 / 100, &format!("seed{i}-{run_tag}"));
        c.price(at, e).await?;
        let (id, _) = c.call(&c.machine, &c.tape.clone(), "commit", json!({"hash": tape_calls::hash_hex(&p), "market": "NEAR", "side": side, "price": price.to_string()}), 0).await?;
        c.call(&c.machine, &c.tape.clone(), "reveal", json!({"id": id}), 0).await?;
        seeds.push((id.as_u64().unwrap(), at, e, p));
    }
    for (id, at, e, p) in &seeds {
        let g = at + 4 * 3600;
        let x = px_at(&prices, g);
        c.price(g, x).await?;
        let (o, l) = c.call(&c.grader, &c.tape.clone(), "grade", json!({"id": id, "payload": p, "px_entry": e, "px_4h": x}), 0).await?;
        println!("  {}  call #{id} graded {:<5}  {:.4} → {:.4}   {l}", hhmm(g), o.as_str().unwrap_or("?"), *e as f64 / D, x as f64 / D);
    }
    let rec = c.view("record", json!({"agent": c.machine.id(), "window_n": 5})).await?;
    println!("  record, last 5: {} right of {}\n", rec[1], rec[0]);

    // 2. Alice funds and sets the rules — a policy, not her keys
    let m = &day["member"];
    c.price(m["at"].as_u64().unwrap(), px_at(&prices, m["at"].as_u64().unwrap())).await?;
    let l = c.deposit(&c.alice, dep, "deposit").await?;
    println!("  ── the night ───────────────────────────────────────────────────────────");
    println!("  {}  Alice deposits ${:.2}   {l}", hhmm(m["at"].as_u64().unwrap()), dep as f64 / D);
    let (_, l) = c.call(&c.alice, &c.tape.clone(), "grant", json!({"agent": c.machine.id(), "policy": m["policy"]}), 0).await?;
    println!("  {}  Alice gives The Machine a policy, not her keys: NEAR only · ≤ 2.5x · a stop on every order ·", hhmm(m["at"].as_u64().unwrap()));
    println!("               freeze at −10% · record ≥ 55% of its last 5 · every trade on a published call   {l}");

    // 3. The calls, the trades, the stops, the refusal
    let mut open: Vec<(String, u64, u64, u64)> = vec![]; // (name, position, stop, target)
    let mut graded_later: Vec<(u64, u64, u64, String, String)> = vec![]; // (id, reveal_at, entry, payload, name)
    let calls = day["calls"].as_array().unwrap().clone();
    let mut ci = 0;
    for &(ts, px) in &prices {
        // stops and targets first: the keeper fires them on the first recorded price through the level
        let mut fired = vec![];
        for (k, (name, pos, stop, target)) in open.iter().enumerate() {
            if px <= *stop || px >= *target {
                c.price(ts, px).await?;
                let (t, l) = c.call(&c.keeper, &c.tape.clone(), "trigger", json!({"position_id": pos}), 0).await?;
                let a = c.view("account", json!({"owner": c.alice.id()})).await?;
                println!("  {}  NEAR {:.4} — call {name}: {} hit. Alice's balance ${:.2}   {l}", hhmm(ts), px as f64 / D,
                         t.as_str().unwrap_or("?").to_lowercase(), money(&a["balance"]));
                fired.push(k);
            }
        }
        for k in fired.into_iter().rev() { open.remove(k); }
        // grades due
        let due: Vec<_> = graded_later.iter().filter(|g| g.1 + 4 * 3600 <= ts).cloned().collect();
        for g in due {
            c.price(ts, px).await?;
            let (o, l) = c.call(&c.grader, &c.tape.clone(), "grade", json!({"id": g.0, "payload": g.3, "px_entry": g.2, "px_4h": px}), 0).await?;
            println!("  {}  call {} graded {} ({:.4} → {:.4}); its levels are now public and match the hash   {l}", hhmm(ts), g.4, o.as_str().unwrap_or("?"), g.2 as f64 / D, px as f64 / D);
            graded_later.retain(|x| x.0 != g.0);
        }
        // the next call
        if ci < calls.len() && calls[ci]["at"].as_u64().unwrap() <= ts {
            let k = &calls[ci];
            ci += 1;
            let name = k["name"].as_str().unwrap().to_string();
            let side = k["side"].as_str().unwrap();
            let (stop, target) = (k["stop"].as_u64().unwrap(), k["target"].as_u64().unwrap());
            c.price(ts, px).await?;
            let p = payload(side, px, stop, target, &format!("{name}-{run_tag}"));
            let (id, l) = c.call(&c.machine, &c.tape.clone(), "commit", json!({"hash": tape_calls::hash_hex(&p), "market": "NEAR", "side": side, "price": price.to_string()}), 0).await?;
            let id = id.as_u64().unwrap();
            c.call(&c.machine, &c.tape.clone(), "reveal", json!({"id": id}), 0).await?;
            println!("  {}  NEAR {:.4} — The Machine locks call {name} ({}) on chain before the move   {l}", hhmm(ts), px as f64 / D, side.to_lowercase());
            let l = c.deposit(&c.alice, price, &format!("buy:{id}")).await?;
            println!("               Alice pays $5 for it and sees the levels: stop {:.2}, target {:.2}   {l}", stop as f64 / D, target as f64 / D);
            let (r, l) = c.call(&c.machine, &c.tape.clone(), "agent_open", json!({"owner": c.alice.id(), "market": "NEAR", "side": side,
                "qty": k["qty"], "collateral": k["collateral"].as_u64().unwrap().to_string(), "stop_px": stop, "target_px": target, "call_id": id}), 0).await?;
            if let Some(pos) = r["position"].as_u64() {
                println!("               The Machine opens it with ${:.0} of her money, inside her policy   {l}", k["collateral"].as_u64().unwrap() as f64 / D);
                open.push((name.clone(), pos, stop, target));
            } else {
                let a = c.view("account", json!({"owner": c.alice.id()})).await?;
                println!("               REFUSED ON CHAIN: {} — her balance ${:.2} is under 90% of her ${:.2} peak. Nothing moves.   {l}",
                         r["refused"].as_str().unwrap_or("?"), money(&a["balance"]), money(&a["peak"]));
            }
            graded_later.push((id, ts, px, p, name));
        }
        if ts >= day["end"].as_u64().unwrap() && graded_later.is_empty() && open.is_empty() { break; }
    }

    // 4. She takes her money home
    println!("\n  ── the morning ─────────────────────────────────────────────────────────");
    let a = c.view("account", json!({"owner": c.alice.id()})).await?;
    let bal = (money(&a["balance"]) * D).round() as u64;
    let (_, l) = c.call(&c.alice, &c.tape.clone(), "revoke", json!({"agent": c.machine.id()}), 0).await?;
    println!("  Alice revokes The Machine   {l}");
    let (_, l) = c.call(&c.alice, &c.tape.clone(), "withdraw", json!({"amount": bal.to_string()}), 1).await?;
    println!("  Alice withdraws ${:.2} — to herself; nobody else can   {l}", bal as f64 / D);
    let m_bal = money(&c.view("account", json!({"owner": c.machine.id()})).await?["balance"]);
    println!("  The Machine keeps what she paid it for calls: ${m_bal:.2}. It never held her keys.");
    let rec = c.view("record", json!({"agent": c.machine.id(), "window_n": 8})).await?;
    println!("  The Machine's public record, last 8: {} right of {} — misses included.\n", rec[1], rec[0]);
    let _ = c.call(&c.alice, &c.tape.clone(), "unfreeze", json!({}), 0).await; // ready for the next run
    Ok(())
}

async fn sandbox_cast(root: &std::path::Path) -> Result<Cast> {
    let wasm = |n: &str| std::fs::read(root.join(format!("../target/near/{n}/{n}.wasm"))).with_context(|| format!("build first: make wasm ({n})"));
    let w: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let usdc = w.dev_deploy(&wasm("usdc_mock")?).await?;
    usdc.call("new").transact().await?.into_result()?;
    let tape = w.dev_deploy(&wasm("tape")?).await?;
    let mut a = vec![];
    for _ in 0..6 { a.push(w.dev_create_account().await?); }
    tape.call("new").args_json(json!({"usdc": usdc.id(), "keeper": a[0].id(), "grader": a[1].id(), "demo": true})).transact().await?.into_result()?;
    usdc.call("register").args_json(json!({"account_id": tape.id()})).transact().await?.into_result()?;
    Ok(Cast { tape: tape.id().clone(), usdc: usdc.id().clone(), keeper: a[0].clone(), grader: a[1].clone(), machine: a[2].clone(),
              alice: a[3].clone(), ops: a[4].clone(), funder: a[5].clone(), explorer: None, shift: 0, last_ts: 0 })
}

async fn testnet_cast() -> Result<Cast> {
    // TAPE_RPC picks the testnet server (the public rpc.testnet.near.org throttles a 45-transaction run)
    let rpc = std::env::var("TAPE_RPC").unwrap_or_else(|_| "https://test.rpc.fastnear.com".to_string());
    let w: Worker<Testnet> = near_workspaces::testnet().rpc_addr(&rpc).await?;
    let dir = std::env::var("TAPE_KEYS").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".near-credentials/testnet"));
    let acct = |name: &str| -> Result<Account> {
        let id = format!("{name}.tape-nvibe.testnet");
        let f: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{id}.json"))).with_context(|| format!("no key for {id} in {}", dir.display()))?)?;
        let sk = SecretKey::from_str(f["private_key"].as_str().ok_or_else(|| anyhow!("no private_key for {id}"))?)?;
        Ok(Account::from_secret_key(AccountId::from_str(&id)?, sk, &w))
    };
    Ok(Cast { tape: "app.tape-nvibe.testnet".parse()?, usdc: "usdc.tape-nvibe.testnet".parse()?, keeper: acct("keeper")?, grader: acct("grader")?,
              machine: acct("machine")?, alice: acct("alice")?, ops: acct("ops")?, funder: acct("ops")?,
              explorer: Some("https://testnet.nearblocks.io/txns/"), shift: 0, last_ts: 0 })
}
