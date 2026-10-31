# SPEC — every rule, and the test that pins it

Written before the code. Units everywhere: money in micro-USD (USDC, 6 decimals); prices in micro-USD per coin; contract
sizes in micro-coins. No rule depends on a clock except the grade (4 hours after the reveal is what "graded" means) and
oracle freshness.

## `calls` — the record

| Rule | Test |
|---|---|
| The agent commits sha256(payload) with market, side and price; the commit time is stored | C_00, C_01 |
| Reveal (the call goes out to paying members) only by the call's agent, once; nothing readable goes on chain | C_02 |
| An allowlisted member (anyone, when the list is empty) buys the call; the price goes to the agent; a second buy is a no-op | C_03, C_03b |
| Only the grader grades, only ≥ 4h after the reveal, once, publishing the levels, which must hash to the commit; long right above entry by > 10 bp, wrong below, flat inside; short mirrors | C_04, C_04b |
| `record(agent, window_n)` = (graded calls in the last window, how many right); flat counts in n, not in right | C_05/C_11 |
| A public chain hides nothing: buyers are recorded (`has_bought`) and the app serves them the levels off chain; the levels go on chain only at the grade | C_06 |
| An open (ungraded) hash cannot be committed again | C_07 |
| The venue attaches its fill id to a call it was traded on | C_08 |
| End to end on chain: buy pays the agent, an early or forged grade is rejected, the levels publish at the grade | C_10 (`e2e.rs`) |

## `guard` — the agent wallet

The owner deposits USDC and grants an agent a policy. The agent can only ask `guard` to trade; `guard` checks, then calls
the venue with `beneficiary = owner`. Money leaves `guard` only to the owner.

Checks, in this order, first failure wins: **revoked → frozen → market → size → collateral → leverage → exposure →
stop (none / wrong side / farther than the owner's max) → call → record.**

| Rule | Test |
|---|---|
| A good order passes | G_00 |
| Only the owner withdraws, only to the owner, only from the free balance | G_01 |
| Revoke beats everything | G_02 |
| Market allowlist | G_03 |
| Max contracts; max collateral | G_04 |
| Leverage = notional ÷ collateral ≤ the cap ($4,861 on $2,041 = 2.38x refused at 2.0x; exactly 2.0x passes) | G_05 |
| Open notional + this order ≤ the exposure cap | G_06 |
| A stop is required, on the right side; a max distance applies only if the owner set one | G_07 |
| Freeze when equity < peak × (1 − freeze); the peak follows equity up; only the owner unfreezes, which resets the peak. A condition on equity, not a clock | G_08 |
| The agent's record over its last window must reach the owner's bar; when the record recovers the grant works again by itself | G_09 |
| With `require_call`, every open cites this agent's revealed, ungraded call, same market and side; once per owner | G_10 |
| The agent closes only positions it opened under the grant; it cannot withdraw the owner's money; refusals are events with codes; the demo beat end to end | G_11–G_14 (`e2e.rs`) |

Refusal codes: REVOKED, FROZEN, MARKET, SIZE, COLLATERAL, LEVERAGE, EXPOSURE, NO_STOP, STOP_SIDE, STOP_FAR, CALL, RECORD.

## `venue` — oracle-priced perps

NEAR market as Coinbase margins it: 500 NEAR per contract, 0.001 tick, 42% initial margin on the oracle notional,
maintenance tiers (≤ 4,000 NEAR 1%, ≤ 40,000 1.5%, ≤ 200,000 2%, … past 8,000,000 100%), 6 bp taker fee, slippage
max(tick, 1 bp) rounded to the tick against the taker. Counterparty: an operator-funded pool.

| Rule | Test |
|---|---|
| Fill, notional, fee, initial margin (oracle 4.8595 → buy fill 4.861; 2 contracts = $4,861; fee $2.9166; $2,041 passes, $2,000 refused) | V_01 |
| Maintenance tiers | V_02 |
| Mark: at 4.70 the long is −$161, equity $1,880, maintenance $47 | V_03 |
| Liquidation only below maintenance; whole position at oracle − slippage; bounty 0.5% of maintenance; below zero the pool pays the shortfall | V_04 |
| Stops and targets fire only when the oracle trades through the level; a gapped stop fills at the market | V_06 |
| Total open notional ≤ 200% of pool equity | V_08 |
| Builder fee ≤ 10 bp | V_10 |
| Oracle older than 60 s or confidence wider than 0.5% refused; replay prices only in demo mode | V_11 |
| Adding averages the entry | V_12 |
| A beneficiary may be named only by `guard` | V_15 |
| A market close | V_16 |
| End to end: a keeper liquidation with the bounty; every dollar held is a balance, collateral or the pool | V_14 (`e2e.rs`) |
| Demo mode: the keeper's recorded timestamps are the clock, forward only, keeper only | T_00 (`e2e.rs`) |

## Demo (`make demo`, one recorded day)

1. A call locks on chain. 2. A member funds $5,000 via Intents and buys it. 3. She gives The Machine a policy — NEAR only,
2.0x, stop required, freeze at −15%, record ≥ 55% of the last 30, every trade on a published call. 4. The Machine opens
on its call; the replay hits the stop. 5. A second loss trips the freeze; The Machine's next order is refused on chain:
FROZEN. 6. The grade lands; the public record updates; she revokes and withdraws to herself.
