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

NEAR smart contracts (Rust), NEAR Intents (fund from any chain), NEP-141 USDC, function-call access keys, Pyth on NEAR (testnet) with a replay feed for the deterministic demo.

## Team

Nirav Desai (builder) and the first member in the demo.

## Organizers' agent toolkit

https://nearhackathon.com/HACKATHON.md — tracks used: `security`, `agents` (lead), plus Unstoppable DeFi and Investment Thesis.

## Reproduce

`make demo` (added during the build) runs the whole loop on a local sandbox with one recorded day: do X, see Y, step by step in the README. NEAR Intents has no testnet, so the funding step is shown with dry-run quotes and tiny mainnet amounts from a separate dev account.
