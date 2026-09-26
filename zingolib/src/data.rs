//! This is a mod for data structs that will be used across all sections of zingolib.

pub mod proposal;

/// Return type for fns that poll the status of task handles.
pub enum PollReport<T, E> {
    /// Task has not been launched.
    NoHandle,
    /// Task is not complete.
    NotReady,
    /// Task has completed successfully or failed.
    Ready(Result<T, E>),
}

/// The connected Indexer's server diagnostics, as reported by its
/// `get_lightd_info` RPC. Returned by `LightClient::info`.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// The server's software version.
    pub version: String,
    /// The git commit the server was built from.
    pub git_commit: String,
    /// The URI this client is connected to.
    pub server_uri: http::Uri,
    /// The server's vendor string.
    pub vendor: String,
    /// Whether the server supports transparent addresses.
    pub taddr_support: bool,
    /// The chain name ("main", "test", "regtest").
    pub chain_name: String,
    /// The Sapling activation height of the chain.
    pub sapling_activation_height: u64,
    /// The consensus branch ID the server reports.
    pub consensus_branch_id: String,
    /// The server's view of the chain tip height.
    pub latest_block_height: u64,
    /// The height-zero block hash of the chain the server indexes, as 64
    /// lowercase hexadecimal characters in display order, or empty when the
    /// server did not state one.
    ///
    /// [`ServerInfo::chain_name`] names a chain but cannot prove it: two chains
    /// built from the same software answer the same label, so a wallet that
    /// trusted the label alone could sync a SWARM production wallet against a
    /// rehearsal chain and write its state back over the right one. The genesis
    /// is the chain's identity, so a wallet can compare the hash it was built
    /// for against the hash the server serves.
    ///
    /// Empty means "this server did not say", never "no genesis": the wire
    /// field (`LightdInfo.genesisHash`, number 19) is appended rather than
    /// inserted, so a server built before it sends nothing, and that decodes as
    /// the empty string. Regtest sends empty for real, since its genesis is
    /// whatever the local validator made.
    pub genesis_hash: String,
}

impl From<ServerInfo> for json::JsonValue {
    fn from(info: ServerInfo) -> Self {
        json::object! {
            "version" => info.version,
            "git_commit" => info.git_commit,
            "server_uri" => info.server_uri.to_string(),
            "vendor" => info.vendor,
            "taddr_support" => info.taddr_support,
            "chain_name" => info.chain_name,
            "sapling_activation_height" => info.sapling_activation_height,
            "consensus_branch_id" => info.consensus_branch_id,
            "latest_block_height" => info.latest_block_height,
            "genesis_hash" => info.genesis_hash
        }
    }
}

/// transforming data related to the destination of a send.
pub mod receivers {
    use zcash_address::ZcashAddress;
    use zcash_client_backend::zip321::Payment;
    use zcash_client_backend::zip321::PaymentError;
    use zcash_client_backend::zip321::TransactionRequest;
    use zcash_client_backend::zip321::Zip321Error;
    use zcash_protocol::memo::MemoBytes;
    use zcash_protocol::value::Zatoshis;

    /// A list of Receivers
    pub type Receivers = Vec<Receiver>;

    /// The superficial representation of the the consumer's intended receiver
    #[derive(Clone, Debug, PartialEq)]
    pub struct Receiver {
        pub recipient_address: ZcashAddress,
        pub amount: Zatoshis,
        pub memo: Option<MemoBytes>,
    }
    impl Receiver {
        /// Create a new Receiver
        pub(crate) fn new(
            recipient_address: ZcashAddress,
            amount: Zatoshis,
            memo: Option<MemoBytes>,
        ) -> Self {
            Self {
                recipient_address,
                amount,
                memo,
            }
        }
    }
    impl TryFrom<Receiver> for Payment {
        type Error = PaymentError;

        fn try_from(receiver: Receiver) -> Result<Self, Self::Error> {
            Payment::new(
                receiver.recipient_address,
                Some(receiver.amount),
                receiver.memo,
                None,
                None,
                vec![],
            )
        }
    }

