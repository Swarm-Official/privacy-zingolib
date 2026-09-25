use zcash_address::unified::{Address, Encoding};
use zcash_client_backend::address::Address as RecipientAddress;
use zcash_keys::encoding::AddressCodec as _;
use zcash_keys::keys::{UnifiedAddressRequest, UnifiedSpendingKey};
use zcash_protocol::consensus::{NetworkConstants, NetworkType, Parameters};
use zingolib::config::{ChainType, SwarmMainnetGenesis};
use zip32::AccountId;

const FIXTURE_SEED_LENGTH: usize = 32;
const FIXTURE_SEED: [u8; FIXTURE_SEED_LENGTH] = [0; FIXTURE_SEED_LENGTH];
const LEGACY_ADDRESS: &str = "utest10c5kutapazdnf8ztl3pu43nkfsjx89fy3uuff8tsmxm6s86j37pe7uz94z5jhkl49pqe8yz75rlsaygexk6jpaxwx0esjr8wm5ut7d5s";

#[test]
fn derived_wallet_address_uses_swarm_prefix() {
    let chain = ChainType::CustomTestnet;
    let spending = UnifiedSpendingKey::from_seed(&chain, &FIXTURE_SEED, AccountId::ZERO).unwrap();
    let viewing = spending.to_unified_full_viewing_key();
    let (address, _) = viewing
        .default_address(UnifiedAddressRequest::AllAvailableKeys)
        .unwrap();
    let encoded = address.encode(&chain);
    assert!(encoded.starts_with("swarm1"));
    assert!(viewing.encode(&chain).starts_with("uviewswarm1"));
    let (network, parsed) = Address::decode(&encoded).unwrap();
    assert_eq!(network, chain.network_type());
    assert_eq!(parsed.encode(&network), encoded);
}

#[test]
fn legacy_address_keeps_its_receivers() {
    let (network, legacy) = Address::decode(LEGACY_ADDRESS).unwrap();
    assert_eq!(network, NetworkType::Test);
    let encoded = legacy.encode(&ChainType::CustomTestnet.network_type());
    assert!(encoded.starts_with("swarm1"));
    assert_eq!(Address::decode(&encoded).unwrap(), (network, legacy));
    assert!(Address::decode(&encoded.replacen("swarm", "SwarM", 1)).is_err());
}

// ---------------------------------------------------------------------------
// The SWARM production network's encodings, and the wall between them and
// SwarmTestnet's. The transparent literals are the golden vectors from the
// vendored protocol crate (an all-zero hash160); the SwarmTestnet ones are
// published addresses from `network/swarm-testnet/DESTINATIONS.md` and the
// upstream test vectors.
// ---------------------------------------------------------------------------

/// A stand-in for the genesis the SWARM launch ceremony will produce.
const CEREMONY_GENESIS: &str = "00d4b1cb01d6bd2d1a3a4a49bba6fd0a4c2e2f7c0d6e5b4a39281706f5e4d3c2";

const SWARM_MAINNET_P2PKH: &str = "s1MCkDhVejM4RqDyRR1rEJkudd26FVWipPD";
const SWARM_MAINNET_P2SH: &str = "s3Mtm9Ez6HFNovPfrY7WpjPGZmYNxztrxbb";
const SWARM_TESTNET_P2PKH: &str = "tm9iMLAuYMzJ6jtFLcA7rzUmfreGuKvr7Ma";
const SWARM_TESTNET_P2SH: &str = "t2DGVURG5tAyXXSkj85JV5xbvTobYv7H99n";

fn swarm_mainnet() -> ChainType {
    ChainType::SwarmMainnet(SwarmMainnetGenesis::from_display_hex(CEREMONY_GENESIS).unwrap())
}

/// The unified address this seed derives on `chain`, as a wallet would show it.
fn derived_unified(chain: &ChainType) -> String {
    let spending = UnifiedSpendingKey::from_seed(chain, &FIXTURE_SEED, AccountId::ZERO).unwrap();
    let (address, _) = spending
        .to_unified_full_viewing_key()
        .default_address(UnifiedAddressRequest::AllAvailableKeys)
        .unwrap();
    address.encode(chain)
}

/// The Sapling address this seed derives on `chain`.
///
/// Derived rather than written out: a wallet validates what it decodes, and the
/// all-zero payload the encoding vectors use is not a curve point.
fn derived_sapling(chain: &ChainType) -> String {
    let spending = UnifiedSpendingKey::from_seed(chain, &FIXTURE_SEED, AccountId::ZERO).unwrap();
    let viewing = spending.to_unified_full_viewing_key();
    let (_, address) = viewing
        .sapling()
        .expect("the seed derives a sapling key")
        .default_address();
    address.encode(chain)
}

