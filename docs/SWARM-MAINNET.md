# SWARM production wallet profile

`ChainType::SwarmMainnet` selects the SWARM production network: a separate
chain from Zcash Mainnet, with its own network type, its own address
encodings, its own consensus branch domain and its own coin type. Nothing it
encodes decodes as Zcash, and nothing Zcash encodes decodes as it.

This document is the contract an application pins. Read it beside
`docs/swarm-testnet.md`, which describes the other SWARM profile.

## The variant carries the genesis

```rust
use zingolib::config::{ChainType, SwarmMainnetGenesis};

let chain = ChainType::SwarmMainnet(
    SwarmMainnetGenesis::from_display_hex(
        "01c34428b9e67cdd8345e0b365aaa37dd8d2d65d3869e0e5d77d567f2c39afdd",
    )?,
);
```

There is deliberately no `SWARM_MAINNET_GENESIS` constant, no `Default` and no
placeholder. Every `ChainType::SwarmMainnet` carries the hash its caller
supplied, which is the hash the wallet then holds its indexer to. A caller that
cannot name the genesis cannot build the variant.

`SwarmMainnetGenesis::from_display_hex` takes 64 **lowercase** hexadecimal
characters in display order, the spelling `getblockhash 0` prints. Uppercase is
refused, so two spellings of one hash can never both exist in a wallet's state.

The live network's genesis is
`01c34428b9e67cdd8345e0b365aaa37dd8d2d65d3869e0e5d77d567f2c39afdd`.

## The chain hint contract

Applications that select a chain by string (the desktop wallet's native addon,
the mobile bridges) use a **chain hint** rather than a bare label:

```
swarm-mainnet:<64 lowercase hex genesis>
```

The bare label `swarm-mainnet` is **refused**. `ChainType::try_from("swarm-mainnet")`
answers `Err(InvalidChainType::SwarmMainnetNeedsGenesis)` and nothing in this
SDK turns that name alone into a chain. The reason is the whole reason the
variant carries a hash: a label is a string the operator chose, and two chains
built from the same software answer the same one. A wallet that accepted the
bare label would sync a production wallet against a rehearsal chain and write
its state back over the right one.

`ChainType::try_from` keeps its other answers unchanged. `"mainnet"` is upstream
Zcash Mainnet and only that, `"testnet"` is upstream Zcash Testnet,
`"swarm-testnet"` is `ChainType::CustomTestnet`, `"regtest"` is regtest.
`ChainType::SwarmMainnet(_)` displays as `swarm-mainnet`, which is the wallet
identity, the data-directory name and the label the indexer reports.

## Addresses

Every encoding follows from one HRP root, `swm`, in
`vendor/zcash_protocol/src/constants/swarm_mainnet.rs`.

| Kind | Encoding | Prefix |
| --- | --- | --- |
| Unified address | Bech32m, HRP `swm` | `swm1…` |
| Unified FVK / IVK | Bech32m, HRP `uviewswm` / `uivkswm` | `uviewswm1…` / `uivkswm1…` |
| Sapling payment address | Bech32, HRP `zswmsapling` | `zswmsapling1…` |
| Transparent P2PKH | Base58Check, `0x1c28` | `s1…` |
| Transparent P2SH | Base58Check, `0x1c2d` | `s3…` |
| TEX (ZIP 320) | Bech32m, HRP `texswm` | `texswm1…` |

Sprout is not supported. `B58_SPROUT_ADDRESS_PREFIX` exists only to keep
`NetworkConstants` total and is deliberately not recognised by the decoder, so a
string produced with it fails to parse rather than being mistaken for a Zcash
address.

The coin type is **9767** (SLIP 44; confirmed unregistered on 2026-09-25, the
registration filed separately, so until it merges this is a private
convention). The network type is `NetworkType::SwarmMain`; the consensus branch
is `BranchId::SwarmMain`, wire value `0x53574d31`, in effect from height 1, so
the chain has one rule epoch above genesis and every network upgrade through
NU6.3 activates there. New wallets begin scanning at `SWARM_MAINNET_BIRTHDAY`,
which is that same height 1.

### The negative rules

These are the point of the profile and each is covered by a test:

- A **bare `swarm-mainnet` label is refused.** Only the chain hint with a
  genesis builds the variant.
- **Zcash addresses are refused on SWARM production**, and SWARM production
  addresses are refused on Zcash. `t1…`, `u1…`, `zs…` are not SWARM strings;
  `s1…`, `s3…`, `swm1…`, `zswmsapling1…`, `texswm1…` are not Zcash strings.
