# HACKATHON.md

## Prior work — read this first

The AI that decides the trades (**The Machine**: its market reads, liquidation map, grading pipeline and paper book) was built before the hackathon and stays private. This repo talks to it through an API, and the judges' demo replays one recorded day of its calls, so nothing here depends on a private server.

**Everything in this repo is new, written after the hackathon was announced (Oct 19, 2026):** the `calls`, `guard` and `venue` contracts, the `tape` contract (record, pay per call, rules, asks, receipts), the app's NEAR layer (Intents fee float, per-call payment, policy setup), the page, and `make demo`. The repo was created on Oct 30, 2026; its first commit is the specification and the tests (interfaces with `todo!()` bodies, every test failing), so the history shows tests first, then the code. Baseline (pre-announcement, not judged): The Machine's engine, which is not in this repo.

Libraries and tools: Rust, `sha2`, `hex`; near-sdk and near-workspaces (from the contract layer on); code written with Claude (Anthropic) as the coding assistant.

Question asked in the organizers' Discord about a pre-existing private engine behind a new on-chain layer: _(answer recorded here)_

## In NEAR's words

| NEAR's term | What it means | Where Tape shows it |
|---|---|---|
| Autonomous agents | An agent acting on your behalf | The Machine trades her Coinbase account under her limits; +$757 while she worked, nothing lost while she slept |
| AI money | Agents hold funds, pay each other, get paid | The Machine is paid $5 per call on chain; the next member can be another agent |
| Agent economy (track 07) | Agents that earn, spend, hire | Any agent lists; any member or agent hires it with one transaction; its graded record is its reputation |
| Trust, but verify (track 06) | Reputation and limits enforced by code | Calls locked before the move, graded after; the record gates the grant; every order is asked on chain first; the −10% limit declined the third trade while she slept |
| Give AI money without giving it the keys | Sandboxed wallets, spending limits, policy engines | Her money stays in her own Coinbase account under a trade-only key; the agent gets a policy on NEAR; refusals are events with a code |
| Verifiable AI | Outputs you can check | The hash committed before the move; the levels published at the grade must match it |
| NEAR Intents / chain abstraction | Act from any chain, no bridge, no gas | The fee float arrives from Coinbase through Intents; Face ID in, no seed phrase |
| Investment Thesis (track 02) | A re-runnable, data-backed case | Two recorded testnet runs, `make demo`, misses included |
| Unstoppable DeFi (track 01) | Venues native to NEAR | `contracts/venue`, an oracle-priced perps core — built and tested, not used by the demo; where a fill could land on chain outside the US |

## Demo video

`docs/the-machine-on-near.mp4` (2:28) — ten steps: what it is, the record, why NEAR, then Alice's three taps on testnet, the night the limit held, and the scorecard in NEAR's words. Voice and captions; every number on screen is from the recorded testnet runs.

## Tracks

- **Agent Economy** (lead) — any agent lists; any member or agent pays it per call in USDC on NEAR; its record is its reputation. The first agent is The Machine; the first member is a person; the next member can be another agent. One cheap transaction per payment, ask and receipt is why this scales.
- **AI Security & Defense** — `guard`: the member's rules on chain, the agent asks before every order, refusals on chain with their code, a freeze that is a condition on her money, reputation from an uneditable record, every trade tied to a published call, one-tap revoke. Her money never leaves her own account.
- **Investment Thesis** — every call graded on chain, misses included.
- **Unstoppable DeFi** — `venue`: an oracle-priced perps core, kept for markets where the fill itself can be on chain.

## NEAR technology used

NEAR smart contracts (Rust, near-sdk 5), NEP-141 USDC (`ft_transfer_call` for the fee float; `buy` pays per call from it), NEP-297 events, near-workspaces for the sandbox and testnet runs, NEAR Intents (1Click) for the fee float from any chain. Live deployments would use Pyth on NEAR; the demo replays recorded prices with their recorded timestamps as the contract clock.

## Team

NVibe AI — Nirav Desai (builder), with his wife as the first member in the demo video.

## Organizers' agent toolkit

https://nearhackathon.com/HACKATHON.md — tracks used: `agents` (lead), `security`, plus Investment Thesis and Unstoppable DeFi.

## Reproduce

1. `make test` — 34 rule tests and 7 on-chain tests on a local NEAR sandbox.
2. `make demo` — the recorded day, narrated step by step on a local sandbox: do X, see Y.
3. `make demo-testnet` (and `make demo-testnet DAY=night.json`) — the same on the testnet deployment
   (`app.tape-nvibe.testnet`); recorded runs with every explorer link are in `demo/testnet-run-*.txt`.
4. **Try it in five taps, no setup:** https://niravdesai7-1975.github.io/tape/try.html — tap “Watch it run once” for a
   one-minute guided run (the page taps the five steps itself, narrated, and pulls one transaction straight from NEAR to
   show it verified), then do it yourself. Each tap signs a real testnet
   transaction from the throwaway accounts `judge-alice` / `judge-machine.tape-nvibe.testnet` (their keys are in the page on
   purpose): the fee float arrives, the rules go on chain and $5 is paid for a call, the chain clears an order and the
   Coinbase receipts land, a −10% report gets the next order DECLINED (FROZEN), revoke — and The Machine's next try comes back REVOKED. Every line links to its transaction.
5. Open the GitHub Pages site (or `docs/index.html`): the three-tap flow. By default the page replays the most recent
   recorded run from the contract's own event log — each tap reveals the next real on-chain events, every line a real
   transaction. With `?live`, the page is a viewer and the taps are the member's own transactions landing as
   `make demo-testnet` plays (the runner signs them with her testnet key). `docs/record.html` is the public record.

What lives where: on NEAR, the fee float (Intents), the pay per call, the member's rules, every ask, every fill's receipt,
every refusal and the graded record. On Coinbase: her money and the fills, under a trade-only API key. We do not offer US
perps on NEAR.

The prices are real (NEAR, Oct 29–31, 2026). The Oct 30 calls are The Machine's registered reads that day; the Oct 29 calls
are scripted. The Coinbase fills in the demo are the recorded day replayed by the runner under the same margin model
(`tape-venue`); live, they are the member's own Coinbase account. NEAR Intents has no testnet, so the page shows a live
mainnet dry-run quote for the fee float (nothing moves) and the demo funds Alice's float with test USDC. Face ID is a button.
