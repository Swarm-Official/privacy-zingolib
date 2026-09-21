//! Indexer identity checks for the fixed SWARM testnet profile.

use zingo_netutils::{Indexer, Status, lightwallet_protocol::BlockId};

use crate::config::{ChainType, SWARM_TESTNET_GENESIS, SWARM_TESTNET_NAME};

use super::{DEFAULT_REQUEST_TIMEOUT, LightClient, error::LightClientError};

const GENESIS_HEIGHT: u64 = 0;

pub(crate) fn check_identity(chain_name: &str, height: u64, hash: &[u8]) -> Result<(), Status> {
    let display_hash = hex::encode(hash.iter().rev().copied().collect::<Vec<_>>());
    if chain_name != SWARM_TESTNET_NAME
        || height != GENESIS_HEIGHT
        || display_hash != SWARM_TESTNET_GENESIS
    {
        Err(Status::failed_precondition(
            "Indexer identity differs from the configured SWARM testnet",
        ))
    } else {
        Ok(())
    }
}

pub(crate) async fn verify_indexer(
    chain: ChainType,
    indexer: &mut impl Indexer,
) -> Result<(), Status> {
    if chain == ChainType::CustomTestnet {
        let info = indexer.get_lightd_info(DEFAULT_REQUEST_TIMEOUT).await?;
        if info.chain_name != SWARM_TESTNET_NAME {
            return Err(Status::failed_precondition(
                "Indexer chain name differs from the configured SWARM testnet",
            ));
        }
        let genesis = indexer
            .get_block(
                BlockId {
                    height: GENESIS_HEIGHT,
                    hash: Vec::new(),
                },
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        check_identity(&info.chain_name, genesis.height, &genesis.hash)?;
    }
    Ok(())
}

impl LightClient {
    /// Verifies the connected indexer against the selected chain profile.
    pub async fn verify_network(&self) -> Result<(), LightClientError> {
        let mut indexer = self.require_indexer()?.clone();
        verify_indexer(self.chain_type(), &mut indexer).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genesis_bytes() -> Vec<u8> {
        let mut hash = hex::decode(SWARM_TESTNET_GENESIS).unwrap();
        hash.reverse();
        hash
    }

    #[test]
    fn swarm_identity_requires_name_height_and_wire_hash() {
        let hash = genesis_bytes();
        assert!(check_identity(SWARM_TESTNET_NAME, GENESIS_HEIGHT, &hash).is_ok());
        assert!(check_identity("test", GENESIS_HEIGHT, &hash).is_err());
        assert!(
            check_identity(
                SWARM_TESTNET_NAME,
                u64::from(crate::config::SWARM_TESTNET_BIRTHDAY),
                &hash
            )
            .is_err()
        );
        assert!(check_identity(SWARM_TESTNET_NAME, GENESIS_HEIGHT, &[]).is_err());
        let display_bytes = hex::decode(SWARM_TESTNET_GENESIS).unwrap();
        assert!(check_identity(SWARM_TESTNET_NAME, GENESIS_HEIGHT, &display_bytes).is_err());
    }

    #[tokio::test]
    async fn swarm_client_rejects_a_different_chain_before_connecting_or_syncing() {
        use crate::config::{ClientConfig, SWARM_TESTNET_BIRTHDAY, WalletConfig};
        use crate::testutils::mock_indexer::MockNet;
        use crate::wallet::WalletSettings;

        let network = MockNet::launch().await;
        let directory = tempfile::tempdir().unwrap();
        let config = ClientConfig::builder()
            .set_chain_type(ChainType::CustomTestnet)
            .set_wallet_dir(directory.path().to_path_buf())
            .set_wallet_config(WalletConfig::NewSeed {
                no_of_accounts: std::num::NonZeroU32::MIN,
                chain_height: SWARM_TESTNET_BIRTHDAY,
                wallet_settings: WalletSettings::default(),
            })
            .build()
            .unwrap();
        let mut client = LightClient::new(config, false).await.unwrap();
        assert!(client.set_indexer_uri(network.indexer_uri()).await.is_err());
        assert!(client.indexer_uri().is_none());
        client.set_indexer_uri_lazy(network.indexer_uri()).unwrap();
        assert!(matches!(
            client.sync().await,
            Err(LightClientError::IndexerError(_))
        ));
        assert!(matches!(
            client.send_stored_proposal(false).await,
            Err(LightClientError::IndexerError(_))
        ));
    }

    #[tokio::test]
    #[ignore = "requires SWARM_TESTNET_INDEXER pointing to the project indexer"]
    async fn swarm_live_indexer_identity_and_sync() {
        use crate::config::{ClientConfig, SWARM_TESTNET_BIRTHDAY, WalletConfig};
        use crate::wallet::WalletSettings;

        let uri = std::env::var("SWARM_TESTNET_INDEXER")
            .unwrap()
            .parse()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let config = ClientConfig::builder()
            .set_chain_type(ChainType::CustomTestnet)
            .set_indexer_uri(uri)
            .set_wallet_dir(directory.path().to_path_buf())
            .set_wallet_config(WalletConfig::NewSeed {
                no_of_accounts: std::num::NonZeroU32::MIN,
                chain_height: SWARM_TESTNET_BIRTHDAY,
                wallet_settings: WalletSettings::default(),
            })
            .build()
            .unwrap();
        let mut client = LightClient::new(config, false).await.unwrap();
        client.verify_network().await.unwrap();
        client.sync_and_await().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires SWARM_OTHER_INDEXER pointing to a different network"]
    async fn swarm_live_rejects_other_network() {
        use crate::config::ClientConfig;

        let uri = std::env::var("SWARM_OTHER_INDEXER")
            .unwrap()
            .parse()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let config = ClientConfig::builder()
            .set_chain_type(ChainType::CustomTestnet)
            .set_indexer_uri(uri)
            .set_wallet_dir(directory.path().to_path_buf())
            .build()
            .unwrap();
        assert!(matches!(
            LightClient::new(config, false).await,
            Err(LightClientError::IndexerError(_))
        ));
    }
}
