# HACKATHON.md

## Prior work — read this first

The AI that decides the trades (**The Machine**: its market reads, liquidation map, grading pipeline and paper book) was built before the hackathon and stays private. This repo talks to it through an API, and the judges' demo replays one recorded day of its calls, so nothing here depends on a private server.

**Everything in this repo is new, written after the hackathon was announced (Oct 19, 2026):** the `calls`, `guard` and `venue` contracts, the app's NEAR layer (Intents funding, per-call payment, policy setup), the record page, and `make demo`. The repo was created on Oct 30, 2026; its first commit is the specification and the tests (interfaces with `todo!()` bodies, every test failing), so the history shows tests first, then the code. Baseline (pre-announcement, not judged): The Machine's engine, which is not in this repo.

Libraries and tools: Rust, `sha2`, `hex`; near-sdk and near-workspaces (from the contract layer on); code written with Claude (Anthropic) as the coding assistant.

Question asked in the organizers' Discord about a pre-existing private engine behind a new on-chain layer: _(answer recorded here)_

## Tracks

- **AI Security & Defense** (lead) — `guard`: spending limits, a policy engine, a sandboxed wallet, reputation from an uneditable record, and trades tied to published calls.
- **Agent Economy** — agents (and people) pay an agent per call in USDC; other agents can hire The Machine the same way.
- **Unstoppable DeFi** — `venue`: oracle-priced perps on NEAR.
- **Investment Thesis** — every call graded on chain, misses included.

## NEAR technology used

NEAR smart contracts (Rust, near-sdk 5), NEP-141 USDC (`ft_transfer_call` for deposits, pool and per-call payment), NEP-297 events, near-workspaces for the sandbox and testnet runs, NEAR Intents (1Click) for funding from any chain. Live deployments would use Pyth on NEAR; the demo replays recorded prices with their recorded timestamps as the contract clock.

## Team

NVibe AI — Nirav Desai (builder), with his wife as the first member in the demo video.

## Organizers' agent toolkit

https://nearhackathon.com/HACKATHON.md — tracks used: `security`, `agents` (lead), plus Unstoppable DeFi and Investment Thesis.

## Reproduce

1. `make test` — 33 rule tests and 5 on-chain tests on a local NEAR sandbox.
2. `make demo` — the recorded night, narrated step by step on a local sandbox: do X, see Y.
3. `make demo-testnet` — the same story on the testnet deployment (`app.tape-nvibe.testnet`); a recorded run with every
   explorer link is in `demo/testnet-run-2026-11-02.txt`.
4. Open `docs/index.html` (or the GitHub Pages site) — the contract's live state on testnet.

The prices are real (NEAR, Oct 29–30, 2026); **the calls in the demo are scripted**, not The Machine's live output. NEAR
Intents has no testnet, so the page shows a live mainnet dry-run quote for adding funds (nothing moves) and the demo funds
Alice with test USDC.