/// A wallet on the SWARM production profile derives `swm1…` addresses and
/// `uviewswm1…` viewing keys, under coin type 9767.
#[test]
fn swarm_mainnet_derives_its_own_encodings() {
    let chain = swarm_mainnet();
    assert_eq!(chain.network_type(), NetworkType::SwarmMain);
    assert_eq!(chain.coin_type(), 9767);

    let encoded = derived_unified(&chain);
    assert!(encoded.starts_with("swm1"), "{encoded}");
    let spending = UnifiedSpendingKey::from_seed(&chain, &FIXTURE_SEED, AccountId::ZERO).unwrap();
    assert!(
        spending
            .to_unified_full_viewing_key()
            .encode(&chain)
            .starts_with("uviewswm1")
    );

    let (network, parsed) = Address::decode(&encoded).unwrap();
    assert_eq!(network, NetworkType::SwarmMain);
    assert_eq!(parsed.encode(&network), encoded);

    // Two chains, two addresses from one seed. The SWARM coin type is what
    // makes them different keys, not just different spellings.
    assert_ne!(encoded, derived_unified(&ChainType::CustomTestnet));
}

/// The SWARM production profile accepts its own recipients and refuses
/// SwarmTestnet's, including the published testnet destinations.
#[test]
fn swarm_mainnet_accepts_only_swarm_mainnet_recipients() {
    let chain = swarm_mainnet();
    let unified = derived_unified(&chain);

    let sapling = derived_sapling(&chain);
    assert!(sapling.starts_with("zswmsapling1"), "{sapling}");

    for accepted in [
        unified.as_str(),
        sapling.as_str(),
        SWARM_MAINNET_P2PKH,
        SWARM_MAINNET_P2SH,
        "texswm1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqpfw3pr",
    ] {
        assert!(
            RecipientAddress::decode(&chain, accepted).is_some(),
            "{accepted} is a swarm-mainnet address",
        );
    }

    for refused in [
        derived_unified(&ChainType::CustomTestnet).as_str(),
        derived_sapling(&ChainType::CustomTestnet).as_str(),
        LEGACY_ADDRESS,
        SWARM_TESTNET_P2PKH,
        SWARM_TESTNET_P2SH,
        "textest1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqggnfr9",
    ] {
        assert!(
            RecipientAddress::decode(&chain, refused).is_none(),
            "{refused} is not a swarm-mainnet address",
        );
    }
}

/// And the wall stands in the other direction: SwarmTestnet keeps taking its
/// own recipients and refuses every SWARM production one.
#[test]
fn swarm_testnet_still_refuses_swarm_mainnet_recipients() {
    let chain = ChainType::CustomTestnet;

    for accepted in [
        derived_unified(&chain).as_str(),
        derived_sapling(&chain).as_str(),
        SWARM_TESTNET_P2PKH,
        SWARM_TESTNET_P2SH,
    ] {
        assert!(
            RecipientAddress::decode(&chain, accepted).is_some(),
            "{accepted} is a swarm-testnet address",
        );
    }

    for refused in [
        derived_unified(&swarm_mainnet()).as_str(),
        derived_sapling(&swarm_mainnet()).as_str(),
        SWARM_MAINNET_P2PKH,
        SWARM_MAINNET_P2SH,
        "texswm1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqpfw3pr",
    ] {
        assert!(
            RecipientAddress::decode(&chain, refused).is_none(),
            "{refused} is not a swarm-testnet address",
        );
    }
}

/// Zcash Mainnet keeps its own encodings, and neither SWARM chain's strings
/// reach it. This is the assertion that says "mainnet" still means upstream.
#[test]
fn zcash_mainnet_is_untouched() {
    let zcash = ChainType::Mainnet;
    assert_eq!(zcash.network_type(), NetworkType::Main);
    assert_eq!(zcash.coin_type(), 133);
    assert!(derived_unified(&zcash).starts_with("u1"));

    for refused in [
        derived_unified(&swarm_mainnet()).as_str(),
        derived_sapling(&swarm_mainnet()).as_str(),
        SWARM_MAINNET_P2PKH,
        SWARM_MAINNET_P2SH,
        derived_unified(&ChainType::CustomTestnet).as_str(),
        SWARM_TESTNET_P2SH,
    ] {
        assert!(
            RecipientAddress::decode(&zcash, refused).is_none(),
            "{refused} is not a Zcash Mainnet address",
        );
    }
    for refused in [derived_unified(&zcash).as_str()] {
        assert!(RecipientAddress::decode(&swarm_mainnet(), refused).is_none());
        assert!(RecipientAddress::decode(&ChainType::CustomTestnet, refused).is_none());
    }
}
