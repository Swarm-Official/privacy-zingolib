//! Module for configuration and construction of [`crate::lightclient::LightClient`] and [`crate::wallet::LightWallet`].

use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use bip0039::{English, Mnemonic};
use http::uri::InvalidUri;

use zcash_protocol::consensus::{BlockHeight, Parameters};

use pepper_sync::config::{SyncConfig, TransparentAddressDiscovery};
use zingo_common_components::protocol::ActivationHeights;

pub use crate::destination::servers::{IndexerConfig, Location, Role, Trust};

use crate::wallet::{
    WalletBase, WalletSettings,
    error::{KeyError, WalletError},
    keys::unified::UnifiedKeyStore,
};

/// The default indexer URIs, from the census (the sole source of truth for
/// indexer endpoints). NOTE: the census testnet default carries an explicit
/// `:443`, retiring the old portless string this module completed to
/// `:9067` — the drift between that completion and the mobile list's `:443`
/// is what the census exists to end.
/// Default wallet file name
pub const DEFAULT_WALLET_NAME: &str = "zingo-wallet.dat";

/// The mainnet Library Birthday: a block height that had already been mined
/// when this zingolib release was cut.
///
/// Any wallet created by this release necessarily post-dates the release, so
/// this height is a safe [`WalletConfig::NewSeed`] `chain_height` for a
/// wallet created while Indexerless, where no Indexer can report the chain
/// tip. The invariant is that the block has been mined, not merely named: a
/// scheduled-but-future network-upgrade activation height does not qualify.
/// Bumping this constant to a recently observed height is a release-checklist
/// step. See `docs/adr/0007-library-birthday.md`.
pub const LIB_BIRTHDAY_MAINNET: u32 = 3_411_499;

/// The testnet Library Birthday. See [`LIB_BIRTHDAY_MAINNET`].
///
/// NU6.3's testnet activation height. An activation height qualifies only
/// once it has actually been mined, and this one has. NU6.3 is already live on
/// testnet (unlike mainnet, where its activation is still scheduled).
pub const LIB_BIRTHDAY_TESTNET: u32 = 4_134_000;

/// The fixed SwarmTestnet profile's first spendable block and upgrade height.
pub const SWARM_TESTNET_BIRTHDAY: u32 = 1;

/// The stand-in written here until SwarmTestnet's genesis block exists.
///
/// It is the ASCII text `SWARMTESTNETGENESISPLACEHOLDER!!` in hexadecimal, so
/// a hex dump of anything carrying it reads as what it is. No block can hash
/// to it, which is the point: a build that still carries this value cannot be
/// mistaken for one that talks to the real network, and
/// [`swarm_testnet_genesis_is_placeholder`] lets a release gate say so.
pub const SWARM_TESTNET_GENESIS_PLACEHOLDER: &str =
    "535741524d544553544e455447454e45534953504c414345484f4c4445522121";

/// The one place SwarmTestnet's display-order genesis hash is written.
///
/// This is the real hash, from `network/swarm-testnet/manifest.json` in the
/// project repository. Its block is stamped 2026-09-21T12:00:00Z, in the past,
/// so the chain can produce block 1 immediately. An earlier genesis carrying a
/// future timestamp was discarded for exactly that reason: a wallet pinned to
/// it would have synced nothing, with nothing to report.
///
/// Everything that identifies the network — the indexer check, the wallet
/// profile, the tests, the desktop wallet's build-time verification — reads it
/// from here and nowhere else.
pub const SWARM_TESTNET_GENESIS: &str =
    "045993f5c91ea160c7ebda573dd97b0016816bca68d395bfff202779b88e2a28";

/// The light-wallet chain label SwarmTestnet's indexer reports, which is also
/// this profile's wallet identity and data-directory name.
pub const SWARM_TESTNET_NAME: &str = "swarm-testnet";