    /// Creates a [`zcash_client_backend::zip321::TransactionRequest`] from receivers.
    /// Note this fn is called to calculate the `spendable_shielded` balance
    /// shielding and TEX should be handled mutually exclusively
    pub fn transaction_request_from_receivers(
        receivers: Receivers,
    ) -> Result<TransactionRequest, Zip321Error> {
        let payments = receivers
            .into_iter()
            .enumerate()
            .map(|(i, receiver)| {
                Payment::try_from(receiver).map_err(|e| match e {
                    PaymentError::TransparentMemo => Zip321Error::TransparentMemo(i),
                    PaymentError::ZeroValuedTransparentOutput => {
                        Zip321Error::ZeroValuedTransparentOutput(i)
                    }
                })
            })
            .collect::<Result<Vec<_>, Zip321Error>>()?;

        TransactionRequest::new(payments)
    }
}

#[cfg(test)]
mod tests {
    //! The wire contract behind [`ServerInfo::genesis_hash`].
    //!
    //! The field is only worth reading if the pinned protocol crate actually
    //! carries `LightdInfo.genesisHash` at number 19, and if the number was
    //! appended rather than inserted. Both are properties of the dependency
    //! pin, not of this crate's code, so they are asserted here: a pin moved
    //! back to a rev without the field stops the build, and a pin moved to a
    //! rev that renumbered it fails these tests.

    use prost::Message as _;
    use zingo_netutils::lightwallet_protocol::LightdInfo;

    const GENESIS: &str = "01c34428b9e67cdd8345e0b365aaa37dd8d2d65d3869e0e5d77d567f2c39afdd";

    /// The hash the server states survives a round trip on the wire.
    #[test]
    fn genesis_hash_round_trips_on_the_wire() {
        let served = LightdInfo {
            chain_name: "swarm-mainnet".to_string(),
            genesis_hash: GENESIS.to_string(),
            ..Default::default()
        };
        let mut bytes = Vec::new();
        served.encode(&mut bytes).expect("a message encodes");
        let read = LightdInfo::decode(bytes.as_slice()).expect("the message decodes");
        assert_eq!(read.genesis_hash, GENESIS);
    }

    /// Field 19, appended and never inserted: it is the last tag on the wire,
    /// and every field below it keeps its number. A message from a server that
    /// predates the field therefore decodes with an empty `genesis_hash`, which
    /// means "this server did not say" and never "no genesis".
    #[test]
    fn a_reply_without_field_19_decodes_as_not_stated() {
        let older_server = LightdInfo {
            chain_name: "main".to_string(),
            ..Default::default()
        };
        let mut bytes = Vec::new();
        older_server.encode(&mut bytes).expect("a message encodes");
        // Tag byte for field 19, wire type 2 (length-delimited): (19 << 3) | 2
        // = 154, which needs a two-byte varint key. Its absence is what makes
        // the reply an older server's.
        assert!(
            !bytes.windows(2).any(|pair| pair == [0x9a, 0x01]),
            "an unset genesis must put nothing on the wire: {bytes:?}"
        );
        let read = LightdInfo::decode(bytes.as_slice()).expect("the message decodes");
        assert_eq!(read.genesis_hash, "");
    }

    /// The JSON `LightClient::info` hands a wallet carries the genesis beside
    /// the chain label, so the wallet can check the chain rather than trust a
    /// name the server chose.
    #[test]
    fn server_info_json_carries_the_genesis() {
        let info = super::ServerInfo {
            version: "1".to_string(),
            git_commit: "abc".to_string(),
            server_uri: "https://lwd-main.swarm.green:8443"
                .parse()
                .expect("a valid uri"),
            vendor: "swarm".to_string(),
            taddr_support: true,
            chain_name: "swarm-mainnet".to_string(),
            sapling_activation_height: 1,
            consensus_branch_id: "53574d31".to_string(),
            latest_block_height: 42,
            genesis_hash: GENESIS.to_string(),
        };
        let rendered = json::JsonValue::from(info);
        assert_eq!(rendered["genesis_hash"].as_str(), Some(GENESIS));
        assert_eq!(rendered["chain_name"].as_str(), Some("swarm-mainnet"));
    }
}
