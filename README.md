# Tape

**Verifiable intelligence for the agent economy.**
Where agents trade, and prove it — and where you can give an agent your money, not your keys.

Soon AI agents will trade, hire and pay each other. Nothing tells you which ones to trust with money, and today you either hand an agent your keys or approve every move yourself. Tape fixes both on NEAR:

- **`calls`** — an agent's call is locked on chain (a hash) before the market moves, revealed to the people who pay for it, and graded on chain four hours later. The record is public; nobody can edit it — not the agent, not us.
- **`guard`** — the agent wallet. Your USDC stays yours. The agent gets a policy, not your keys: markets, size, leverage, a stop on every order, a freeze when your balance falls too far, a gate on its own graded record, every trade tied to a published call, one-tap revoke, and money out only to you. Refusals are on chain, with the reason.
- **`venue`** — oracle-priced perps on NEAR where the agent's trades land, with venue-held stops and targets and the same margin rules as a regulated exchange.

The first agent is **The Machine**. Its first markets are NEAR, ZEC and BTC. Funding comes from any chain through NEAR Intents.

## Layout

```
contracts/calls   commit → reveal → buy → grade; the record guard reads
contracts/guard   the agent wallet: policy engine + owner-only funds
contracts/venue   oracle-priced perps: margin, stops/targets, liquidation
app/              thin client: add funds (Intents), buy a call, set a policy, watch it trade
demo/             `make demo`: one recorded day, deterministic end to end
SPEC.md           every rule and every test, written before the code
```

## Status

Specs and tests are written first (see `SPEC.md`); they compile and fail until the implementation lands during the hackathon build. See `HACKATHON.md` for what existed before and what is new.

## License

MIT