/// Whether this build still carries [`SWARM_TESTNET_GENESIS_PLACEHOLDER`]
/// rather than a real genesis hash. Release gates refuse a build for which
/// this is true.
#[must_use]
pub const fn swarm_testnet_genesis_is_placeholder() -> bool {
    // `str::eq` is not const, so compare the bytes the two constants hold.
    let (configured, placeholder) = (
        SWARM_TESTNET_GENESIS.as_bytes(),
        SWARM_TESTNET_GENESIS_PLACEHOLDER.as_bytes(),
    );
    if configured.len() != placeholder.len() {
        return false;
    }
    let mut index = 0;
    while index < configured.len() {
        if configured[index] != placeholder[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The upgrade schedule bound to [`SWARM_TESTNET_GENESIS`].
pub fn swarm_testnet_activation_heights() -> ActivationHeights {
    let height = Some(SWARM_TESTNET_BIRTHDAY);
    ActivationHeights::builder()
        .set_overwinter(height)
        .set_sapling(height)
        .set_blossom(height)
        .set_heartwood(height)
        .set_canopy(height)
        .set_nu5(height)
        .set_nu6(height)
        .set_nu6_1(height)
        .set_nu6_2(height)
        .set_nu6_3(height)
        .build()
}

/// Returns the Library Birthday for the given chain: a block height known to
/// have been mined before this zingolib release was cut, safe as the
/// [`WalletConfig::NewSeed`] `chain_height` for a wallet created while
/// Indexerless.
///
/// Restores must not use this, since a restored seed or viewing key may predate
/// the library and always requires a caller-supplied birthday. See
/// `docs/adr/0007-library-birthday.md`.
pub fn lib_birthday(chain: ChainType) -> u32 {
    match chain {
        ChainType::Mainnet => LIB_BIRTHDAY_MAINNET,
        ChainType::Testnet => LIB_BIRTHDAY_TESTNET,
        ChainType::CustomTestnet => SWARM_TESTNET_BIRTHDAY,
        // A regtest chain is born alongside its wallets; scanning from
        // genesis is both correct and cheap.
        ChainType::Regtest(_) => 1,
    }
}

/// The network types a lightclient can connect to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainType {
    /// Mainnet
    Mainnet,
    /// Testnet
    Testnet,
    /// The project's own testnet: SwarmTestnet, labelled
    /// [`SWARM_TESTNET_NAME`] and identified by [`SWARM_TESTNET_GENESIS`].
    /// The variant name is deliberately generic — there is one custom-testnet
    /// slot in this SDK and SwarmTestnet is what currently occupies it.
    CustomTestnet,
    /// Regtest
    Regtest(ActivationHeights),
}

impl std::fmt::Display for ChainType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let chain = match self {
            ChainType::Mainnet => "mainnet",
            ChainType::Testnet => "testnet",
            ChainType::CustomTestnet => SWARM_TESTNET_NAME,
            ChainType::Regtest(_) => "regtest",
        };
        write!(f, "{chain}")
    }
}

impl TryFrom<&str> for ChainType {
    type Error = InvalidChainType;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "mainnet" => Ok(ChainType::Mainnet),
            "testnet" => Ok(ChainType::Testnet),
            SWARM_TESTNET_NAME => Ok(ChainType::CustomTestnet),
            "regtest" => Ok(ChainType::Regtest(ActivationHeights::default())),
            _ => Err(InvalidChainType(value.to_string())),
        }
    }
}

pub(crate) mod consealed {
    use zcash_protocol::consensus::{
        BlockHeight, MAIN_NETWORK, NetworkType, NetworkUpgrade, Parameters, TEST_NETWORK,
    };

    use super::ChainType;

    impl Parameters for ChainType {
        fn network_type(&self) -> NetworkType {
            match self {
                ChainType::Mainnet => NetworkType::Main,
                ChainType::Testnet => NetworkType::Test,
                ChainType::CustomTestnet => NetworkType::Test,
                ChainType::Regtest(_) => NetworkType::Regtest,
            }
        }