- **SWARM testnet addresses are refused on SWARM production.** `swarm1…` (this
  SDK's vendored testnet unified HRP), its legacy `utest1…` alias,
  `ztestsapling1…`, `tm…`, `t2…`. Every historical SwarmTestnet encoding,
  including the published funding-stream destinations, decodes as
  `NetworkType::Test` and is rejected when `SwarmMain` is expected.
- **A wallet file of another chain does not open here**, and a SWARM production
  wallet file does not open on another chain.

`zingo-cli parse_address` answers all of this directly. It decodes against every chain
profile and reports both the first match and a `valid_on` list of every profile
the string decodes on:

```console
$ zingo-cli parse_address s1MCkDhVejM4RqDyRR1rEJkudd26FVWipPD
{"status":"success","chain_name":"swarm-mainnet","valid_on":["swarm-mainnet"],"address_kind":"transparent"}
```

`ChainType::Testnet` and `ChainType::CustomTestnet` share `NetworkType::Test`,
so no address can tell them apart; `valid_on` lists both rather than the answer
picking one and hiding the other.

## Pools: there is no Orchard era here

Every network upgrade through NU6.3 activates at height 1, so the SWARM
production network has **no Orchard era at all**. Every shielded payment a
wallet plans on it is an Ironwood payment, from the chain's first block.
SwarmTestnet is the same: its schedule activates everything at height 1 too.

This does **not** change what a unified address carries. ZIP 318 routes
Ironwood payments to the **Orchard receiver**. ZIP 316 defines no Ironwood
receiver typecode, and `zcash_address::unified::Receiver` has no variant for
one. An orchard-only unified address is therefore the payable Ironwood address
on this chain, not an unpayable one, and `ReceiverSelection` keeps its two
shielded options, `orchard` and `sapling`.

What differs from a pre-NU6.3 chain is the pool the proposal targets:
`create_send_proposal` reads the chain's NU6.3 activation against the synced
height and selects `ShieldedPool::Ironwood` for both the payment and the
change. On SWARM production that condition holds at every height, so a payment
to a `swm1…` address carrying an orchard receiver is proposed into Ironwood,
and so is the change.

## The server, and checking it is the right one

The production indexer is **`lwd-main.swarm.green:8443`**, TLS.

Applications select the profile with
`ClientConfig::builder().set_chain_type(ChainType::SwarmMainnet(genesis))` and
supply the indexer URI explicitly. This chain has no public destination registry
and never falls back to one.

`LightClient::info()` returns a typed `ServerInfo`, rendered to JSON at
presentation boundaries. Beside `chain_name` it now carries:

```json
{
  "chain_name": "swarm-mainnet",
  "genesis_hash": "01c34428b9e67cdd8345e0b365aaa37dd8d2d65d3869e0e5d77d567f2c39afdd"
}
```

`genesis_hash` comes from `LightdInfo.genesisHash`, **field 19** of
`GetLightdInfo`, served by the SWARM indexer from
`Swarm-Official/privacy-zaino` commit `a0f42a41` and carried in this SDK by the
protocol crate pinned in the root `Cargo.toml`
(`Swarm-Official/privacy-lightwallet-protocol-rust`).

Read it as follows, and nowhere differently:

- A **non-empty** value is the chain's identity. Compare it against the genesis
  the wallet was built for, and refuse to sync if they differ. A wallet that
  compares only `chain_name` is comparing a string the operator chose.
- An **empty** value means *this server did not say*, never *no genesis*. Field
  19 is appended rather than inserted, so a server built before it sends
  nothing, and nothing decodes as the empty string. Regtest also sends empty for
  real: its genesis is whatever the local validator made.

The field is additive in both directions. An older client parses a reply that
carries field 19 exactly as it did before, skipping 19 as an unknown field.

## What an application pins

Pin the tag, not a branch commit: **`swarm-sdk-mainnet-1`** on
`Swarm-Official/privacy-zingolib`.

The public form of `ChainType::SwarmMainnet` is unchanged by this release. It
is still the one-field tuple variant carrying a `SwarmMainnetGenesis`, so a
consumer that pinned the branch commit `d9f1a5b8` moves to the tag without
touching its own code. What it gains is `ServerInfo::genesis_hash`, the CLI's
knowledge of the SWARM chains, and this document.

The workspace pins four crates by path in `[patch.crates-io]`
(`zcash_address`, `zcash_protocol`, `zcash_primitives`, `zcash_transparent`)
because each has an exhaustive match that must answer for `NetworkType::SwarmMain`
or `BranchId::SwarmMain`. Cargo ignores a dependency's own `[patch]` table, so an
application with its own workspace must carry the same four patches, at the same
versions, in its own root manifest. `vendor/README.md` states what was changed
in each.
