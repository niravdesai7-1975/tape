//! `tape` — one NEAR contract built from the three tested cores: `calls` (the record), `guard` (the agent wallet) and
//! `venue` (oracle-priced perps). One contract on purpose: the guard's check and the venue's fill happen in the same
//! transaction, so nothing can change between "allowed" and "traded".
//!
//! Money: NEP-141 USDC (6 decimals), sent with `ft_transfer_call` and a message — "deposit" (your wallet), "pool" (the
//! venue's counterparty) or "buy:<call id>" (pay an agent for a call). Money leaves only through `withdraw`, to the caller,
//! from the caller's own free balance.
//! Every dollar the contract holds is a free balance, a position's collateral, or the pool (`v14` reconciles it).
use near_sdk::json_types::U128;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{env, log, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseOrValue};

use tape_calls as calls;
use tape_guard as guard;
use tape_venue as venue;

const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const GAS_FT: Gas = Gas::from_tgas(10);
const GAS_CB: Gas = Gas::from_tgas(30);

#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Long, Short }

impl Side {
    fn v(self) -> venue::Side { if self == Side::Long { venue::Side::Long } else { venue::Side::Short } }
    fn g(self) -> guard::Side { if self == Side::Long { guard::Side::Long } else { guard::Side::Short } }
    fn c(self) -> calls::Side { if self == Side::Long { calls::Side::Long } else { calls::Side::Short } }
}

/// A position plus who opened it (an agent under a grant, or the owner directly).
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Pos { pub id: u64, pub p: venue::Position, pub opened_by: AccountId }

#[near(serializers = [json])]
pub struct AccountView { pub balance: U128, pub locked: U128, pub peak: U128, pub frozen: bool }

#[near(serializers = [json])]
pub struct OpenResult { pub position: Option<u64>, pub refused: Option<String> }

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Grant { pub policy: guard::Policy, pub revoked: bool }

/// An allowed ask: the order the chain cleared before it went to the member's own venue (her Coinbase account).
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Ticket { pub id: u64, pub owner: AccountId, pub agent: AccountId, pub call_id: Option<u64>, pub side: Side,
                    pub notional: u128, pub collateral: u128, pub open: bool, pub refs: Vec<String> }

#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillKind { Open, Close }

#[near(serializers = [json])]
pub struct AskResult { pub ticket: Option<u64>, pub refused: Option<String> }

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Tape {
    usdc: AccountId,
    keeper: AccountId,
    demo: bool,
    market: venue::Market,
    oracle: Option<venue::Oracle>,
    book: calls::Book,
    accounts: LookupMap<AccountId, guard::Account>,
    grants: LookupMap<(AccountId, AccountId), Grant>,
    used_calls: LookupMap<(AccountId, u64), bool>,
    positions: IterableMap<u64, Pos>,
    next_pos: u64,
    pool: u128,
    events: Vec<String>,
    tickets: IterableMap<u64, Ticket>,
    next_ticket: u64,
}

const RECENT: usize = 100; // newest events kept in state for the app

/// The state before asks and receipts (deployed Nov 2, 2026); `migrate` carries it over.
#[near(serializers = [borsh])]
pub struct CallV1 {
    id: u64, agent: String, market: String, side: calls::Side, commit_hash: String, commit_ts: u64, price: u128,
    reveal_ts: Option<u64>, payload: Option<String>, grade: Option<calls::Grade>, fill_id: Option<String>,
}
#[near(serializers = [borsh])]
pub struct BookV1 { calls: Vec<CallV1>, grader: String, venue: String, allowlist: Vec<String>, buyers: Vec<(String, u64)>, flat_bp: u32 }
#[near(serializers = [borsh])]
pub struct TapeV1 {
    usdc: AccountId, keeper: AccountId, demo: bool, market: venue::Market, oracle: Option<venue::Oracle>, book: BookV1,
    accounts: LookupMap<AccountId, guard::Account>, grants: LookupMap<(AccountId, AccountId), Grant>,
    used_calls: LookupMap<(AccountId, u64), bool>, positions: IterableMap<u64, Pos>, next_pos: u64, pool: u128, events: Vec<String>,
}