        fn activation_height(&self, nu: NetworkUpgrade) -> Option<BlockHeight> {
            match self {
                ChainType::Mainnet => MAIN_NETWORK.activation_height(nu),
                ChainType::Testnet => TEST_NETWORK.activation_height(nu),
                ChainType::CustomTestnet => {
                    ChainType::Regtest(super::swarm_testnet_activation_heights())
                        .activation_height(nu)
                }
                ChainType::Regtest(activation_heights) => match nu {
                    NetworkUpgrade::Overwinter => {
                        activation_heights.overwinter().map(BlockHeight::from_u32)
                    }
                    NetworkUpgrade::Sapling => {
                        activation_heights.sapling().map(BlockHeight::from_u32)
                    }
                    NetworkUpgrade::Blossom => {
                        activation_heights.blossom().map(BlockHeight::from_u32)
                    }
                    NetworkUpgrade::Heartwood => {
                        activation_heights.heartwood().map(BlockHeight::from_u32)
                    }
                    NetworkUpgrade::Canopy => {
                        activation_heights.canopy().map(BlockHeight::from_u32)
                    }
                    NetworkUpgrade::Nu5 => activation_heights.nu5().map(BlockHeight::from_u32),
                    NetworkUpgrade::Nu6 => activation_heights.nu6().map(BlockHeight::from_u32),
                    NetworkUpgrade::Nu6_1 => activation_heights.nu6_1().map(BlockHeight::from_u32),
                    NetworkUpgrade::Nu6_2 => activation_heights.nu6_2().map(BlockHeight::from_u32),
                    NetworkUpgrade::Nu6_3 => activation_heights.nu6_3().map(BlockHeight::from_u32),
                },
            }
        }
    }
}

/// Invalid chain type.
#[derive(thiserror::Error, Debug)]
#[error("Invalid chain type '{0}'. Expected 'mainnet', 'testnet', 'swarm-testnet' or 'regtest'.")]
pub struct InvalidChainType(String);

/// Configuration data for the construction of a [`crate::wallet::LightWallet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletConfig {
    /// Generate a wallet with a new seed for a number of accounts.
    NewSeed {
        no_of_accounts: NonZeroU32,
        chain_height: u32,
        wallet_settings: WalletSettings,
    },
    /// Generate a wallet from a mnemonic phrase for a number of accounts.
    MnemonicPhrase {
        mnemonic_phrase: String,
        no_of_accounts: NonZeroU32,
        birthday: u32,
        wallet_settings: WalletSettings,
    },
    /// Generate a wallet from an encoded unified full viewing key.
    // TODO: take concrete UFVK type
    Ufvk {
        ufvk: String,
        birthday: u32,
        wallet_settings: WalletSettings,
    },
    /// Generate a wallet from a unified spending key.
    // TODO: take concrete USK type
    Usk {
        usk: Vec<u8>,
        birthday: u32,
        wallet_settings: WalletSettings,
    },
    /// Read from wallet file.
    Read,
}

