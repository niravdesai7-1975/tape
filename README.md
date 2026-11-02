# Tape

**Hire an AI trader on chain. Your money stays where you trade.**

Soon AI agents will trade, hire and pay each other. Nothing tells you which ones to trust with money, and today you either
hand an agent your keys or approve every move yourself. In the US you cannot even pay one per trade: builder codes on
Hyperliquid are closed to US persons. Tape is the rail, on NEAR:

- **The record (`calls`)** — an agent's call is locked on chain (a hash) before the market moves, sold to the members who
  pay for it, and graded on chain four hours later. Public; nobody can edit it — not the agent, not us.
- **Pay per call** — $5 in USDC, on chain, from a small fee float that arrives through NEAR Intents. Any agent can list;
  any member — or any other agent — can pay. That is the agent economy, and it scales because every payment, ask and
  receipt is one cheap NEAR transaction.
- **Your rules on chain (`guard`)** — your money never leaves your own exchange account (Coinbase, under a trade-only key).
  The agent gets a policy, not your keys: Coinbase's margin, a stop on every order, a freeze when your balance falls too far,
  a gate on its own graded record, every trade tied to a published call, one-tap revoke. Before any order goes to Coinbase
  the agent **asks** the chain; the same policy engine answers; a refusal is an event with its code. After Coinbase fills,
  the **receipt** lands on chain. The record of every ask, fill and refusal is public.

The first agent is **The Machine**. Its first markets are NEAR, ZEC and BTC.

## See it

Three taps. **Connect Coinbase** — your money stays there; $20 of fees come to NEAR through Intents. **Hire The Machine** —
its record, what it sees, your rules, $5 a call. **Watch** — it asks the chain, Coinbase fills, the receipt lands; every
line on screen is a NEAR transaction. Then **Revoke**: one tap, and your money never moved.

**What lives where.** On NEAR: the fee float (Intents), the pay per call, the member's rules, every ask, every fill's
receipt, every refusal, and the graded record. On Coinbase: the member's money and the fills, under a trade-only API key.
We do not offer US perps on NEAR. (`contracts/venue`, an oracle-priced perps core, is kept for markets where the fill
itself can be on chain; the demo does not use it.)

- **The page:** `docs/index.html` (GitHub Pages) — a phone-sized flow that reads the contract live on NEAR testnet
  ([`app.tape-nvibe.testnet`](https://testnet.nearblocks.io/address/app.tape-nvibe.testnet)); `docs/record.html` is the public record.
- **Two recorded runs on testnet, every step with its explorer link:** the winning day
  ([`demo/testnet-run-day-2026-11-02.txt`](demo/testnet-run-day-2026-11-02.txt)) and the night the rules did their job
  ([`demo/testnet-run-night-2026-11-02.txt`](demo/testnet-run-night-2026-11-02.txt)).

## Run it

```
rustup target add wasm32-unknown-unknown
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/near/cargo-near/releases/latest/download/cargo-near-installer.sh | sh
make test
```

`make demo` replays the recorded day on a local NEAR sandbox and narrates it, step by step (`make demo-testnet` plays the
same story on the testnet deployment). The runner stands in for Coinbase — filling at the recorded price under the same
margin model — and for The Machine's server, which asks the chain before every order and posts every receipt. `make unit`
checks every rule in plain Rust (calls, guard, venue). `make e2e` builds the contracts and runs the whole story on a local
NEAR sandbox — see `contracts/tape/tests/e2e.rs`:

- **Her money stays on Coinbase (G_15)** — Alice's $20 fee float arrives; she pays $5 for a call and grants The Machine her
  rules. It asks the chain: 2 contracts at 3.99 on $2,000 margin, stop 3.80, her Coinbase equity $5,000 — allowed, a ticket.
  Only it can post the receipt, and the same order ref only once. The close receipt carries +$454.54 and her equity after.
  A second ask on the same call is refused (once per owner). She revokes: the next ask is REVOKED. The only money of hers on
  chain is what she paid; The Machine withdraws exactly that and not a cent more.
- **The night (G_16)** — two losing closes put her at $4,075.43, under 85% of her $5,000 peak; the third ask is **refused on
  chain: FROZEN** before any order exists. Only she unfreezes.
- **The record (C_10)** — commit, reveal, buy, grade; the levels go on chain only at the grade and must hash to the commit
  made before the move; a forged grade is rejected.
- **The on-chain venue (G_11–14, V_14)** — the earlier design, kept and still tested: positions on NEAR, a keeper
  liquidation, every dollar reconciled.

In demo mode the keeper replays a recorded day: each price carries its recorded time, and that is the contract's clock — so
"graded four hours after the reveal" is measured on the replayed day. Live, the clock is block time and the price is Pyth's.

## Layout

```
contracts/calls   commit → reveal → buy → grade → fill refs; the record guard reads
contracts/guard   the policy engine (ask) + the owner-only fee float
contracts/venue   oracle-priced perps core (margin model; the on-chain-fill option, not used by the demo)
contracts/tape    the NEAR contract: the record, the rules, asks, receipts, the fee float — one contract
contracts/usdc-mock  a test USDC (NEP-141) for the sandbox
docs/index.html   the app page: the three-tap flow, read live from the contract on testnet
demo/             `make demo`: day.json / night.json played and narrated, on a sandbox or on testnet
SPEC.md           every rule and every test, written before the code
```

## The two days

Real NEAR prices, 2-minute marks, Oct 29–31 2026 — the night BTC lost $80K and the day after. Alice's rules in both:
Coinbase's margin (42%, at most 2.4x) · a stop on every trade · stops itself at −10% from her peak · only while The Machine's
record is ≥ 55% of its last 5 · every trade on a published call.

**`demo/day.json` — Oct 30, the day after.** The two calls are The Machine's registered BTC 24h open-interest reads that day
(the stop and target levels are the demo's). Alice connects Coinbase with $5,000 at 00:31. Call 1 (01:01, long 3.99): the
chain clears the order, Coinbase fills, target 4.30 hit at 06:31 → $5,454.54. Call 2 (17:01, long 4.29): target 4.50 hit
the next afternoon → **$5,757.13**. Both graded Right four hours after the reveal. She revokes; her money never moved.

**`demo/night.json` — Oct 29, the night.** Scripted calls, the night the rules did their job: two stops take her from $5,000
to $4,309.53, under her −10% line; the third ask is **refused on chain: FROZEN** — no order goes to Coinbase. That call is
graded Right four hours later — the freeze kept her out of a third loss and out of the bounce, and the record shows both.

`make demo` plays `day.json` on a local sandbox; `make demo-testnet` plays it on testnet; add `night.json` for the night.

## Status

Every rule was written as a test before its code (see `SPEC.md` and the git history). See `HACKATHON.md` for what existed
before and what is new.

## License

MIT
