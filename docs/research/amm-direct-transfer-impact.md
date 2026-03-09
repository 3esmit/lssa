# Impact of unrecoverable tokens sent directly to AMM pools

## Short summary
If you send tokens directly to an AMM pool address, the result depends on which pool-controlled account receives them. Sends to the pool definition account or the LP token definition account fail. Sends to a matching vault holding account can succeed, but the AMM does not currently expose a way to recover the extra tokens through its normal interface. In practice, those extra tokens can remain
stranded.

## Background
The AMM uses a few different accounts to manage a pool.

- A `vault` is a token holding account controlled by the pool. The pool uses   one vault for token A and one vault for token B.
- A `reserve` is the amount of each token that the AMM records in the pool's   internal state and uses for pricing, swaps, and liquidity accounting.
- An `LP token` is the token that represents a user's share of the pool.

The important detail is that the AMM tracks reserves separately from the raw token balances stored in the vaults. That distinction is what creates the unrecoverable-token problem.

## What happens on a direct transfer
Under the current token and AMM behavior, there are three main cases.

- Send to the pool definition account: the transfer fails. That account stores AMM pool data, not a token holding layout.
- Send to the LP definition account: the transfer fails. That account stores token definition data, not a token holding layout.
- Send to a vault holding account with the matching token type: the transfer can succeed.

The dangerous case is the last one. The transfer may succeed at the token layer, but that does not mean the AMM will treat the new tokens as usable pool liquidity.

## Why tokens become unrecoverable
When tokens are sent directly to a vault, they can create a gap between:

- the actual token balance inside the vault, and
- the reserve value that the AMM uses internally.

You can think of this gap as `surplus`: tokens that exist in the vault, but are not part of the AMM's tracked reserves.

Current `add-liquidity`, `swap`, and `remove-liquidity` flows operate on the AMM's reserve values. They do not include a public recovery path for arbitrary surplus that was added outside the AMM interface.

That means a direct transfer to a vault can leave extra tokens sitting in the vault without any normal AMM action that returns them to a user. The problem is especially confusing after all LP tokens have been removed, because the vault can still contain tokens even though no remaining LP position represents a claim over them.

## Impact
### User impact
- A user can lose tokens permanently by sending them to a vault address directly.
- The transfer can look successful, which makes the later loss harder to understand.

### Product and operational impact
- Pools can accumulate stranded balances that no normal AMM action can recover.
- Support burden increases because users may report that their transfer succeeded but the AMM never reflected it as usable liquidity.
- Trust in the AMM experience degrades when successful transfers can still lead to unrecoverable funds.

### Integration impact
- A vault's raw token balance can be higher than the AMM's recorded reserve.
- Tools or integrations that look only at vault balances can misread how much
  liquidity the AMM can actually use.

### Abuse and griefing impact
- Anyone who knows a vault address can send dust or nuisance balances into it.
- Even small unwanted transfers can leave cluttered, stranded balances behind.
