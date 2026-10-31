# Tape

**Verifiable intelligence for the agent economy.**
Where agents trade, and prove it — and where you can give an agent your money, not your keys.

Soon AI agents will trade, hire and pay each other. Nothing tells you which ones to trust with money, and today you either hand an agent your keys or approve every move yourself. Tape fixes both on NEAR:

- **`calls`** — an agent's call is locked on chain (a hash) before the market moves, revealed to the people who pay for it, and graded on chain four hours later. The record is public; nobody can edit it — not the agent, not us.
- **`guard`** — the agent wallet. Your USDC stays yours. The agent gets a policy, not your keys: markets, size, leverage, a stop on every order, a freeze when your balance falls too far, a gate on its own graded record, every trade tied to a published call, one-tap revoke, and money out only to you. Refusals are on chain, with the reason.
- **`venue`** — oracle-priced perps on NEAR where the agent's trades land, with venue-held stops and targets and the same margin rules as a regulated exchange.

The first agent is **The Machine**. Its first markets are NEAR, ZEC and BTC. Funding comes from any chain through NEAR Intents.

## Run it

```
rustup target add wasm32-unknown-unknown
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/near/cargo-near/releases/latest/download/cargo-near-installer.sh | sh
make test
```

`make unit` checks every rule in plain Rust (venue, guard, calls). `make e2e` builds the contracts and runs the whole story on a
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
app/              thin client: add funds (Intents), buy a call, set a policy, watch it trade
demo/             `make demo`: one recorded day, deterministic end to end
SPEC.md           every rule and every test, written before the code
```

## Status

Every rule was written as a test before its code (see `SPEC.md` and the git history). Next: the app (fund with NEAR
Intents, buy a call, set a policy, watch it trade) and `make demo` on one recorded NEAR day. See `HACKATHON.md` for what
existed before and what is new.

## License

MIT
