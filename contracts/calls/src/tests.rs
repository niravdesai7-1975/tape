//! Calls core tests, written before the code (SPEC.md §calls). C_09 (events) and C_10 (end to end) run against the
//! NEAR contract wrapper in `tests/` from Nov 1.
use super::*;

const H: u64 = 3600;
const PAYLOAD: &str = r#"{"market":"NEAR","side":"long","entry":4.86,"stop":4.50,"target":5.20}"#;

fn book() -> Book { Book::new("grader", "venue", &["alice", "bob"], 10) }

#[test]
fn c00_hash_is_sha256_hex() {
    assert_eq!(hash_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn c01_commit_stores_the_hash_and_time() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(id, 0);
    let c = &b.calls[0];
    assert_eq!((c.agent.as_str(), c.commit_ts, c.reveal_ts, c.payload.as_deref()), ("machine", 1_000, None, None));
    assert_eq!(b.commit("machine", &hash_hex("x"), "NEAR", Side::Short, 5_000_000, 1_001), Ok(1));
}

#[test]
fn c02_reveal_only_by_the_agent_and_only_the_committed_payload() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(b.reveal("eve", id, PAYLOAD, 1_100), Err(CallError::NotAgent));
    assert_eq!(b.reveal("machine", id, "{\"changed\":true}", 1_100), Err(CallError::HashMismatch));
    assert_eq!(b.reveal("machine", id, PAYLOAD, 1_100), Ok(()));
    assert_eq!(b.calls[0].reveal_ts, Some(1_100));
    assert_eq!(b.reveal("machine", id, PAYLOAD, 1_200), Err(CallError::AlreadyRevealed));
    assert_eq!(b.reveal("machine", 99, PAYLOAD, 1_200), Err(CallError::NoSuchCall));
}

#[test]
fn c03_buy_pays_the_agent_once_and_respects_the_allowlist() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(b.buy("mallory", id), Err(CallError::NotAllowed));
    assert_eq!(b.buy("alice", id), Ok(Some(("machine".to_string(), 5_000_000))));
    assert_eq!(b.buy("alice", id), Ok(None));
}

#[test]
fn c04_grade_by_the_grader_four_hours_after_the_reveal() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(b.grade("grader", id, 4_860_500, 4_950_000, 1_000 + 5 * H), Err(CallError::NotRevealed));
    b.reveal("machine", id, PAYLOAD, 1_100).unwrap();
    assert_eq!(b.grade("machine", id, 4_860_500, 4_950_000, 1_100 + 4 * H), Err(CallError::NotGrader));
    assert_eq!(b.grade("grader", id, 4_860_500, 4_950_000, 1_100 + 4 * H - 1), Err(CallError::TooEarly));
    assert_eq!(b.grade("grader", id, 4_860_500, 4_950_000, 1_100 + 4 * H), Ok(Outcome::Right));
    assert_eq!(b.grade("grader", id, 4_860_500, 4_700_000, 1_100 + 5 * H), Err(CallError::AlreadyGraded));
}

#[test]
fn c04b_outcome_math() {
    assert_eq!(outcome(Side::Long, 4_860_500, 4_950_000, 10), Outcome::Right);
    assert_eq!(outcome(Side::Long, 4_860_500, 4_700_000, 10), Outcome::Wrong);
    assert_eq!(outcome(Side::Long, 4_860_500, 4_865_000, 10), Outcome::Flat);    // +0.09% inside 0.10%
    assert_eq!(outcome(Side::Short, 4_860_500, 4_700_000, 10), Outcome::Right);
    assert_eq!(outcome(Side::Short, 4_860_500, 4_950_000, 10), Outcome::Wrong);
}

fn graded(b: &mut Book, agent: &str, side: Side, entry: u64, px4h: u64, t: u64) {
    let p = format!("{agent}-{t}");
    let id = b.commit(agent, &hash_hex(&p), "NEAR", side, 5_000_000, t).unwrap();
    b.reveal(agent, id, &p, t).unwrap();
    b.grade("grader", id, entry, px4h, t + GRADE_AFTER_S).unwrap();
}

#[test]
fn c05_c11_record_counts_the_last_window() {
    let mut b = book();
    graded(&mut b, "machine", Side::Long, 4_000_000, 4_100_000, 10);      // right
    graded(&mut b, "machine", Side::Long, 4_000_000, 3_900_000, 20);      // wrong
    graded(&mut b, "machine", Side::Long, 4_000_000, 4_001_000, 30);      // flat
    graded(&mut b, "machine", Side::Short, 4_000_000, 3_900_000, 40);     // right
    graded(&mut b, "other", Side::Long, 4_000_000, 4_100_000, 50);        // someone else's
    assert_eq!(b.record("machine", 30), (4, 2));
    assert_eq!(b.record("machine", 2), (2, 1));                           // the last two: flat, right
    assert_eq!(b.record("nobody", 30), (0, 0));
}

#[test]
fn c06_payload_only_to_buyers_and_the_agent() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    b.reveal("machine", id, PAYLOAD, 1_100).unwrap();
    assert_eq!(b.payload_for("bob", id), None);
    b.buy("alice", id).unwrap();
    assert_eq!(b.payload_for("alice", id).as_deref(), Some(PAYLOAD));
    assert_eq!(b.payload_for("machine", id).as_deref(), Some(PAYLOAD));
}

#[test]
fn c07_no_recommit_of_an_open_call() {
    let mut b = book();
    b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_001), Err(CallError::Duplicate));
}

#[test]
fn c08_the_venue_attaches_a_fill() {
    let mut b = book();
    let id = b.commit("machine", &hash_hex(PAYLOAD), "NEAR", Side::Long, 5_000_000, 1_000).unwrap();
    assert_eq!(b.traded("eve", id, "fill-1"), Err(CallError::NotVenue));
    assert_eq!(b.traded("venue", id, "fill-1"), Ok(()));
    assert_eq!(b.calls[0].fill_id.as_deref(), Some("fill-1"));
}
