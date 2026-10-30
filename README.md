# recur-program

The on-chain program behind [Recur](https://www.recurme.com): monthly memberships paid in USDC on Solana. Creators and communities sell a membership, members pay from their own wallet, and renewals are collected on schedule.

It is built on the Solana Foundation [Subscriptions program](https://github.com/solana-foundation/subscriptions) (`De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44`). That program holds the member's token delegation and enforces the plan's price and period. This program adds merchants, the fee split, comps, upfront payments and refunds.

| | |
| --- | --- |
| Program id (devnet) | `bY4Yyya6CMHBUfU9BfgoWK8TCjVhWtxQPurtX56dF5v` |
| Subscriptions program (the rail) | `De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44` |
| Framework | Anchor 1.2.1, Solana 4.3.2 (`Anchor.toml`), host Rust 1.97 (`rust-toolchain.toml`) |
| Status | Devnet. Not audited yet; an audit comes before mainnet |

## Instructions

| Instruction | What it does |
| --- | --- |
| `init_config`, `update_config` | Protocol config: fee (bps and minimum), treasury, admin, pause |
| `register_merchant`, `update_merchant` | A merchant and its referral share |
| `set_merchant_terms` | Per-merchant fee override and fee-free period |
| `subscribe_hook` | Records the membership, in the same transaction as the rail's subscribe (optional referrer) |
| `collect` | Collects a due period through the rail and splits the fee |
| `pay_upfront` | Pays for a membership upfront |
| `grant_comp`, `revoke_comp` | Free memberships granted by the merchant |
| `refund` | Merchant refund, optionally shortening paid time |
| `sweep_transit`, `close` | Housekeeping for the transit account and ended memberships |

## How Recur uses this program

- **One delegation, on the rail.** At a member's first checkout the rail's `init_subscription_authority` makes the member's Subscription Authority PDA the delegate of their USDC token account. It is created once per member and mint. A merchant's rail plan fixes the amount and the period, so a pull can never take more than one period's price, and the member can cancel on the rail without Recur.
- **Subscribe.** Checkout sends one transaction: `init_subscription_authority` the first time, the rail's `subscribe` next to this program's `subscribe_hook` (it opens the `Membership` account and records an optional referrer at the merchant's `referral_bps`), then a first `collect`. The member pays the first period at once.
- **Renewals.** `collect` takes no amount and no signer of its own: it reads the price from the rail plan, pulls one period through the rail's `transfer_subscription` (signed by this program's puller PDA) into the transit PDA, splits it and moves `paid_until` forward. Recur's keeper calls it when a period is due. Anyone can call it, because every payout account is constrained to the plan owner, `config.treasury` and the recorded referrer.
- **The fee split** (`src/fee.rs`). The protocol fee is `fee_bps` of the amount with a `min_fee` floor, capped at the amount; a merchant can have a fee override and a fee-free period (`set_merchant_terms`). The referral is `referral_bps` of what's left after the fee, and the merchant gets the rest. All three transfers go straight to the recipients' token accounts in the same instruction; the transit account ends every instruction empty.
- **Access follows `paid_until`,** set by `collect`, `pay_upfront` or `grant_comp`, never the rail's "active" flag. Grace is computed off-chain from `paid_until`.
- **Comps.** The plan owner grants free time with `grant_comp` (until a date) and ends it with `revoke_comp`. `collect` refuses while comp or upfront time is still running (`PrepaidAccessActive`), so a member isn't charged for time they already have.
- **Refunds.** The merchant signs `refund` to send USDC from their own token account back to the member, optionally shortening `paid_until`. It can never extend it.
- **Pause.** While `config.paused`, the money-moving instructions (`collect`, `pay_upfront`, `subscribe_hook`) refuse; refunds, comps and housekeeping keep working.

Recur's API (private) builds these transactions, pays their network fees as the fee payer and runs the keeper. The member signs in their own wallet and needs no SOL.

## Setup

1. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`. The pinned host toolchain (1.97, from `rust-toolchain.toml`) installs on first use.
2. Solana CLI 4.3.2 (Agave): `sh -c "$(curl -sSfL https://release.anza.xyz/v4.3.2/install)"`.
3. Anchor 1.2.1 through `avm`: `cargo install --git https://github.com/solana-foundation/anchor avm --force && avm install 1.2.1 && avm use 1.2.1`.

zsh users: put `. "$HOME/.cargo/env"` and the Solana `active_release/bin` PATH line in `~/.zshrc`.

## Build and test

```sh
anchor build
cargo test
```

Build first: the tests load `target/deploy/recur.so`. They run on LiteSVM against pinned binaries of the Subscriptions program and of Lighthouse (`programs/recur/tests/fixtures`, each with its sha256), one test file per instruction, with a test for every failure path.

## Deploy to devnet

1. A devnet wallet kept outside the repo: `solana-keygen new -o ~/.config/solana/devnet.json`, funded from https://faucet.solana.com.
2. `anchor build`, then `anchor deploy --provider.cluster devnet --provider.wallet ~/.config/solana/devnet.json`. `anchor build` creates the program keypair in `target/deploy/recur-keypair.json`. It is the upgrade key: keep a copy somewhere safe, never in git. A fresh keypair gives a new program id, so run `anchor keys sync` and build again before deploying your own copy.
3. From `programs/recur`: `cargo run --example devnet_collect -- ~/.config/solana/devnet.json`. It runs `init_config` if the config is absent (only the upgrade authority can), creates a test mint, a merchant and a 1 USDC / 1 hour plan, and sends the subscribe bundle. An hour later, `cargo run --example devnet_collect -- ~/.config/solana/devnet.json --resume` runs the second `collect` and prints the balances.

## About this repository

This is a read-only mirror of the program folder of Recur's private monorepo, synced from its main branch. Please open issues here; changes are made upstream. Recur's API and web app are not public.

## License

Apache-2.0, see [LICENSE](LICENSE).