fn block_s() -> u64 { env::block_timestamp() / 1_000_000_000 }

#[near]
impl Tape {
    #[init]
    pub fn new(usdc: AccountId, keeper: AccountId, grader: AccountId, demo: bool) -> Self {
        let me = env::current_account_id();
        Self {
            usdc, keeper, demo, market: venue::Market::near(), oracle: None,
            book: calls::Book::new(grader.as_str(), me.as_str(), &[], 10),
            accounts: LookupMap::new(b"a"), grants: LookupMap::new(b"g"), used_calls: LookupMap::new(b"u"),
            positions: IterableMap::new(b"p"), next_pos: 0, pool: 0, events: Vec::new(),
            tickets: IterableMap::new(b"t"), next_ticket: 0,
        }
    }

    /// Owner-only, once: the deployed state gains the ticket book.
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let v1: TapeV1 = env::state_read().unwrap_or_else(|| env::panic_str("no state"));
        let b = v1.book;
        let book = calls::Book { grader: b.grader, venue: b.venue, allowlist: b.allowlist, buyers: b.buyers, flat_bp: b.flat_bp,
            calls: b.calls.into_iter().map(|c| calls::Call { id: c.id, agent: c.agent, market: c.market, side: c.side, commit_hash: c.commit_hash,
                commit_ts: c.commit_ts, price: c.price, reveal_ts: c.reveal_ts, payload: c.payload, grade: c.grade, fill_id: c.fill_id, fills: Vec::new() }).collect() };
        Self { usdc: v1.usdc, keeper: v1.keeper, demo: v1.demo, market: v1.market, oracle: v1.oracle, book, accounts: v1.accounts,
               grants: v1.grants, used_calls: v1.used_calls, positions: v1.positions, next_pos: v1.next_pos, pool: v1.pool, events: v1.events,
               tickets: IterableMap::new(b"t"), next_ticket: 0 }
    }

    // ---------- money in / out ----------

    /// NEP-141 receiver. Returns the unused amount (refunded by the token).
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        require!(env::predecessor_account_id() == self.usdc, "only USDC");
        let amount = amount.0;
        if msg == "deposit" {
            let mut a = self.acct(&sender_id);
            a.deposit(amount);
            self.accounts.insert(sender_id.clone(), a);
            self.emit("deposit", near_sdk::serde_json::json!({"owner": sender_id, "amount": amount.to_string()}));
            return PromiseOrValue::Value(U128(0));
        }
        if msg == "pool" {
            self.pool += amount;
            return PromiseOrValue::Value(U128(0));
        }
        if let Some(id) = msg.strip_prefix("buy:").and_then(|s| s.parse::<u64>().ok()) {
            return match self.book.buy(sender_id.as_str(), id) {
                Ok(Some((agent, price))) if price <= amount => {
                    let agent: AccountId = agent.parse().unwrap();
                    let mut a = self.acct(&agent);
                    a.balance += price;
                    self.accounts.insert(agent.clone(), a);
                    self.emit("bought", near_sdk::serde_json::json!({"member": sender_id, "call": id, "agent": agent, "price": price.to_string()}));
                    PromiseOrValue::Value(U128(amount - price))
                }
                _ => PromiseOrValue::Value(U128(amount)),                  // not allowed, already bought, or too little: refund
            };
        }
        PromiseOrValue::Value(U128(amount))
    }

    /// Pay for a call from your own fee float on the contract: the price goes to the call's agent.
    pub fn buy(&mut self, id: u64) {
        let member = env::predecessor_account_id();
        let (agent, price) = match self.book.buy(member.as_str(), id) {
            Ok(Some(p)) => p,
            Ok(None) => return,                                            // already bought
            Err(e) => env::panic_str(&format!("{e:?}")),
        };
        let mut a = self.acct(&member);
        require!(a.balance >= price, "not enough on your float");
        a.balance -= price;
        self.accounts.insert(member.clone(), a);
        let agent: AccountId = agent.parse().unwrap();
        let mut g = self.acct(&agent);
        g.balance += price;
        self.accounts.insert(agent.clone(), g);
        self.emit("bought", near_sdk::serde_json::json!({"member": member, "call": id, "agent": agent, "price": price.to_string()}));
    }

    /// Only your own free balance, only to you.
    #[payable]
    pub fn withdraw(&mut self, amount: U128) -> Promise {
        near_sdk::assert_one_yocto();
        let me = env::predecessor_account_id();
        let mut a = self.acct(&me);
        if let Err(e) = a.withdraw(me.as_str(), me.as_str(), amount.0) { env::panic_str(&format!("{e:?}")); }
        self.accounts.insert(me.clone(), a);
        self.emit("withdraw", near_sdk::serde_json::json!({"owner": me, "amount": amount.0.to_string()}));
        Promise::new(self.usdc.clone())
            .function_call("ft_transfer".to_string(), near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({"receiver_id": me, "amount": amount})).unwrap(), ONE_YOCTO, GAS_FT)
            .then(Promise::new(env::current_account_id()).function_call("on_withdraw".to_string(),
                near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({"owner": me, "amount": amount})).unwrap(), NearToken::from_yoctonear(0), GAS_CB))
    }

    /// If the token refused the transfer, the money comes back to the balance.
    #[private]
    pub fn on_withdraw(&mut self, owner: AccountId, amount: U128) {
        if env::promise_result_checked(0, 64).is_err() {
            let mut a = self.acct(&owner);
            a.balance += amount.0;
            self.accounts.insert(owner, a);
        }
    }

    // ---------- the record (calls) ----------

    /// `note`: the public part of the call — what the agent saw (facts, no levels); the levels stay in the hash.
    pub fn commit(&mut self, hash: String, market: String, side: Side, price: U128, note: Option<String>) -> u64 {
        let agent = env::predecessor_account_id();
        let id = self.book.commit(agent.as_str(), &hash, &market, side.c(), price.0, self.now()).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.emit("committed", near_sdk::serde_json::json!({"id": id, "agent": agent, "market": market, "side": side, "hash": hash, "note": note}));
        id
    }

    pub fn reveal(&mut self, id: u64) {
        let agent = env::predecessor_account_id();
        self.book.reveal(agent.as_str(), id, self.now()).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.emit("revealed", near_sdk::serde_json::json!({"id": id}));
    }

    pub fn grade(&mut self, id: u64, payload: String, px_entry: u64, px_4h: u64) -> calls::Outcome {
        let caller = env::predecessor_account_id();
        let o = self.book.grade(caller.as_str(), id, &payload, px_entry, px_4h, self.now()).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.emit("graded", near_sdk::serde_json::json!({"id": id, "outcome": o, "px_entry": px_entry, "px_4h": px_4h, "levels": payload}));
        o
    }

    pub fn record(&self, agent: AccountId, window_n: u32) -> (u32, u32) { self.book.record(agent.as_str(), window_n) }
    pub fn has_bought(&self, member: AccountId, id: u64) -> bool { self.book.has_bought(member.as_str(), id) }
    pub fn published(&self, id: u64) -> Option<String> { self.book.published(id) }
    pub fn call(&self, id: u64) -> Option<calls::Call> {
        // the public view never carries the levels before the grade
        self.book.calls.get(id as usize).cloned().map(|mut c| { if c.grade.is_none() { c.payload = None; } c })
    }

    // ---------- the agent wallet (guard) ----------

    pub fn grant(&mut self, agent: AccountId, policy: guard::Policy) {
        let owner = env::predecessor_account_id();
        self.grants.insert((owner.clone(), agent.clone()), Grant { policy, revoked: false });
        self.emit("granted", near_sdk::serde_json::json!({"owner": owner, "agent": agent}));
    }

    pub fn revoke(&mut self, agent: AccountId) {
        let owner = env::predecessor_account_id();
        if let Some(g) = self.grants.get_mut(&(owner.clone(), agent.clone())) { g.revoked = true; }
        self.emit("revoked", near_sdk::serde_json::json!({"owner": owner, "agent": agent}));
    }

    pub fn unfreeze(&mut self) {
        let owner = env::predecessor_account_id();
        let eq = self.equity(&owner);
        let mut a = self.acct(&owner);
        a.unfreeze(owner.as_str(), eq).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.accounts.insert(owner, a);
    }

    /// The agent asks to trade the owner's money. Refusals are returned and logged, never silent; nothing moves.
    #[allow(clippy::too_many_arguments)]
    pub fn agent_open(&mut self, owner: AccountId, market: String, side: Side, qty: u32, collateral: U128,
                      stop_px: Option<u64>, target_px: Option<u64>, call_id: Option<u64>) -> OpenResult {
        let agent = env::predecessor_account_id();
        let Some(grant) = self.grants.get(&(owner.clone(), agent.clone())).cloned() else { return self.refuse(&owner, &agent, "REVOKED") };
        // mark the owner's equity first: the freeze is a condition on money, checked before every order
        let eq = self.equity(&owner);
        let mut a = self.acct(&owner);
        let was = a.frozen;
        a.mark(eq, grant.policy.drawdown_freeze_bp);
        if a.frozen && !was { self.emit("frozen", near_sdk::serde_json::json!({"owner": owner, "equity": eq.to_string(), "peak": a.peak.to_string()})); }
        let frozen = a.frozen;
        self.accounts.insert(owner.clone(), a);

        let ox = self.oracle.unwrap_or_else(|| env::panic_str("no price"));
        let fill = venue::fill_px(&self.market, ox.px, side == Side::Long);
        let notional = venue::notional(&self.market, qty, fill);
        let exposure: u128 = self.positions.values().filter(|x| x.p.owner == owner.as_str() && x.opened_by == agent)
            .map(|x| venue::notional(&self.market, x.p.qty, ox.px)).sum();
        let (n, right) = self.book.record(agent.as_str(), grant.policy.window_n);
        let cv = call_id.and_then(|id| self.book.calls.get(id as usize)).map(|c| guard::CallView {
            id: c.id, agent: c.agent.clone(), market: c.market.clone(), side: if c.side == calls::Side::Long { guard::Side::Long } else { guard::Side::Short },
            revealed: c.reveal_ts.is_some(), graded: c.grade.is_some() });
        let used = call_id.map(|id| self.used_calls.contains_key(&(owner.clone(), id))).unwrap_or(false);
        let order = guard::Order { market: market.clone(), side: side.g(), qty, collateral: collateral.0, notional, fill_px: fill, stop_px, call_id };
        let ctx = guard::Ctx { agent: agent.as_str(), revoked: grant.revoked, frozen, open_exposure: exposure,
                               record: guard::Record { n, right }, call: cv.as_ref(), call_used_by_owner: used };
        if let Err(r) = guard::check(&grant.policy, &order, &ctx) {
            return self.refuse(&owner, &agent, code(r));
        }
        let id = self.open_for(&owner, &agent, side, qty, collateral.0, stop_px, target_px);
        if let Some(c) = call_id {
            self.used_calls.insert((owner.clone(), c), true);
            let me = env::current_account_id();
            let _ = self.book.traded(me.as_str(), c, &id.to_string());
        }
        OpenResult { position: Some(id), refused: None }
    }

    /// The agent closes a position it opened under a live grant.
    pub fn agent_close(&mut self, owner: AccountId, position_id: u64) -> U128 {
        let agent = env::predecessor_account_id();
        let x = self.positions.get(&position_id).cloned().unwrap_or_else(|| env::panic_str("no such position"));
        require!(x.p.owner == owner.as_str() && x.opened_by == agent, "not a position this agent opened for this owner");
        let g = self.grants.get(&(owner, agent)).unwrap_or_else(|| env::panic_str("no grant"));
        require!(!g.revoked, "revoked");
        let px = self.price();
        let s = venue::close(&self.market, &x.p, px);
        self.settle(position_id, &x, s, None, "closed")
    }

    // ---------- her money stays where she trades: asks and receipts ----------

    /// Before any order goes to the owner's own venue (her Coinbase account), the agent asks the chain, reporting her
    /// venue equity. The same policy engine answers; a refusal is an event with its code and nothing else happens.
    #[allow(clippy::too_many_arguments)]
    pub fn ask(&mut self, owner: AccountId, market: String, side: Side, qty: u32, collateral: U128, notional: U128,
               stop_px: Option<u64>, call_id: Option<u64>, equity: U128) -> AskResult {
        let agent = env::predecessor_account_id();
        let Some(grant) = self.grants.get(&(owner.clone(), agent.clone())).cloned() else {
            self.refuse(&owner, &agent, "REVOKED"); return AskResult { ticket: None, refused: Some("REVOKED".into()) } };
        let frozen = self.mark(&owner, equity.0, grant.policy.drawdown_freeze_bp);
        let exposure: u128 = self.tickets.values().filter(|t| t.owner == owner && t.agent == agent && t.open).map(|t| t.notional).sum();
        let (n, right) = self.book.record(agent.as_str(), grant.policy.window_n);
        let cv = call_id.and_then(|id| self.book.calls.get(id as usize)).map(|c| guard::CallView {
            id: c.id, agent: c.agent.clone(), market: c.market.clone(), side: if c.side == calls::Side::Long { guard::Side::Long } else { guard::Side::Short },
            revealed: c.reveal_ts.is_some(), graded: c.grade.is_some() });
        let used = call_id.map(|id| self.used_calls.contains_key(&(owner.clone(), id))).unwrap_or(false);
        let fill_px = if notional.0 > 0 && qty > 0 { (notional.0 * 1_000_000 / (qty as u128 * self.market.contract_size as u128)) as u64 } else { 0 };
        let order = guard::Order { market: market.clone(), side: side.g(), qty, collateral: collateral.0, notional: notional.0, fill_px, stop_px, call_id };
        let ctx = guard::Ctx { agent: agent.as_str(), revoked: grant.revoked, frozen, open_exposure: exposure,
                               record: guard::Record { n, right }, call: cv.as_ref(), call_used_by_owner: used };
        if let Err(r) = guard::check(&grant.policy, &order, &ctx) {
            self.refuse(&owner, &agent, code(r));
            return AskResult { ticket: None, refused: Some(code(r).to_string()) };
        }
        let id = self.next_ticket;
        self.next_ticket += 1;
        self.tickets.insert(id, Ticket { id, owner: owner.clone(), agent: agent.clone(), call_id, side, notional: notional.0, collateral: collateral.0, open: false, refs: Vec::new() });
        if let Some(c) = call_id { self.used_calls.insert((owner.clone(), c), true); }
        self.emit("asked", near_sdk::serde_json::json!({"ticket": id, "owner": owner, "agent": agent, "call": call_id, "market": market, "side": side,
            "qty": qty, "notional": notional.0.to_string(), "collateral": collateral.0.to_string(), "stop": stop_px, "equity": equity.0.to_string()}));
        AskResult { ticket: Some(id), refused: None }
    }

    /// The receipt of a fill on the owner's venue, against the ticket the ask returned. Only the agent that asked, once
    /// per order ref. A close carries the P&L and her equity after it, which is what can freeze her.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(&mut self, owner: AccountId, ticket: u64, venue: String, order_ref: String, kind: FillKind, px: u64, pnl: Option<String>, equity: U128) {
        let agent = env::predecessor_account_id();
        let mut t = self.tickets.get(&ticket).cloned().unwrap_or_else(|| env::panic_str("no such ticket"));
        require!(t.owner == owner && t.agent == agent, "not this agent's ticket for this owner");
        let fill_ref = format!("{venue}:{order_ref}");
        require!(!self.tickets.values().any(|x| x.refs.iter().any(|r| *r == fill_ref)), "duplicate receipt");
        t.refs.push(fill_ref.clone());
        if let (Some(c), FillKind::Open) = (t.call_id, kind) {
            self.book.filled(agent.as_str(), c, &fill_ref).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        }
        match kind {
            FillKind::Open => { require!(!t.open, "already open"); t.open = true; }
            FillKind::Close => { require!(t.open, "nothing open on this ticket"); t.open = false; }
        }
        self.tickets.insert(ticket, t);
        let g = self.grants.get(&(owner.clone(), agent.clone())).cloned();
        if kind == FillKind::Close { if let Some(g) = g { self.mark(&owner, equity.0, g.policy.drawdown_freeze_bp); } }
        self.emit("filled", near_sdk::serde_json::json!({"ticket": ticket, "owner": owner, "agent": agent, "venue": venue, "ref": order_ref,
            "kind": kind, "px": px, "pnl": pnl, "equity": equity.0.to_string()}));
    }

    /// The owner unfreezes, resetting the peak to where her money is now.
    pub fn unfreeze_at(&mut self, equity: U128) {
        let owner = env::predecessor_account_id();
        let mut a = self.acct(&owner);
        a.unfreeze(owner.as_str(), equity.0).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.accounts.insert(owner.clone(), a);
        self.emit("unfrozen", near_sdk::serde_json::json!({"owner": owner, "equity": equity.0.to_string()}));
    }

    pub fn ticket(&self, id: u64) -> Option<Ticket> { self.tickets.get(&id).cloned() }

    // ---------- the venue (direct use, keepers) ----------

    pub fn open(&mut self, market: String, side: Side, qty: u32, collateral: U128, stop_px: Option<u64>, target_px: Option<u64>) -> u64 {
        require!(market == self.market.id, "unknown market");
        let me = env::predecessor_account_id();
        self.open_for(&me, &me, side, qty, collateral.0, stop_px, target_px)
    }

    pub fn close(&mut self, position_id: u64) -> U128 {
        let me = env::predecessor_account_id();
        let x = self.positions.get(&position_id).cloned().unwrap_or_else(|| env::panic_str("no such position"));
        require!(x.p.owner == me.as_str(), "not yours");
        let px = self.price();
        let s = venue::close(&self.market, &x.p, px);
        self.settle(position_id, &x, s, None, "closed")
    }

    /// Anyone may fire a stop or target once the price has traded through it.
    pub fn trigger(&mut self, position_id: u64) -> venue::Trigger {
        let x = self.positions.get(&position_id).cloned().unwrap_or_else(|| env::panic_str("no such position"));
        let px = self.price();
        let (t, s) = venue::trigger(&self.market, &x.p, px).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        self.settle(position_id, &x, s, None, if t == venue::Trigger::Stop { "stop" } else { "target" });
        t
    }

    /// Anyone may liquidate below maintenance and keeps the bounty.
    pub fn liquidate(&mut self, position_id: u64) -> U128 {
        let x = self.positions.get(&position_id).cloned().unwrap_or_else(|| env::panic_str("no such position"));
        let px = self.price();
        let s = venue::liquidate(&self.market, &x.p, px).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        let caller = env::predecessor_account_id();
        self.settle(position_id, &x, s, Some(caller), "liquidated")
    }

    /// The keeper pushes the oracle price. Live: Pyth's price at block time. Demo: the recorded day's price with its own
    /// timestamp, which becomes the contract's clock (so "4 hours after the reveal" is measured on the replayed day). The
    /// replay clock only moves forward.
    pub fn push_price(&mut self, px: u64, conf: u64, ts: Option<u64>) {
        require!(env::predecessor_account_id() == self.keeper, "only the keeper");
        let ts = if self.demo {
            let ts = ts.unwrap_or_else(|| env::panic_str("demo prices carry their recorded time"));
            if let Some(o) = self.oracle { require!(ts > o.ts, "the replay clock only moves forward"); }
            ts
        } else {
            block_s()
        };
        self.oracle = Some(venue::Oracle { px, conf, ts, replay: self.demo });
    }

    // ---------- views ----------

    pub fn account(&self, owner: AccountId) -> AccountView {
        let a = self.accounts.get(&owner).cloned().unwrap_or_else(|| guard::Account::new(owner.as_str()));
        AccountView { balance: U128(a.balance), locked: U128(a.locked), peak: U128(a.peak), frozen: a.frozen }
    }
    pub fn positions_of(&self, owner: AccountId) -> Vec<Pos> { self.positions.values().filter(|x| x.p.owner == owner.as_str()).cloned().collect() }
    pub fn position(&self, id: u64) -> Option<Pos> { self.positions.get(&id).cloned() }
    pub fn pool(&self) -> U128 { U128(self.pool) }
    pub fn oracle(&self) -> Option<venue::Oracle> { self.oracle }
    /// The newest events, newest first — what the app shows.
    pub fn recent(&self, n: u32) -> Vec<near_sdk::serde_json::Value> {
        self.events.iter().rev().take(n as usize).filter_map(|e| near_sdk::serde_json::from_str(e).ok()).collect()
    }
    pub fn grant_of(&self, owner: AccountId, agent: AccountId) -> Option<Grant> { self.grants.get(&(owner, agent)).cloned() }
}