impl WalletConfig {
    /// Resolves the wallet config into the base data needed to construct the wallet.
    ///
    /// `NewSeed` generates the wallet base data from a new 24-word mnemonic.
    #[allow(clippy::result_large_err)]
    pub(crate) fn resolve(self, chain_type: ChainType) -> Result<WalletBase, WalletError> {
        match self {
            WalletConfig::NewSeed {
                no_of_accounts,
                chain_height,
                wallet_settings,
            } => {
                let sapling_activation_height = chain_type
                    .activation_height(zcash_protocol::consensus::NetworkUpgrade::Sapling)
                    .expect("should have some sapling activation height");
                let birthday = if chain_type == ChainType::CustomTestnet {
                    BlockHeight::from_u32(SWARM_TESTNET_BIRTHDAY)
                } else {
                    sapling_activation_height.max(BlockHeight::from_u32(chain_height) - 100)
                };

                WalletConfig::MnemonicPhrase {
                    mnemonic_phrase: Mnemonic::<English>::generate(bip0039::Count::Words24)
                        .into_phrase(),
                    no_of_accounts,
                    birthday: u32::from(birthday),
                    wallet_settings,
                }
                .resolve(chain_type)
            }
            WalletConfig::MnemonicPhrase {
                mnemonic_phrase: mnemonic,
                no_of_accounts,
                birthday,
                wallet_settings,
            } => {
                let mnemonic = Mnemonic::from_phrase(mnemonic)?;
                let no_of_accounts = u32::from(no_of_accounts);
                let unified_key_store = (0..no_of_accounts)
                    .map(|account_index| {
                        let account_id = zip32::AccountId::try_from(account_index)?;
                        Ok((
                            account_id,
                            UnifiedKeyStore::new_from_mnemonic(chain_type, &mnemonic, account_id)?,
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, KeyError>>()?;
                Ok(WalletBase {
                    unified_key_store,
                    mnemonic: Some(mnemonic),
                    birthday: BlockHeight::from_u32(birthday),
                    wallet_settings,
                })
            }
            WalletConfig::Ufvk {
                ufvk,
                birthday,
                wallet_settings,
            } => {
                let mut unified_key_store = BTreeMap::new();
                unified_key_store.insert(
                    zip32::AccountId::ZERO,
                    UnifiedKeyStore::new_from_ufvk(chain_type, ufvk)?,
                );
                Ok(WalletBase {
                    unified_key_store,
                    mnemonic: None,
                    birthday: BlockHeight::from_u32(birthday),
                    wallet_settings,
                })
            }
            WalletConfig::Usk {
                usk,
                birthday,
                wallet_settings,
            } => {
                let mut unified_key_store = BTreeMap::new();
                unified_key_store.insert(
                    zip32::AccountId::ZERO,
                    UnifiedKeyStore::new_from_usk(usk.as_slice())?,
                );
                Ok(WalletBase {
                    unified_key_store,
                    mnemonic: None,
                    birthday: BlockHeight::from_u32(birthday),
                    wallet_settings,
                })
            }
            WalletConfig::Read => Err(WalletError::WalletAlreadyCreated),
        }
    }
}

/// Constructs an `http::Uri` from `server`, adding an `http://` prefix and a
/// `:9067` port when they are missing.
pub fn construct_indexer_uri(server: String) -> Result<http::Uri, InvalidUri> {
    if server.is_empty() {
        return Ok(http::Uri::default());
    }
    let mut s = if server.starts_with("http") {
        server
    } else {
        "http://".to_string() + &server
    };
    let uri: http::Uri = s.parse()?;
    if uri.port().is_none() {
        s += ":9067";
    }
    s.parse()
}

/// Configuration data for the construction of a [`crate::lightclient::LightClient`].
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// URI of the indexer the lightclient is connected to. `None` means the
    /// client is Indexerless (no Indexer connection).
    indexer_uri: Option<http::Uri>,
    /// URI the Ironwood migration parts are transmitted to. Transmitting to a
    /// different server than the one used for synchronization reduces the
    /// correlation between the two (ZIP 318). While Mixnet Mode is on (the
    /// `nym` feature, ADR 0011), this URI is dialed through the mixnet and
    /// must be https on a host distinct from an untrusted synchronization
    /// endpoint's (a shared host is refused). Unset, parts go to one
    /// drawn Destination per submission. When both are
    /// `None` the client emits no network traffic and transmission fails
    /// with [`crate::lightclient::error::LightClientError::Offline`].
    migration_transmission_uri: Option<http::Uri>,
    /// The consumer's indexer classifications.
    indexers: Vec<IndexerConfig>,
    /// Overrides the chain's trust for an unclassified remote indexer.
    remote_indexer_trust: Option<Trust>,
    /// Chain type of the blockchain the lightclient is connected to.
    chain_type: ChainType,
    /// Directory where the wallet file will be created. By default, this will be in ~/.zcash on Linux and %APPDATA%\Zcash on Windows.
    wallet_dir: PathBuf,
    /// Wallet file name. This will be created in the `wallet_dir`.
    wallet_name: String,
    /// Wallet config.
    wallet_config: WalletConfig,
}

impl ClientConfig {
    /// Constructs a default builder.
    #[must_use]
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::default()
    }

    /// Returns indexer URI, or `None` if the client is configured for offline use.
    #[must_use]
    pub fn indexer_uri(&self) -> Option<http::Uri> {
        self.indexer_uri.clone()
    }

    /// Returns the migration transmission URI, if one is configured.
    #[must_use]
    pub fn migration_transmission_uri(&self) -> Option<http::Uri> {
        self.migration_transmission_uri.clone()
    }

    /// Returns the indexer classifications.
    #[must_use]
    pub fn indexers(&self) -> &[IndexerConfig] {
        &self.indexers
    }

    /// Returns the remote trust override, if any.
    #[must_use]
    pub fn remote_indexer_trust(&self) -> Option<Trust> {
        self.remote_indexer_trust
    }

    /// Returns wallet directory.
    #[must_use]
    pub fn chain_type(&self) -> ChainType {
        self.chain_type
    }

    /// Returns wallet directory.
    #[must_use]
    pub fn wallet_dir(&self) -> PathBuf {
        self.wallet_dir.clone()
    }

    /// Returns wallet file name.
    #[must_use]
    pub fn wallet_name(&self) -> &str {
        &self.wallet_name
    }

    /// Returns wallet config.
    #[must_use]
    pub fn wallet_config(&self) -> WalletConfig {
        self.wallet_config.clone()
    }

    /// Returns full path to wallet file.
    #[must_use]
    pub fn get_wallet_path(&self) -> Box<Path> {
        let mut wallet_path = self.wallet_dir();
        wallet_path.push(self.wallet_name());

        wallet_path.into_boxed_path()
    }
}

/// Builder for [`ClientConfig`].
#[derive(Clone, Debug)]
pub struct ClientConfigBuilder {
    indexer_uri: Option<http::Uri>,
    migration_transmission_uri: Option<http::Uri>,
    indexers: Vec<IndexerConfig>,
    remote_indexer_trust: Option<Trust>,
    chain_type: ChainType,
    wallet_dir: Option<PathBuf>,
    wallet_name: Option<String>,
    wallet_config: WalletConfig,
}

impl ClientConfigBuilder {
    /// Constructs a new builder for [`ClientConfig`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect to an indexer at the given URI.
    ///
    /// Without this call the client starts offline. See [`Self::build`].
    ///
    /// TODO: Will be renamed `set_indexer` and accept an `Indexer` type from
    /// `zingo-netutils` instead of `http::Uri`.
    pub fn set_indexer_uri(mut self, indexer_uri: http::Uri) -> Self {
        self.indexer_uri = Some(indexer_uri);
        self
    }

    /// Set a dedicated URI for transmitting Ironwood migration parts,
    /// distinct from the synchronization endpoint.
    pub fn set_migration_transmission_uri(mut self, migration_transmission_uri: http::Uri) -> Self {
        self.migration_transmission_uri = Some(migration_transmission_uri);
        self
    }

    /// Classify one indexer.
    pub fn add_indexer(mut self, indexer: IndexerConfig) -> Self {
        self.indexers.push(indexer);
        self
    }

    /// Override the chain's trust for an unclassified remote indexer.
    pub fn set_remote_indexer_trust(mut self, trust: Trust) -> Self {
        self.remote_indexer_trust = Some(trust);
        self
    }

    /// Set chain type.
    pub fn set_chain_type(mut self, chain_type: ChainType) -> Self {
        self.chain_type = chain_type;
        self
    }

    /// Set wallet directory.
    pub fn set_wallet_dir(mut self, dir: PathBuf) -> Self {
        self.wallet_dir = Some(dir);
        self
    }

    /// Set wallet file name.
    pub fn set_wallet_name(mut self, wallet_name: String) -> Self {
        self.wallet_name = Some(wallet_name);
        self
    }

    /// Set wallet config.
    pub fn set_wallet_config(mut self, wallet_config: WalletConfig) -> Self {
        self.wallet_config = wallet_config;
        self
    }

    /// Build a [`ClientConfig`] from the builder.
    ///
    /// The default indexer is `None`, so the resulting [`crate::lightclient::LightClient`] starts
    /// in offline mode. All local operations (balance, addresses, proposals) work immediately.
    /// Call [`crate::lightclient::LightClient::set_indexer_uri`] to connect when the network
    /// is available, then [`crate::lightclient::LightClient::sync`] to fetch blocks.
    ///
    /// To start online, call [`set_indexer_uri`](Self::set_indexer_uri) before building.
    pub fn build(self) -> Result<ClientConfig, ClientConfigError> {
        let wallet_dir = wallet_dir_or_default(self.wallet_dir, self.chain_type)?;
        let wallet_name = wallet_name_or_default(self.wallet_name);

        Ok(ClientConfig {
            indexer_uri: self.indexer_uri,
            migration_transmission_uri: self.migration_transmission_uri,
            indexers: self.indexers,
            remote_indexer_trust: self.remote_indexer_trust,
            chain_type: self.chain_type,
            wallet_dir,
            wallet_name,
            wallet_config: self.wallet_config,
        })
    }
}

impl Default for ClientConfigBuilder {
    fn default() -> Self {
        Self {
            indexer_uri: None,
            migration_transmission_uri: None,
            indexers: Vec::new(),
            remote_indexer_trust: None,
            wallet_dir: None,
            wallet_name: None,
            chain_type: ChainType::Mainnet,
            wallet_config: WalletConfig::NewSeed {
                no_of_accounts: NonZeroU32::try_from(1).expect("hard coded non-zero integer"),
                chain_height: 1,
                wallet_settings: WalletSettings {
                    sync_config: SyncConfig {
                        transparent_address_discovery: TransparentAddressDiscovery::minimal(),
                        performance_level: pepper_sync::config::PerformanceLevel::High,
                    },
                    min_confirmations: NonZeroU32::try_from(3)
                        .expect("hard coded non-zero integer"),
                },
            },
        }
    }
}

fn wallet_name_or_default(opt_wallet_name: Option<String>) -> String {
    let wallet_name = opt_wallet_name.unwrap_or_else(|| DEFAULT_WALLET_NAME.into());
    if wallet_name.is_empty() {
        DEFAULT_WALLET_NAME.into()
    } else {
        wallet_name
    }
}

fn wallet_dir_or_default(
    opt_wallet_dir: Option<PathBuf>,
    chain: ChainType,
) -> Result<PathBuf, ClientConfigError> {
    let wallet_dir: PathBuf;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        wallet_dir = opt_wallet_dir.ok_or_else(|| ClientConfigError::WalletDirNotSpecified)?;
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        wallet_dir = opt_wallet_dir.clone().map_or_else(
            || {
                let mut dir = dirs::data_dir().ok_or(ClientConfigError::UsersDataDirNotFound)?;

                #[cfg(any(target_os = "macos", target_os = "windows"))]
                {
                    dir.push("Zcash");
                }

                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    dir.push(".zcash");
                }

                match chain {
                    ChainType::Mainnet => {}
                    ChainType::Testnet => dir.push("testnet3"),
                    ChainType::CustomTestnet => dir.push(SWARM_TESTNET_NAME),
                    ChainType::Regtest(_) => dir.push("regtest"),
                }

                Ok(dir)
            },
            Ok,
        )?;

        // Create directory if it doesn't exist on non-mobile platforms
        std::fs::create_dir_all(wallet_dir.clone())
            .map_err(|e| ClientConfigError::FileError(e.to_string()))?;
    }

