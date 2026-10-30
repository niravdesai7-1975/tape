//! `calls` — an agent's call is committed (a hash) before the market moves, revealed to the people who pay for it, and
//! graded four hours later by the grader key. The record is public; the levels are not. `guard` reads
//! `record(agent, window_n)` as its reputation input. The core is plain Rust and unit-tested; the NEAR contract wrapper
//! (storage, NEP-141 payment, events, the encrypted/served payload) sits on top (Nov 1+).
//!
//! The side (long/short) is public at commit — the push already says "NEAR · long · $5"; the levels are the paid part.
//! SCAFFOLDING ONLY before Nov 1: signatures and types are the spec; bodies are `todo!()` so the tests compile and fail.

pub const BP: u128 = 10_000;
pub const GRADE_AFTER_S: u64 = 4 * 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Long, Short }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome { Right, Wrong, Flat }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grade { pub outcome: Outcome, pub px_entry: u64, pub px_4h: u64, pub ts: u64 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub id: u64, pub agent: String, pub market: String, pub side: Side, pub commit_hash: String, pub commit_ts: u64,
    pub price: u128, pub reveal_ts: Option<u64>, pub payload: Option<String>, pub grade: Option<Grade>,
    pub fill_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallError { NoSuchCall, NotAgent, NotGrader, NotVenue, Duplicate, AlreadyRevealed, HashMismatch,
                     NotRevealed, TooEarly, AlreadyGraded, NotAllowed }

/// sha256 of the payload, lowercase hex — what the agent commits.
pub fn hash_hex(payload: &str) -> String { let _ = payload; todo!() }

/// Long: right when px_4h is above the entry by more than flat_bp; wrong when below by more; flat inside. Short mirrors.
pub fn outcome(side: Side, px_entry: u64, px_4h: u64, flat_bp: u32) -> Outcome { let _ = (side, px_entry, px_4h, flat_bp); todo!() }

pub struct Book {
    pub calls: Vec<Call>,
    pub grader: String,
    pub venue: String,
    pub allowlist: Vec<String>,
    pub buyers: Vec<(String, u64)>,
    pub flat_bp: u32,
}

impl Book {
    pub fn new(grader: &str, venue: &str, allowlist: &[&str], flat_bp: u32) -> Book { let _ = (grader, venue, allowlist, flat_bp); todo!() }
    /// The agent locks a call. A hash already committed and not yet graded cannot be committed again.
    pub fn commit(&mut self, agent: &str, hash: &str, market: &str, side: Side, price: u128, now: u64) -> Result<u64, CallError> { let _ = (agent, hash, market, side, price, now); todo!() }
    /// Only the call's agent; sha256(payload) must equal the committed hash.
    pub fn reveal(&mut self, caller: &str, id: u64, payload: &str, now: u64) -> Result<(), CallError> { let _ = (caller, id, payload, now); todo!() }
    /// An allowlisted member buys read access: Some((agent, price)) to pay; a second buy is a no-op (None).
    pub fn buy(&mut self, member: &str, id: u64) -> Result<Option<(String, u128)>, CallError> { let _ = (member, id); todo!() }
    /// Only the grader, only after the reveal + 4h; once.
    pub fn grade(&mut self, caller: &str, id: u64, px_entry: u64, px_4h: u64, now: u64) -> Result<Outcome, CallError> { let _ = (caller, id, px_entry, px_4h, now); todo!() }
    /// The payload only to buyers and the agent; the hash, times and grade are public.
    pub fn payload_for(&self, viewer: &str, id: u64) -> Option<String> { let _ = (viewer, id); todo!() }
    /// The venue attaches a fill to a call it was traded on.
    pub fn traded(&mut self, caller: &str, id: u64, fill_id: &str) -> Result<(), CallError> { let _ = (caller, id, fill_id); todo!() }
    /// The agent's last `window_n` graded calls (by grade time): (n, right). Flat counts in n, not in right.
    pub fn record(&self, agent: &str, window_n: u32) -> (u32, u32) { let _ = (agent, window_n); todo!() }
}

#[cfg(test)]
mod tests;