impl Tape {
    /// NEP-297 event in the logs, and kept (newest RECENT) for the app.
    fn emit(&mut self, kind: &str, data: near_sdk::serde_json::Value) {
        let e = near_sdk::serde_json::json!({"standard": "tape", "version": "1.0.0", "event": kind, "ts": self.now(), "data": data}).to_string();
        log!("EVENT_JSON:{}", e);
        self.events.push(e);
        if self.events.len() > RECENT { self.events.remove(0); }
    }

    /// Mark the owner's equity against her peak; the freeze is a condition on money. Returns whether she is frozen.
    fn mark(&mut self, owner: &AccountId, equity: u128, freeze_bp: u32) -> bool {
        let mut a = self.acct(owner);
        let was = a.frozen;
        a.mark(equity, freeze_bp);
        if a.frozen && !was { self.emit("frozen", near_sdk::serde_json::json!({"owner": owner, "equity": equity.to_string(), "peak": a.peak.to_string()})); }
        let f = a.frozen;
        self.accounts.insert(owner.clone(), a);
        f
    }

    fn refuse(&mut self, owner: &AccountId, agent: &AccountId, code: &str) -> OpenResult {
        self.emit("refused", near_sdk::serde_json::json!({"owner": owner, "agent": agent, "code": code}));
        OpenResult { position: None, refused: Some(code.to_string()) }
    }

