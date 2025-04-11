use alloy_eips::{BlockNumberOrTag, eip1898::BlockId};
use anvil_core::eth::EthRequest;
use arc_swap::ArcSwap;
use moka::Expiry;
use moka::future::Cache;
use serde_json::Value;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct ReqRes {
    pub req: EthRequest,
    pub res: Value,
}

/// Represents a cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The actual value stored in the cache
    pub value: ReqRes,
    /// Duration after which this entry should expire
    pub ttl: Duration,
}

impl CacheEntry {
    /// Creates a new cache entry with the given value and TTL
    pub fn new(value: ReqRes, ttl: Duration) -> Self {
        Self { value, ttl }
    }
}

/// An expiry policy that uses the TTL from the cache entry
#[derive(Debug)]
pub struct TtlExpiry;

impl Expiry<String, CacheEntry> for TtlExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &CacheEntry,
        _: Instant,
    ) -> Option<Duration> {
        Some(value.ttl)
    }

    fn expire_after_update(
        &self,
        key: &String,
        value: &CacheEntry,
        updated_at: Instant,
        _: Option<Duration>,
    ) -> Option<Duration> {
        self.expire_after_create(key, value, updated_at)
    }
}

/// A cache implementation with field-level TTL
#[derive(Debug)]
pub struct RpcCache {
    /// The underlying cache implementation
    cache: Cache<String, CacheEntry>,
    /// The block time for this chain
    block_time: Duration,
    /// The latest block number for this chain
    latest_block_number: ArcSwap<u64>,
}

static ONE_YEAR: Duration = Duration::from_secs(31536000);

impl RpcCache {
    /// Creates a new cache with the given maximum capacity and block time
    pub fn new(max_capacity: u64, block_time: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .expire_after(TtlExpiry)
            .build();
        Self {
            cache,
            block_time,
            latest_block_number: ArcSwap::new(Arc::new(0)),
        }
    }

    /// Gets a value from the cache if it exists
    pub async fn get(&self, req: &EthRequest) -> Option<ReqRes> {
        let mut hasher = DefaultHasher::new();
        req.hash(&mut hasher);
        let key = hasher.finish().to_string();
        self.cache.get(&key).await.map(|entry| entry.value)
    }

    /// Inserts a value into the cache with the given TTL
    pub async fn insert(&self, req: &EthRequest, response: &Value, ttl: Duration) {
        let mut hasher = DefaultHasher::new();
        req.hash(&mut hasher);
        let key = hasher.finish().to_string();
        let reqres = ReqRes {
            req: req.clone(),
            res: response.clone(),
        };
        let entry = CacheEntry::new(reqres, ttl);
        self.cache.insert(key, entry).await;
    }

    fn get_ttl_from_block_number_or_tag(
        &self,
        block_number_or_tag: &BlockNumberOrTag,
    ) -> Option<Duration> {
        match block_number_or_tag {
            BlockNumberOrTag::Latest => Some(self.block_time),
            BlockNumberOrTag::Finalized => Some(ONE_YEAR),
            BlockNumberOrTag::Safe => Some(self.block_time), // TODO: can do better here
            BlockNumberOrTag::Earliest => Some(ONE_YEAR),
            BlockNumberOrTag::Pending => None,
            BlockNumberOrTag::Number(number) => {
                let latest_block_number = **self.latest_block_number.load();
                if *number < latest_block_number && latest_block_number - number > 50 {
                    // TODO: can cache block with longer diff a bit longer than the rest. revisit this part.
                    Some(ONE_YEAR)
                } else {
                    Some(self.block_time)
                }
            }
        }
    }

    fn get_ttl_from_block_id(&self, block_id: &BlockId) -> Option<Duration> {
        let block_number_or_tag = match block_id {
            BlockId::Hash(_) => return Some(ONE_YEAR), // can cache hash for a very long time - 1 year in seconds
            BlockId::Number(block_number_or_tag) => block_number_or_tag,
        };
        self.get_ttl_from_block_number_or_tag(block_number_or_tag)
    }

