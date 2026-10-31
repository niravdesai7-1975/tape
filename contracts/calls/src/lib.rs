//! `calls` — an agent's call is committed (a hash) before the market moves, revealed to the people who pay for it, and
//! graded four hours later by the grader key. A public chain hides nothing, so the levels never go on chain before the
//! grade: buyers get them from the app (which checks `has_bought`), and the grade publishes them on chain, where they must
//! match the hash committed before the move. The record is public. `guard` reads
//! `record(agent, window_n)` as its reputation input. The core is plain Rust and unit-tested; the NEAR contract wrapper
//! (storage, NEP-141 payment, events, the encrypted/served payload) sits on top.
//!
//! The side (long/short) is public at commit — the push already says "NEAR · long · $5"; the levels are the paid part.

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
pub fn hash_hex(payload: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(payload.as_bytes()))
}

/// Long: right when px_4h is above the entry by more than flat_bp; wrong when below by more; flat inside. Short mirrors.
pub fn outcome(side: Side, px_entry: u64, px_4h: u64, flat_bp: u32) -> Outcome {
    let mv = px_4h as i128 - px_entry as i128;
    let mv = match side { Side::Long => mv, Side::Short => -mv };
    if mv.unsigned_abs() * BP <= px_entry as u128 * flat_bp as u128 {
        Outcome::Flat
    } else if mv > 0 {
        Outcome::Right
    } else {
        Outcome::Wrong
    }
}

pub struct Book {
    pub calls: Vec<Call>,
    pub grader: String,
    pub venue: String,
    pub allowlist: Vec<String>,
    pub buyers: Vec<(String, u64)>,
    pub flat_bp: u32,
}

impl Book {
    pub fn new(grader: &str, venue: &str, allowlist: &[&str], flat_bp: u32) -> Book {
        Book { calls: Vec::new(), grader: grader.to_string(), venue: venue.to_string(),
               allowlist: allowlist.iter().map(|a| a.to_string()).collect(), buyers: Vec::new(), flat_bp }
    }
    /// The agent locks a call. A hash already committed and not yet graded cannot be committed again.
    pub fn commit(&mut self, agent: &str, hash: &str, market: &str, side: Side, price: u128, now: u64) -> Result<u64, CallError> {
        if self.calls.iter().any(|c| c.commit_hash == hash && c.grade.is_none()) {
            return Err(CallError::Duplicate);
        }
        let id = self.calls.len() as u64;
        self.calls.push(Call { id, agent: agent.to_string(), market: market.to_string(), side, commit_hash: hash.to_string(),
                               commit_ts: now, price, reveal_ts: None, payload: None, grade: None, fill_id: None });
        Ok(id)
    }
    /// Only the call's agent, once: the moment the call goes out to paying members. Nothing readable goes on chain.
    pub fn reveal(&mut self, caller: &str, id: u64, now: u64) -> Result<(), CallError> {
        let c = self.calls.get_mut(id as usize).ok_or(CallError::NoSuchCall)?;
        if c.agent != caller {
            return Err(CallError::NotAgent);
        }
        if c.reveal_ts.is_some() {
            return Err(CallError::AlreadyRevealed);
        }
        c.reveal_ts = Some(now);
        Ok(())
    }
    /// An allowlisted member buys read access: Some((agent, price)) to pay; a second buy is a no-op (None).
    pub fn buy(&mut self, member: &str, id: u64) -> Result<Option<(String, u128)>, CallError> {
        let c = self.calls.get(id as usize).ok_or(CallError::NoSuchCall)?;
        if !self.allowlist.iter().any(|a| a == member) {
            return Err(CallError::NotAllowed);
        }
        if self.bought(member, id) {
            return Ok(None);
        }
        let pay = (c.agent.clone(), c.price);
        self.buyers.push((member.to_string(), id));
        Ok(Some(pay))
    }
    /// Only the grader, only after the reveal + 4h, once; publishes the levels, which must hash to the commit.
    pub fn grade(&mut self, caller: &str, id: u64, payload: &str, px_entry: u64, px_4h: u64, now: u64) -> Result<Outcome, CallError> {
        if caller != self.grader {
            return Err(CallError::NotGrader);
        }
        let flat_bp = self.flat_bp;
        let c = self.calls.get_mut(id as usize).ok_or(CallError::NoSuchCall)?;
        if c.grade.is_some() {
            return Err(CallError::AlreadyGraded);
        }
        let revealed = c.reveal_ts.ok_or(CallError::NotRevealed)?;
        if now < revealed + GRADE_AFTER_S {
            return Err(CallError::TooEarly);
        }
        if hash_hex(payload) != c.commit_hash {
            return Err(CallError::HashMismatch);
        }
        let o = outcome(c.side, px_entry, px_4h, flat_bp);
        c.grade = Some(Grade { outcome: o, px_entry, px_4h, ts: now });
        c.payload = Some(payload.to_string());
        Ok(o)
    }
    /// Whether a member paid for a call — the app serves the levels to buyers off chain on this.
    pub fn has_bought(&self, member: &str, id: u64) -> bool {
        self.bought(member, id)
    }
    /// The levels, public once graded (and checkable against the commit hash); nothing before.
    pub fn published(&self, id: u64) -> Option<String> {
        self.calls.get(id as usize).and_then(|c| c.grade.as_ref().and(c.payload.clone()))
    }
    /// The venue attaches a fill to a call it was traded on.
    pub fn traded(&mut self, caller: &str, id: u64, fill_id: &str) -> Result<(), CallError> {
        if caller != self.venue {
            return Err(CallError::NotVenue);
        }
        let c = self.calls.get_mut(id as usize).ok_or(CallError::NoSuchCall)?;
        c.fill_id = Some(fill_id.to_string());
        Ok(())
    }
    fn bought(&self, member: &str, id: u64) -> bool {
        self.buyers.iter().any(|(m, i)| m == member && *i == id)
    }

    /// The agent's last `window_n` graded calls (by grade time): (n, right). Flat counts in n, not in right.
    pub fn record(&self, agent: &str, window_n: u32) -> (u32, u32) {
        let mut graded: Vec<&Grade> = self.calls.iter().filter(|c| c.agent == agent).filter_map(|c| c.grade.as_ref()).collect();
        graded.sort_by_key(|g| g.ts);
        let window: Vec<&&Grade> = graded.iter().rev().take(window_n as usize).collect();
        let right = window.iter().filter(|g| g.outcome == Outcome::Right).count();
        (window.len() as u32, right as u32)
    }
}

#[cfg(test)]
mod tests;
