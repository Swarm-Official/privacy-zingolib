use zcash_address::unified::{Address, Encoding};
use zcash_keys::keys::{UnifiedAddressRequest, UnifiedSpendingKey};
use zcash_protocol::consensus::{NetworkType, Parameters};
use zingolib::config::ChainType;
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