    /// Returns the TTL for a given method if it's cacheable
    pub fn get_ttl(&self, req: &EthRequest) -> Option<Duration> {
        match req {
            // EthRequest::Web3ClientVersion(_) => todo!(), TODO: self-implement
            // EthRequest::Web3Sha3(bytes) => todo!(), TODO: self-implement
            // EthRequest::EthChainId(_) => todo!(), TODO: self-implement
            // EthRequest::EthNetworkId(_) => todo!(), TODO: self-implement
            // EthRequest::NetListening(_) => todo!(),
            EthRequest::EthGasPrice(_) => Some(self.block_time), // TODO: make this configurable
            EthRequest::EthMaxPriorityFeePerGas(_) => Some(self.block_time), // TODO: make this configurable
            EthRequest::EthBlobBaseFee(_) => Some(self.block_time), // TODO: make this configurable
            // EthRequest::EthAccounts(_) => todo!(),
            EthRequest::EthBlockNumber(_) => Some(self.block_time), // TODO: make this configurable
            EthRequest::EthGetBalance(_, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetAccount(_, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetStorageAt(_, _, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetBlockByHash(_, _) => Some(ONE_YEAR),
            EthRequest::EthGetBlockByNumber(block_number_or_tag, _) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetTransactionCount(_, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetTransactionCountByHash(_) => Some(ONE_YEAR),
            EthRequest::EthGetTransactionCountByNumber(block_number_or_tag) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetUnclesCountByHash(_) => Some(ONE_YEAR),
            EthRequest::EthGetUnclesCountByNumber(block_number_or_tag) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetCodeAt(_, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetProof(_, _, block_id) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            // EthRequest::EthSign(address, bytes) => todo!(),
            // EthRequest::PersonalSign(bytes, address) => todo!(),
            // EthRequest::EthSignTransaction(_) => todo!(),
            // EthRequest::EthSignTypedData(address, value) => todo!(),
            // EthRequest::EthSignTypedDataV3(address, value) => todo!(),
            // EthRequest::EthSignTypedDataV4(address, typed_data) => todo!(),
            // EthRequest::EthSendTransaction(_) => todo!(),
            // EthRequest::EthSendRawTransaction(bytes) => todo!(),
            EthRequest::EthCall(_, block_id, _) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            // EthRequest::EthSimulateV1(simulate_payload, block_id) => todo!(),
            // EthRequest::EthCreateAccessList(_, block_id) => todo!(),
            EthRequest::EthEstimateGas(_, block_id, _) => block_id
                .and_then(|block_id| self.get_ttl_from_block_id(&block_id))
                .or(Some(self.block_time)),
            EthRequest::EthGetTransactionByHash(_) => Some(ONE_YEAR),
            EthRequest::EthGetTransactionByBlockHashAndIndex(_, _) => Some(ONE_YEAR),
            EthRequest::EthGetTransactionByBlockNumberAndIndex(block_number_or_tag, _) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetRawTransactionByHash(_) => Some(ONE_YEAR),
            EthRequest::EthGetRawTransactionByBlockHashAndIndex(_, _) => Some(ONE_YEAR),
            EthRequest::EthGetRawTransactionByBlockNumberAndIndex(block_number_or_tag, _) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetTransactionReceipt(_) => {
                // TODO: this actually depends on the transaction itself. sometimes the ttl needs to be aware of the data we're writing.
                // We're currently re-using ttls to determine if we should cache the response - or whether the request could even exist in the cache in the first place. So we need to separate the two, and actually take a look at the data we're writing while determining the final ttl.
                Some(self.block_time)
            }
            EthRequest::EthGetBlockReceipts(block_id) => self.get_ttl_from_block_id(block_id),
            EthRequest::EthGetUncleByBlockHashAndIndex(_, _) => Some(ONE_YEAR),
            EthRequest::EthGetUncleByBlockNumberAndIndex(block_number_or_tag, _) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            EthRequest::EthGetLogs(_) => Some(self.block_time),
            // EthRequest::EthNewFilter(filter) => todo!(),
            EthRequest::EthGetFilterChanges(_) => Some(self.block_time),
            // EthRequest::EthNewBlockFilter(_) => todo!(),
            // EthRequest::EthNewPendingTransactionFilter(_) => todo!(),
            EthRequest::EthGetFilterLogs(_) => Some(self.block_time),
            // EthRequest::EthUninstallFilter(_) => todo!(),
            // EthRequest::EthGetWork(_) => todo!(),
            // EthRequest::EthSubmitWork(fixed_bytes, fixed_bytes1, fixed_bytes2) => todo!(),
            // EthRequest::EthSubmitHashRate(uint, fixed_bytes) => todo!(),
            EthRequest::EthFeeHistory(_, block_number_or_tag, _) => {
                self.get_ttl_from_block_number_or_tag(block_number_or_tag)
            }
            // EthRequest::EthSyncing(_) => todo!(),
            // EthRequest::DebugGetRawTransaction(fixed_bytes) => todo!(),
            // EthRequest::DebugTraceTransaction(fixed_bytes, geth_debug_tracing_options) => todo!(),
            // EthRequest::DebugTraceCall(_, block_id, geth_debug_tracing_call_options) => todo!(),
            // EthRequest::TraceTransaction(fixed_bytes) => todo!(),
            // EthRequest::TraceBlock(block_number_or_tag) => todo!(),
            // EthRequest::TraceFilter(trace_filter) => todo!(),
            // EthRequest::ImpersonateAccount(address) => todo!(),
            // EthRequest::StopImpersonatingAccount(address) => todo!(),
            // EthRequest::AutoImpersonateAccount(_) => todo!(),
            // EthRequest::GetAutoMine(_) => todo!(),
            // EthRequest::Mine(uint, uint1) => todo!(),
            // EthRequest::SetAutomine(_) => todo!(),
            // EthRequest::SetIntervalMining(_) => todo!(),
            // EthRequest::GetIntervalMining(_) => todo!(),
            // EthRequest::DropTransaction(fixed_bytes) => todo!(),
            // EthRequest::DropAllTransactions() => todo!(),
            // EthRequest::Reset(params) => todo!(),
            // EthRequest::SetRpcUrl(_) => todo!(),
            // EthRequest::SetBalance(address, uint) => todo!(),
            // EthRequest::SetCode(address, bytes) => todo!(),
            // EthRequest::SetNonce(address, uint) => todo!(),
            // EthRequest::SetStorageAt(address, uint, fixed_bytes) => todo!(),
            // EthRequest::SetCoinbase(address) => todo!(),
            // EthRequest::SetChainId(_) => todo!(),
            // EthRequest::SetLogging(_) => todo!(),
            // EthRequest::SetMinGasPrice(uint) => todo!(),
            // EthRequest::SetNextBlockBaseFeePerGas(uint) => todo!(),
            // EthRequest::EvmSetTime(uint) => todo!(),
            // EthRequest::DumpState(params) => todo!(),
            // EthRequest::LoadState(bytes) => todo!(),
            // EthRequest::NodeInfo(_) => todo!(),
            // EthRequest::AnvilMetadata(_) => todo!(),
            // EthRequest::EvmSnapshot(_) => todo!(),
            // EthRequest::EvmRevert(uint) => todo!(),
            // EthRequest::EvmIncreaseTime(uint) => todo!(),
            // EthRequest::EvmSetNextBlockTimeStamp(uint) => todo!(),
            // EthRequest::EvmSetBlockGasLimit(uint) => todo!(),
            // EthRequest::EvmSetBlockTimeStampInterval(_) => todo!(),
            // EthRequest::EvmRemoveBlockTimeStampInterval(_) => todo!(),
            // EthRequest::EvmMine(params) => todo!(),
            // EthRequest::EvmMineDetailed(params) => todo!(),
            // EthRequest::EthSendUnsignedTransaction(_) => todo!(),
            // EthRequest::EnableTraces(_) => todo!(),
            // EthRequest::TxPoolStatus(_) => todo!(),
            // EthRequest::TxPoolInspect(_) => todo!(),
            // EthRequest::TxPoolContent(_) => todo!(),
            // EthRequest::ErigonGetHeaderByNumber(block_number_or_tag) => todo!(),
            // EthRequest::OtsGetApiLevel(_) => todo!(),
            // EthRequest::OtsGetInternalOperations(fixed_bytes) => todo!(),
            // EthRequest::OtsHasCode(address, block_number_or_tag) => todo!(),
            // EthRequest::OtsTraceTransaction(fixed_bytes) => todo!(),
            // EthRequest::OtsGetTransactionError(fixed_bytes) => todo!(),
            // EthRequest::OtsGetBlockDetails(block_number_or_tag) => todo!(),
            // EthRequest::OtsGetBlockDetailsByHash(fixed_bytes) => todo!(),
            // EthRequest::OtsGetBlockTransactions(_, _, _) => todo!(),
            // EthRequest::OtsSearchTransactionsBefore(address, _, _) => todo!(),
            // EthRequest::OtsSearchTransactionsAfter(address, _, _) => todo!(),
            // EthRequest::OtsGetTransactionBySenderAndNonce(address, uint) => todo!(),
            // EthRequest::OtsGetContractCreator(address) => todo!(),
            // EthRequest::RemovePoolTransactions(address) => todo!(),
            // EthRequest::Reorg(reorg_options) => todo!(),
            // EthRequest::Rollback(_) => todo!(),
            // EthRequest::WalletGetCapabilities(_) => todo!(),
            // EthRequest::WalletSendTransaction(_) => todo!(),
            // EthRequest::AnvilAddCapability(address) => todo!(),
            // EthRequest::AnvilSetExecutor(_) => todo!(),
            _ => None,
        }
    }
}
