# First-depositor attack vectors and impact in the AMM

## Short summary

The AMM can return an existing token pair to a first-depositor state. That happens when all LP tokens are removed, the pool becomes inactive, and the same pair is then initialized again through `NewDefinition`. The next actor can choose a fresh reserve ratio, which means they choose a fresh starting price for that pair.

That is not ordinary market movement. It is a price reset on an existing pool identity. For users and integrations, that creates a dangerous mismatch: the pair looks continuous, but its pricing history has effectively been restarted.

## Background

The AMM keeps one pool per token pair. Each pool has three ideas that matter for this discussion.

- A `pool` is the AMM state for one token pair.
- A `reserve` is the amount of each token that the AMM records internally and
  uses for pricing.
- An `LP token` represents a share of the pool.

The important detail is that the first liquidity provider is special. When a pool is created, there is no previous ratio to follow, so the depositor's token amounts become the pool's initial reserves. Those reserves set the pool's first price relationship between token A and token B.

Later liquidity providers do not get the same freedom. The AMM derives `ideal_a` and `ideal_b` from the current reserves. In other words, once a pool exists, later LPs are expected to add liquidity in the ratio the pool already has. That is why the first depositor has special power: they set the ratio that everyone else must follow.

## How the current AMM behaves

The current code allows the following sequence.

1. A pool is created for a token pair.
2. Liquidity providers remove LP until total LP supply reaches zero.
3. The pool becomes inactive.
4. The same pair can be initialized again.
5. The next depositor chooses a new reserve ratio.
6. That new reserve ratio becomes the pair's new price anchor.

Each part of that flow is visible in the current code.

- The core pool state lives in `amm_core::PoolDefinition`, which stores both `liquidity_pool_supply` and `active`.
- `amm_core::Instruction::NewDefinition` is initializing a new pool or re-initializing an inactive pool.
- `new_definition` accepts any pool whose `active` flag is false.
- `new_definition` writes the caller-provided deposit amounts directly into `reserve_a` and `reserve_b`.
- `remove_liquidity` can reduce LP supply to zero.
- A pool with zero LP supply is explicitly represented as inactive in the NSSA state tests.
- The NSSA state tests also show that re-initializing such an inactive pool is a valid path today.

The pricing side is also straightforward. Swap output is computed directly from the recorded reserves. So once the next depositor has chosen the new reserves, they have also chosen the new starting price for the pair.

## First-depositor attack vectors

### Pair re-genesis

An existing pair can effectively become a new market again. The pair address and pool identity stay the same, but after LP supply reaches zero, the next successful initialization behaves like a fresh genesis event.

That means first-depositor privilege is not a one-time property. It can be recovered later by whichever actor is first to re-initialize the empty pool.

### Arbitrary price-ratio reset

The first depositor after re-initialization does not need to respect the old market ratio. They choose `token_a_amount` and `token_b_amount`, and those amounts become the new reserves.

This matters because the AMM does not derive the initial ratio from history, an oracle, or any external reference. It derives it from the new depositor's chosen reserve amounts. So after a full drain, the next actor can replace the
old price anchor with a new one.

### Capital-light manipulation compared with a live pool

Moving the price of a live pool normally requires trading against the pool's existing liquidity. That gets more expensive as the pool gets deeper.

The empty-pool case is different. Once LP supply has reached zero, the attacker is no longer trying to move an existing market. They are seeding a new one. Their cost is the cost of choosing new reserves, not the cost of fighting established liquidity. That can be much cheaper than moving a deep live pool to the same ratio.

### Pathological swap behavior

Extreme reserve ratios can make one side of the pool nearly useless or highly misleading. Since swaps use recorded reserves directly, a very skewed re-initialization can create a market that looks valid but behaves badly.

This is especially dangerous when the skewed pool is the first thing a user or integration sees after the pair comes back online. The pool exists, swaps are possible in principle, but the economic behavior no longer resembles the prior market.

### Misleading continuity for readers and integrations

The pair does not look like a brand-new market. It is still the same token pair, and it still uses the same pool identity derived from the pair. That can mislead anyone who assumes price continuity from pool identity alone.

For a human reader, the risk is confusion. For a router or analytics system, the risk is treating a reset market as if it had merely experienced normal trading.

## Impact

### User impact

- Users can trade against a pool whose price anchor was replaced in one transaction.
- A pair can look continuous even though its reserve history was reset.
- A user can get unexpectedly bad execution if they trust the old market
  relationship.

### Protocol impact

- The protocol loses price continuity for an existing pair.
- First-depositor privilege becomes repeatable instead of one-time.
- Trust in the AMM decreases because market identity and market history no longer line up cleanly.

### Integration impact

- Routers and analytics tools can misread the pair as stable when it has in fact been re-started.
- Historical assumptions about the pair become unsafe after a full drain.
- Monitoring that keys only on pair identity can miss the reset.

### Abuse impact

- An attacker can intentionally wait for an empty pool and re-seed it with a distorted ratio.
- Thin or inactive pools are especially exposed because they are easier to fully drain and easier to re-seed.
- The attack is practical whenever the pool can return to zero LP supply.

## Quantified example

Here is a simple example that shows why an attacker-controlled reserve ratio can create pathological behavior.

Assume the pool is re-initialized with reserves `(A, B) = (1, 1000)`. That is already a very skewed price anchor. Now consider a user swapping token B for token A.

The AMM computes swap output:

`amount_out = reserve_out * amount_in / (reserve_in + amount_in)`

For `B -> A`, that becomes:

`A_out = 1 * B_in / (1000 + B_in)`

For any finite `B_in`, the numerator is always smaller than the denominator, so integer division floors the result to `0`.

That is important because `programs/amm/src/swap.rs:141` rejects swaps whose computed output is zero. So this re-initialized pool is not just badly priced.
In one direction, it is effectively unusable for normal trades.

This example shows why the problem is more serious than a bad spot price. A new first depositor can create a market that immediately behaves in a broken or misleading way.

## Why this is not normal slippage

Normal slippage happens inside an existing market. Traders push against the current reserves, and the price moves as a result of that trading.

That is not what happens here. After a full drain, the attacker does not need to trade the market to a new price. They regain the right to define the next starting reserves for the pair. The price change is therefore not a result of price discovery inside a live pool. It is a result of recreating genesis rights for an existing pair.

That distinction is why this issue is more serious than a normal AMM price move. The attacker is not just moving the market. They are resetting it.

## Design implications

This research points to two protocol requirements.

- The pool must not be able to return to a zero-supply genesis state through normal operation.
- An already-created pair must not be re-initializable as if it were a fresh market.

This is where protections such as dead shares and one-time initialization rules become relevant. The exact implementation choice belongs in a separate design document, but the attack analysis already tells us what invariants need to be preserved.

## Conclusion

The AMM currently restores first-depositor privilege after a full liquidity drain. That allows a new actor to choose a fresh reserve ratio and therefore a fresh price anchor for an existing pair. Because the pair identity remains the same while the market has effectively been restarted, the problem is severe enough to justify protocol-level protection.