    Ok(wallet_dir)
}

/// Invalid client config.
#[derive(thiserror::Error, Debug, Clone)]
pub enum ClientConfigError {
    #[error("Wallet directory must be specified for iOS and Android platforms.")]
    WalletDirNotSpecified,
    #[error("User's default data directory not found.")]
    UsersDataDirNotFound,
    #[error("Failed to create wallet directory. {0}")]
    FileError(String),
}

#[cfg(test)]
mod tests {
    use crate::config::{ChainType, ClientConfig};

    #[test]
    fn swarm_profile_keeps_test_encoding_and_early_upgrades() {
        use super::{SWARM_TESTNET_BIRTHDAY, SWARM_TESTNET_NAME};
        use zcash_protocol::consensus::{
            BlockHeight, BranchId, NetworkType, NetworkUpgrade, Parameters,
        };

        let chain = ChainType::CustomTestnet;
        let activation = BlockHeight::from_u32(SWARM_TESTNET_BIRTHDAY);
        assert_eq!(ChainType::try_from(SWARM_TESTNET_NAME).unwrap(), chain);
        assert_eq!(chain.to_string(), SWARM_TESTNET_NAME);
        assert_eq!(chain.network_type(), NetworkType::Test);
        assert_eq!(super::lib_birthday(chain), SWARM_TESTNET_BIRTHDAY);
        for upgrade in [
            NetworkUpgrade::Overwinter,
            NetworkUpgrade::Sapling,
            NetworkUpgrade::Blossom,
            NetworkUpgrade::Heartwood,
            NetworkUpgrade::Canopy,
            NetworkUpgrade::Nu5,
            NetworkUpgrade::Nu6,
            NetworkUpgrade::Nu6_1,
            NetworkUpgrade::Nu6_2,
            NetworkUpgrade::Nu6_3,
        ] {
            assert_eq!(chain.activation_height(upgrade), Some(activation));
        }
        assert_eq!(BranchId::for_height(&chain, activation), BranchId::Nu6_3);
        assert_ne!(chain, ChainType::Testnet);
    }

