# Tape

**Verifiable intelligence for the agent economy.**
Where agents trade, and prove it — and where you can give an agent your money, not your keys.

Soon AI agents will trade, hire and pay each other. Nothing tells you which ones to trust with money, and today you either hand an agent your keys or approve every move yourself. Tape fixes both on NEAR:

- **`calls`** — an agent's call is locked on chain (a hash) before the market moves, revealed to the people who pay for it, and graded on chain four hours later. The record is public; nobody can edit it — not the agent, not us.
- **`guard`** — the agent wallet. Your USDC stays yours. The agent gets a policy, not your keys: markets, size, leverage, a stop on every order, a freeze when your balance falls too far, a gate on its own graded record, every trade tied to a published call, one-tap revoke, and money out only to you. Refusals are on chain, with the reason.
- **`venue`** — oracle-priced perps on NEAR where the agent's trades land, with venue-held stops and targets and the same margin rules as a regulated exchange.

The first agent is **The Machine**. Its first markets are NEAR, ZEC and BTC. Funding comes from any chain through NEAR Intents.

## See it

- **Live on NEAR testnet:** contract [`app.tape-nvibe.testnet`](https://testnet.nearblocks.io/address/app.tape-nvibe.testnet) ·
  the page: `docs/index.html` (GitHub Pages) reads it live — Alice's balance and freeze line, The Machine's record, every
  on-chain event, and each call's levels, sealed until its grade.
- **One full run on testnet, every step with its explorer link:** [`demo/testnet-run-2026-11-02.txt`](demo/testnet-run-2026-11-02.txt).

## Run it

```
rustup target add wasm32-unknown-unknown
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/near/cargo-near/releases/latest/download/cargo-near-installer.sh | sh
make test
```

`make demo` replays the recorded night on a local NEAR sandbox and narrates it, step by step (`make demo-testnet` plays the
same story on the testnet deployment). `make unit` checks every rule in plain Rust (venue, guard, calls). `make e2e` builds the contracts and runs the whole story on a
local NEAR sandbox — see `contracts/tape/tests/e2e.rs`:

- **The demo beat** — Alice deposits $5,000 and pays The Machine $5 for a call. She gives it a policy, not her keys: NEAR only,
  2.0x, a stop on every order, freeze at −15%, a record of at least 55% over its last 3 graded calls, every trade tied to a
  published call. It opens on its call; the stop is hit; a second call, a second stop. Her balance is $4,075.43 — under 85% of
  her $5,000 peak — so the third order is **refused on chain: FROZEN**, and nothing moves. She revokes and withdraws to herself;
  The Machine keeps only the $5 she paid it.
- **Scope, money, refusals** — the agent cannot close her own positions or anyone else's, cannot withdraw her money, and every
  refusal is an on-chain event with its code.
- **The record** — commit, reveal, buy, grade; the levels go on chain only at the grade and must hash to the commit made
  before the move; a forged grade is rejected.
- **The venue** — a position liquidated by a keeper below maintenance; every dollar the contract holds is someone's balance,
  someone's collateral, or the pool.

In demo mode the keeper replays a recorded day: each price carries its recorded time, and that is the contract's clock — so
"graded four hours after the reveal" is measured on the replayed day. Live, the clock is block time and the price is Pyth's.

## Layout

```
contracts/calls   commit → reveal → buy → grade; the record guard reads
contracts/guard   the agent wallet: policy engine + owner-only funds
contracts/venue   oracle-priced perps: margin, stops/targets, liquidation
contracts/tape    the NEAR contract: the three above in one, so the guard's check and the fill are one transaction
contracts/usdc-mock  a test USDC (NEP-141) for the sandbox
docs/index.html   the app page: live view of the contract on testnet, plus a live NEAR Intents quote for adding funds
demo/             `make demo`: the recorded night (day.json) played and narrated, on a sandbox or on testnet
SPEC.md           every rule and every test, written before the code
```

## The demo night

Real NEAR prices from Oct 29–30, 2026 — the night BTC lost $80K (NEAR 4.65 → 3.94 → 4.37), 2-minute marks. The calls are
**scripted demo calls, not The Machine's live output**: the demo shows the protocol, not the agent's edge.

| | When | What happens on chain |
|---|---|---|
| Record | 05:01–15:01 | five calls locked before the move and graded four hours later: 4 of 5 right |
| Policy | 16:01 | Alice deposits $5,000 and grants a policy — NEAR · ≤ 2.5x · stop required · freeze −10% · record ≥ 55% of 5 · published calls only |
| Call A | 17:01 → 18:47 | long 4.59, The Machine trades it with $3,000 of hers; stop hit at 4.397 → $4,700.91 |
| Call B | 21:31 → 00:27 | long 4.23 with $2,800; stop hit at 3.972 → **$4,309.53**, under 90% of her $5,000 peak |
| Call C | 01:31 | **refused on chain: FROZEN** — nothing moves |
| Grades | +4h each | A wrong, B wrong, C *right* (+7.4%): the freeze kept her out of a third loss *and* out of the bounce — the record shows both |
| Morning | | she revokes and withdraws $4,309.53 to herself; The Machine keeps the $15 she paid it for calls |

## Status

Every rule was written as a test before its code (see `SPEC.md` and the git history). See `HACKATHON.md` for what existed
before and what is new.

## License

MIT
