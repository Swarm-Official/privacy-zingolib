# SwarmTestnet wallet profile

`ChainType::CustomTestnet` selects the fixed SwarmTestnet network, whose light-wallet chain label is `swarm-testnet`. This profile replaces the retired Privacy Testnet profile that the same variant carried on the `privacy-testnet-wallet` branch; there is one custom-testnet slot in this SDK and SwarmTestnet occupies it.

## The genesis hash is one constant, and it is currently a placeholder

`SWARM_TESTNET_GENESIS` in `zingolib/src/config.rs` is the only place the genesis hash is written. Everything else — the indexer identity check, the wallet profile, the tests, the desktop wallet's build-time verification — reads it from there.

SwarmTestnet's genesis block does not exist yet. Until it does, that constant holds `SWARM_TESTNET_GENESIS_PLACEHOLDER`, which is the ASCII text `SWARMTESTNETGENESISPLACEHOLDER!!` in hexadecimal. No block can hash to it, so a build carrying it cannot be mistaken for one that talks to the real network, and `swarm_testnet_genesis_is_placeholder()` lets a release gate refuse such a build. Setting the real hash means editing that one constant and nothing else.

## Profile

The profile uses the upstream `NetworkType::Test` address encodings and `ActivationHeights` with every upgrade through NU6.3 activated at height one. New wallets begin scanning at height one (`SWARM_TESTNET_BIRTHDAY`). The default data directory ends with `swarm-testnet`, and the wallet file stores chain tag 5.

Tag 5, not 3: 3 was the retired Privacy Testnet, whose wallet files exist on the owner's machine and may hold real coins on that chain. SwarmTestnet has a different genesis, so reading one of those files here would open the wallet against the wrong chain and write SwarmTestnet's scan state back over it. `chain_type_from_tag` refuses tag 3 by name instead. Tag 4 is skipped because the version 40 reader separates a chain tag from a chain-name string on that same byte, where 4 and 7 are string lengths.

Loading a SwarmTestnet wallet file under any other chain still produces a chain-mismatch error, and loading any other chain's file here does too.

## Indexer identity

Applications select the profile through `ClientConfig::builder().set_chain_type(ChainType::CustomTestnet)` and explicitly provide the user's indexer URI; this chain has no public destination registry and never falls back to one. Connection initialization and subsequent sync requests verify the indexer's reported chain name (`swarm-testnet`) and its genesis compact block against `SWARM_TESTNET_GENESIS`. `LightClient::verify_network()` exposes the same check. `set_indexer_uri()` returns `LightClientError`, so a failed identity check propagates to the application.

Direct transmission verifies the destination immediately before submitting a transaction, including destinations selected for migration. Mixnet transmission would need an adapter that verifies identity through that transport, so this profile currently requires the direct route. Existing network profiles retain their upstream routing behaviour.

## Scope

This profile changes network identity constants, chain label strings, the wallet-file chain tag and the tests that cover them. It preserves upstream key derivation, signing, proving, note scanning, address encoding and transaction construction. Genesis checks identify the selected network within the existing light-client trust model. Applications should label these test coins `SWARM`, separately from ZEC, and keep fiat pricing disabled: there is no market and these coins have no value.

Focused regression tests use the `swarm_` filter. They cover upgrade parameters, wallet serialization, refusal of a retired Privacy Testnet wallet file, cross-network rejection, network identity byte order, the genesis placeholder's shape, explicit endpoint selection, and rejection of a different chain before sync or sending.

## Provenance of the profile this one replaces

The Privacy Testnet profile it descends from was validated on 2026-09-21 with Rust 1.96.0 on WSL: six focused SDK tests, the CLI explicit-indexer test, and live checks that synchronized the PrivacyTestnetV2 indexer and rejected a separate regtest indexer. That evidence belongs to that network and its genesis `01d6e85d…`; it is not evidence about SwarmTestnet, which has neither a genesis nor a running indexer yet. It is recorded in the `privacy-testnet-wallet` branch's copy of this document.