    /// The contract's clock: block time live; the replay clock in demo mode.
    fn now(&self) -> u64 {
        if self.demo { self.oracle.map(|o| o.ts).unwrap_or(0) } else { block_s() }
    }

    fn acct(&self, who: &AccountId) -> guard::Account {
        self.accounts.get(who).cloned().unwrap_or_else(|| guard::Account::new(who.as_str()))
    }

    /// A fresh, sane oracle price or nothing happens.
    fn price(&self) -> u64 {
        let o = self.oracle.unwrap_or_else(|| env::panic_str("no price"));
        venue::check_oracle(&o, self.now(), self.demo).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        o.px
    }

    /// Free balance + collateral + unrealized at the oracle, over the owner's open positions.
    fn equity(&self, owner: &AccountId) -> u128 {
        let a = self.acct(owner);
        let px = self.oracle.map(|o| o.px).unwrap_or(0);
        let mut e = a.balance as i128;
        for x in self.positions.values().filter(|x| x.p.owner == owner.as_str()) {
            e += venue::equity(&self.market, &x.p, px).max(0);
        }
        e.max(0) as u128
    }

    #[allow(clippy::too_many_arguments)]
    fn open_for(&mut self, owner: &AccountId, by: &AccountId, side: Side, qty: u32, collateral: u128, stop_px: Option<u64>, target_px: Option<u64>) -> u64 {
        let px = self.price();
        let m = &self.market;
        venue::check_initial(m, qty, px, collateral).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        let open: u128 = self.positions.values().map(|x| venue::notional(m, x.p.qty, px)).sum();
        let fill = venue::fill_px(m, px, side == Side::Long);
        let notional = venue::notional(m, qty, fill);
        venue::check_capacity(open, notional, self.pool).unwrap_or_else(|e| env::panic_str(&format!("{e:?}")));
        let fee = venue::taker_fee(m, notional);
        let mut a = self.acct(owner);
        require!(a.balance >= collateral + fee, "not enough free balance");
        a.balance -= collateral + fee;
        a.locked += collateral;
        self.accounts.insert(owner.clone(), a);
        self.pool += fee;
        let id = self.next_pos;
        self.next_pos += 1;
        let p = venue::Position { owner: owner.to_string(), beneficiary: Some(owner.to_string()), market: m.id.clone(), side: side.v(),
                                  qty, entry_px: fill, collateral, stop_px, target_px };
        self.positions.insert(id, Pos { id, p, opened_by: by.clone() });
        self.emit("opened", near_sdk::serde_json::json!({"id": id, "owner": owner, "by": by, "side": side, "qty": qty, "px": fill,
                                                       "collateral": collateral.to_string(), "fee": fee.to_string(), "stop": stop_px, "target": target_px}));
        id
    }

