# recur-program

The on-chain program behind Recur: monthly memberships paid in USDC on Solana. Creators and communities sell a membership, members pay from their own wallet, and renewals are collected on schedule.

It is built on the Solana Foundation [Subscriptions program](https://github.com/solana-foundation/subscriptions) (`De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44`). That program holds the member's token delegation and enforces the plan's price and period. This program adds merchants, the fee split, comps, upfront payments and refunds.

| | |
| --- | --- |
| Program id (devnet) | `bY4Yyya6CMHBUfU9BfgoWK8TCjVhWtxQPurtX56dF5v` |
| Framework | Anchor 1.2.1, Solana 4.3.2 (`rust-toolchain.toml`) |
| Status | Devnet. Not audited yet; an audit comes before mainnet |

## Instructions

| Instruction | What it does |
| --- | --- |
| `init_config`, `update_config` | Protocol config: fee (bps and minimum), treasury, admin, pause |
| `register_merchant`, `update_merchant` | A merchant and its referral share |
| `set_merchant_terms` | Per-merchant fee override and fee-free period |
| `subscribe_hook` | Runs inside the rail's subscribe and records the membership (optional referrer) |
| `collect` | Collects a due period through the rail and splits the fee |
| `pay_upfront` | Pays for a membership upfront |
| `grant_comp`, `revoke_comp` | Free memberships granted by the merchant |
| `refund` | Merchant refund, optionally shortening paid time |
| `sweep_transit`, `close` | Housekeeping for the transit account and ended memberships |

## Build and test

```sh
anchor build
cargo test
```

The tests run on LiteSVM against pinned binaries of the Subscriptions program and of Lighthouse (`programs/recur/tests/fixtures`, each with its sha256).

## About this repository

This is a read-only mirror of the program folder of Recur's private monorepo, synced from its main branch. Please open issues here; changes are made upstream.

## License

Apache-2.0, see [LICENSE](LICENSE).