    #[test]
    fn swarm_wallet_disk_identity_and_birthday_roundtrip() {
        use super::{SWARM_TESTNET_BIRTHDAY, WalletConfig};
        use crate::wallet::{LightWallet, WalletSettings};

        let chain = ChainType::CustomTestnet;
        let mut wallet = LightWallet::new(
            chain,
            WalletConfig::NewSeed {
                no_of_accounts: std::num::NonZeroU32::MIN,
                chain_height: SWARM_TESTNET_BIRTHDAY,
                wallet_settings: WalletSettings::default(),
            },
        )
        .unwrap();
        assert_eq!(u32::from(wallet.birthday()), SWARM_TESTNET_BIRTHDAY);
        let mut bytes = Vec::new();
        wallet.write(&mut bytes, &chain).unwrap();
        let restored = LightWallet::read(bytes.as_slice(), chain).unwrap();
        assert_eq!(restored.chain_type(), chain);
        assert_eq!(u32::from(restored.birthday()), SWARM_TESTNET_BIRTHDAY);
        assert!(LightWallet::read(bytes.as_slice(), ChainType::Testnet).is_err());
        assert!(LightWallet::read(bytes.as_slice(), ChainType::Mainnet).is_err());
        assert!(
            LightWallet::read(
                bytes.as_slice(),
                ChainType::Regtest(crate::ActivationHeights::default())
            )
            .is_err()
        );

        // A wallet file written by the retired Privacy Testnet build carries
        // chain tag 3, the byte that follows the u64 version word. Opening one
        // here would scan it against SwarmTestnet and save that state back
        // over it, so it is refused by name.
        let mut retired = bytes.clone();
        retired[8] = 3;
        let refusal = LightWallet::read(retired.as_slice(), chain)
            .err()
            .expect("a Privacy Testnet wallet file is not a SwarmTestnet wallet file");
        assert!(
            refusal.to_string().contains("Privacy Testnet"),
            "the refusal should name the network the file belongs to: {refusal}"
        );
    }