    /// Pay out a closed position: collateral leaves `locked`; the owner gets what is left; a liquidator its bounty; the
    /// pool takes the rest (or pays a profit).
    fn settle(&mut self, id: u64, x: &Pos, s: venue::Settlement, liquidator: Option<AccountId>, how: &str) -> U128 {
        let owner: AccountId = x.p.owner.parse().unwrap();
        let mut a = self.acct(&owner);
        a.locked -= x.p.collateral;
        a.balance += s.to_owner;
        self.accounts.insert(owner.clone(), a);
        let mut paid = s.to_owner;
        if let Some(l) = liquidator {
            let mut b = self.acct(&l);
            b.balance += s.bounty;
            self.accounts.insert(l, b);
            paid += s.bounty;
        }
        self.pool = (self.pool + x.p.collateral).checked_sub(paid).unwrap_or_else(|| env::panic_str("pool exhausted"));
        self.positions.remove(&id);
        self.emit(how, near_sdk::serde_json::json!({"id": id, "owner": owner, "px": s.exit_px, "pnl": s.pnl.to_string(), "fee": s.fee.to_string(),
                                                  "to_owner": s.to_owner.to_string(), "bounty": s.bounty.to_string()}));
        U128(s.to_owner)
    }
}

fn code(r: guard::Refusal) -> &'static str {
    use guard::Refusal::*;
    match r {
        Revoked => "REVOKED", Frozen => "FROZEN", Market => "MARKET", Size => "SIZE", Collateral => "COLLATERAL",
        Leverage => "LEVERAGE", Exposure => "EXPOSURE", NoStop => "NO_STOP", StopSide => "STOP_SIDE", StopFar => "STOP_FAR",
        Call => "CALL", Record => "RECORD",
    }
}
