# Privacy testnet wallet profile

`ChainType::CustomTestnet` selects the fixed `privacy-testnet` network. Its V2 genesis hash is `01d6e85dd3c1c128941a849c5025cd2e437258811a2551b82aefd68686c982e1`. The node genesis manifest is maintained in the Privacy Network repository under `genesis/privacy-testnet-v2/manifest.json`.

The profile uses the upstream `NetworkType::Test` address encodings and `ActivationHeights` with every upgrade through NU6.3 activated at height one. New wallets begin scanning at height one. The wallet file stores a dedicated chain tag, and the default data directory ends with `privacy-testnet`. Loading its wallet file under another chain produces a chain-mismatch error. This profile binds its disk tag to this genesis and upgrade schedule.

Applications select the profile through `ClientConfig::builder().set_chain_type(ChainType::CustomTestnet)` and explicitly provide the user's indexer URI. Connection initialization and subsequent sync requests verify the indexer's reported chain name and genesis compact block. `LightClient::verify_network()` exposes the same check. `set_indexer_uri()` returns `LightClientError` so a failed identity check can propagate to the application.

Direct transmission verifies the destination immediately before submitting a transaction, including destinations selected for migration. The project's destination registry contains user-configured endpoints. Mixnet transmission requires a future adapter that verifies identity through that transport, so this profile currently requires the direct route. Existing network profiles retain their upstream routing behavior.

This change preserves upstream key derivation, transaction construction, proof generation and cryptographic dependencies. Genesis checks identify the selected network within the existing light-client trust model. Applications should label these test coins separately from ZEC and keep fiat pricing disabled until project market discovery exists.

Focused regression tests use the `privacy_` filter. They cover upgrade parameters, wallet serialization and cross-network rejection, network identity byte order, explicit endpoint selection, and rejection of a different chain before sync or sending.

## Recorded validation

On 2026-09-21, Rust 1.96.0 on WSL passed six focused SDK tests, the CLI explicit-indexer test, and live checks that synchronized the V2 indexer and rejected a separate regtest indexer. The feature sweep covered thirteen SDK and six CLI combinations. Disposable V2 wallets also persisted and reloaded actual mining receipts, with upstream coinbase maturity keeping their spendable balance at zero.

The required `makers sync-bench` comparison used one online mixnet session per revision and the same fixed birthday, 3,490,501. Both sessions booted the upstream Nym transport and synchronized the pinned public indexer. The public tip advanced between runs.

| Revision | Fully scanned height | Blocks from birthday | Boot | Sync | Boot + sync | Sync throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Parent `7a542ce4` | 3,490,607 | 107 | 48.805 s | 3.460 s | 52.265 s | 30.9 blocks/s |
| Privacy profile change | 3,490,603 | 103 | 70.667 s | 4.437 s | 75.104 s | 23.2 blocks/s |

The current change was slower in this sample. Causal attribution remains unresolved: these single samples include variable live-network and Nym startup time, with bounded local test mining running on the same host. Compilation finished before each measured session. A performance conclusion requires repeated controlled measurements. The earlier parent attempt failed during indexer connection, and an earlier large-range current session was stopped through its normal quit command before selecting this recent range.