    /// The genesis constant is a well-formed hash, and the build says
    /// truthfully whether it is the real one or the stand-in. The gate that
    /// refuses a release build reads that answer, so it has to keep being
    /// right after the real hash lands, not only before it.
    #[test]
    fn swarm_genesis_is_a_declared_placeholder_of_the_right_shape() {
        use super::{
            SWARM_TESTNET_GENESIS, SWARM_TESTNET_GENESIS_PLACEHOLDER,
            swarm_testnet_genesis_is_placeholder,
        };

        assert_eq!(SWARM_TESTNET_GENESIS.len(), 64);
        assert!(
            SWARM_TESTNET_GENESIS
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(
            swarm_testnet_genesis_is_placeholder(),
            SWARM_TESTNET_GENESIS == SWARM_TESTNET_GENESIS_PLACEHOLDER
        );
        assert_eq!(
            String::from_utf8(hex::decode(SWARM_TESTNET_GENESIS_PLACEHOLDER).unwrap()).unwrap(),
            "SWARMTESTNETGENESISPLACEHOLDER!!"
        );
        // The real hash is in force, so the release gate now passes. Written
        // as its own assertion rather than folded into the one above: if a
        // future edit ever put the stand-in back, this is the line that says
        // so in one word.
        assert!(!swarm_testnet_genesis_is_placeholder());
        assert_ne!(SWARM_TESTNET_GENESIS, SWARM_TESTNET_GENESIS_PLACEHOLDER);
    }

    #[tokio::test]
    async fn test_load_clientconfig() {
        let valid_uri =
            crate::config::construct_indexer_uri("https://zec.rocks:443".to_string()).unwrap();

        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let valid_config = ClientConfig::builder()
            .set_indexer_uri(valid_uri.clone())
            .set_chain_type(ChainType::Mainnet)
            .set_wallet_dir(temp_path)
            .build()
            .unwrap();

        assert_eq!(valid_config.indexer_uri(), Some(valid_uri));
        assert_eq!(valid_config.chain_type(), ChainType::Mainnet);
    }

    /// The Library Birthday must land at or above Sapling activation on the
    /// public chains, so a NewSeed wallet built from it starts scanning at
    /// the library floor rather than the bottom of the chain.
    #[test]
    fn lib_birthday_exceeds_sapling_activation() {
        use zcash_protocol::consensus::{NetworkUpgrade, Parameters};

        for chain in [ChainType::Mainnet, ChainType::Testnet] {
            let sapling_activation = chain
                .activation_height(NetworkUpgrade::Sapling)
                .expect("public chains have a sapling activation height");
            assert!(
                crate::config::lib_birthday(chain) > u32::from(sapling_activation),
                "lib_birthday({chain}) must exceed sapling activation"
            );
        }
    }
}
