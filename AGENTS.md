# apps/solana

The `recur` Anchor program (`@repo/solana`). The root `AGENTS.md` still applies; this file adds the on-chain rules. Product design: `docs/superpowers/specs/2026-10-24-onchain-subscriptions-design.md`. Decisions and their reasons: `docs/adr/0011-solana-program-and-rail.md`.

## Concepts you need here

New to Solana? These are the ideas every file below relies on. The vendored `solana-dev` skill (`.claude/skills/solana-dev/references/concepts.md`) and the `solana-mcp-server` MCP go deeper.

- **Accounts:** all state lives in accounts (address, owner program, lamports, data). A program can only change accounts it owns. [Docs](https://solana.com/docs/core/accounts)
- **PDAs:** program-derived addresses, made from seeds plus a program id. Nobody holds their key, but the program can sign for them with `invoke_signed`. Recur's puller and transit accounts are PDAs. [Docs](https://solana.com/docs/core/pda)
- **Rent:** accounts must hold a minimum lamport balance for their size. Whoever creates an account pays it and gets it back when the account closes. [Docs](https://solana.com/docs/core/accounts#rent)
- **Token accounts and delegates:** USDC sits in SPL token accounts (usually the owner's associated token account, ATA). A token account can name one delegate allowed to move its tokens. [Docs](https://solana.com/docs/tokens)
- **The rail:** Solana Foundation's Subscriptions program (`De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44`). Each member has one Subscription Authority PDA per mint, which is the single delegate on their token account. A merchant's **plan** fixes the amount, the period and up to 4 destinations and 4 pullers; a **subscription** is the member's acceptance of a plan. [Docs](https://solana.com/docs/payments/subscriptions/overview)
- **CPI:** a program calling another program. `collect` calls the rail's `transfer_subscription`, then the token program. [Docs](https://solana.com/docs/core/cpi)
- **Fee payer:** any signer can pay a transaction's fees, which is how Recur sponsors gas. [Docs](https://solana.com/docs/core/fees)

## Setup

Run once per machine (the user runs the installers; agents don't):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
sh -c "$(curl -sSfL https://release.anza.xyz/v4.3.2/install)"
cargo install --git https://github.com/solana-foundation/anchor avm --force && avm install 1.2.1 && avm use 1.2.1
```

zsh users: put `. "$HOME/.cargo/env"` and the Solana `active_release/bin` PATH line in `~/.zshrc`, not just `~/.profile`. Every script here starts with `scripts/require-solana-toolchain.mjs`, which names any missing tool. Prefix CLI calls with `NO_DNA=1` when an agent runs them (non-interactive output).

## Stack

| Concern | Use | Not |
| --- | --- | --- |
| Program | Anchor 1.2.1 (`anchor-lang`, `anchor-spl`) | Pinocchio, raw `solana-program` |
| Rail | `subscriptions` 0.5.0 crate (builders, `Plan` deserializer) | hand-built rail instructions |
| Tests | Rust integration tests on LiteSVM 0.18 + `litesvm-token`, loading the pinned rail binary | `solana-test-validator`, mocha |
| Client | `@repo/solana-client`, Codama-generated from the IDL | hand-written instruction encoders |
| Toolchain | Rust 1.97 for host tests (`rust-toolchain.toml`); `cargo build-sbf` brings its own rustc; Agave 4.3.2 | |

## Layout

```
Anchor.toml               program id (localnet + devnet), toolchain pins
Cargo.toml                workspace
rust-toolchain.toml       host Rust 1.97
programs/recur/
  src/lib.rs              declare_id! and thin #[program] handlers
  src/state.rs            accounts and PDA seeds
  src/errors.rs           RecurError
  src/fee.rs              fee_for, bps_of
  src/instructions/       one file per instruction: Accounts struct + handle_<name>
  tests/common/mod.rs     LiteSVM setup, rail plan + subscription fixtures
  tests/<name>.rs         one file per instruction
  tests/fixtures/         subscriptions.so (rail binary from devnet) + its sha256
target/                   build output, gitignored (keypair lives in target/deploy, never committed)
```

PDA seeds:

| Account | Seeds |
| --- | --- |
| `Config` | `["config"]` |
| puller | `["puller"]` |
| transit | `["transit"]` |
| rail Subscription Authority | `["SubscriptionAuthority", member, mint]` (rail program) |
| rail plan | `["plan", merchant, plan_id_le]` (rail program) |
| rail subscription | `["subscription", plan, member]` (rail program) |

## Rules

A failing tool names the rule number. Fix the code, never the rule.

| # | Rule | Enforced by |
| --- | --- | --- |
| 1 | Every account is checked: signer, owner (`owner = …`), PDA (`seeds`), address, or token mint/authority. An `UncheckedAccount` needs a constraint plus its `/// CHECK:` line | Anchor build + reviewer |
| 2 | New PDAs are added to the seed table above | reviewer |
| 3 | Checked math only (`checked_*`, `u128` for bps), never bare `+ - *` on amounts | clippy + reviewer |
| 4 | No `unwrap`/`expect` in program code (tests may) | reviewer |
| 5 | Every instruction has a success test and a test per failure path | reviewer |
| 6 | Every account has `version` and `reserved` padding; new fields take bytes from `reserved` and bump `version` | reviewer |
| 7 | Access never follows the rail's "active": it follows `paid_until` advanced by a successful `collect` | reviewer |
| 8 | The transit balance ends every instruction at 0 | tests (`collect.rs`) |
| 9 | Never hand-edit `target/`, the rail fixture or `@repo/solana-client/src/generated` | guard hook |
| 10 | No comments except Anchor's `/// CHECK:` | `pnpm lint:comments` |
| 11 | Permissionless instructions constrain every payout account (plan owner, `config.treasury`); nothing a caller passes decides where money goes | reviewer |

## Recipes

- **Add an instruction:** `/new-instruction <name>`.
- **Add an account field:** take bytes from `reserved`, bump the account's `VERSION` constant, handle the old version where the account is read, add a test that reads a version-1 account.
- **Build / test:** `pnpm turbo run typecheck test --filter=@repo/solana` (builds first). After changing the program, `pnpm turbo run gen:solana` regenerates the client.
- **Deploy to devnet:** a devnet wallet kept outside the repo (`solana-keygen new -o ~/.config/solana/devnet.json`, funded from faucet.solana.com), then `NO_DNA=1 anchor deploy --provider.cluster devnet --provider.wallet ~/.config/solana/devnet.json`. The program keypair in `target/deploy/recur-keypair.json` is the upgrade key: keep a copy in a password manager, never in git. Right after `anchor deploy`, the same wallet runs `init_config` and checks `config.admin`: only the upgrade authority can initialise. Before mainnet, the upgrade authority (and with it the right to initialise) moves to Squads.
- **Re-pin the rail binary:** `solana program dump -u devnet De1egAFMkMWZSN5rYXRj9CAdheBamobVNubTsi9avR44 programs/recur/tests/fixtures/subscriptions.so`, then `shasum -a 256 subscriptions.so > subscriptions.so.sha256` in that folder, run the tests, and record why in the commit body.
