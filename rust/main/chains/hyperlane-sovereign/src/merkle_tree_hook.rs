use async_trait::async_trait;
use core::ops::RangeInclusive;
use hyperlane_core::{
    ChainResult, CheckpointAtBlock, ContractLocator, HyperlaneChain, HyperlaneContract,
    HyperlaneDomain, HyperlaneProvider, IncrementalMerkleAtBlock, Indexed, Indexer, LogMeta,
    MerkleTreeHook, MerkleTreeInsertion, ReorgPeriod, SequenceAwareIndexer, H256, H512,
};
use serde::Deserialize;

use crate::indexer::SovIndexer;
use crate::types::TxEvent;
use crate::{ConnectionConf, Signer, SovereignProvider};

/// Struct that retrieves event data for a Sovereign Mailbox contract.
#[derive(Debug, Clone)]
pub struct SovereignMerkleTreeHookIndexer {
    provider: SovereignProvider,
}

impl SovereignMerkleTreeHookIndexer {
    pub async fn new(
        conf: ConnectionConf,
        locator: ContractLocator<'_>,
        signer: Option<Signer>,
    ) -> ChainResult<Self> {
        let provider = SovereignProvider::new(locator.domain.clone(), &conf, signer).await?;
        Ok(SovereignMerkleTreeHookIndexer { provider })
    }
}

#[async_trait]
impl crate::indexer::SovIndexer<MerkleTreeInsertion> for SovereignMerkleTreeHookIndexer {
    const EVENT_KEY: &'static str = "MerkleTreeHook/InsertedIntoTree";

    fn provider(&self) -> &SovereignProvider {
        &self.provider
    }

    async fn latest_sequence(&self, at_slot: Option<u64>) -> ChainResult<Option<u32>> {
        let sequence = self.provider().tree_count(at_slot).await?;

        Ok(Some(sequence))
    }

    fn decode_event(&self, event: &TxEvent) -> ChainResult<MerkleTreeInsertion> {
        let parsed_event: InsertedIntoTreeEvent = serde_json::from_value(event.value.clone())?;

        let merkle_insertion = MerkleTreeInsertion::new(
            parsed_event.inserted_into_tree.index,
            parsed_event.inserted_into_tree.id,
        );

        Ok(merkle_insertion)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct InsertedIntoTreeEvent {
    inserted_into_tree: TreeEventBody,
}

#[derive(Clone, Debug, Deserialize)]
struct TreeEventBody {
    id: H256,
    index: u32,
}

#[async_trait]
impl SequenceAwareIndexer<MerkleTreeInsertion> for SovereignMerkleTreeHookIndexer {
    async fn latest_sequence_count_and_tip(&self) -> ChainResult<(Option<u32>, u32)> {
        <Self as SovIndexer<MerkleTreeInsertion>>::latest_sequence_count_and_tip(self).await
    }
}
impl SovereignMerkleTreeHookIndexer {
    /// Search in the given range for the lowest slot with the given
    /// tree-count.
    ///
    /// It returns the slot and the count just before this slot to
    /// handle multiple events in a slot.
    async fn binary_search(
        &self,
        mut left: u32,
        mut right: u32,
        tree_count: u32,
    ) -> ChainResult<(u32, u32)> {
        let mut left_cnt = 0;
        while left < right {
            let mid = left + (right - left) / 2;
            let cnt = self.provider().tree_count(Some(mid as u64)).await?;

            if cnt < tree_count {
                left = mid + 1;
                left_cnt = left_cnt.max(cnt);
            } else {
                right = mid;
            }
        }
        Ok((left, left_cnt))
    }
}

#[async_trait]
impl Indexer<MerkleTreeInsertion> for SovereignMerkleTreeHookIndexer {
    async fn fetch_logs_in_range(
        &self,
        range: RangeInclusive<u32>,
    ) -> ChainResult<Vec<(Indexed<MerkleTreeInsertion>, LogMeta)>> {
        let left = *range.start();
        let mut right = *range.end();

        // check the boundaries first - the ranges are overlapping
        // between different calls so no need to subtract one.
        let (start, mut end): (_, _) = tokio::try_join!(
            self.provider().tree_count(Some(left as u64)),
            self.provider().tree_count(Some(right as u64)),
        )?;

        let mut res = vec![];
        while start < end {
            // use binary search to find the slot start is incremented
            let (slot_num, next_count) = self.binary_search(left, right, end).await?;
            right = slot_num;
            end = next_count;

            // fetch the metadata
            let (header, events) = tokio::try_join!(
                self.provider().get_header_for_slot(slot_num.into()),
                self.provider()
                    .get_events_for_slot(slot_num.into(), Self::EVENT_KEY)
            )?;

            res.extend(
                events
                    .into_iter()
                    // just in case as there is only a prefix check in the rollup
                    .filter(|ev| ev.key == Self::EVENT_KEY)
                    .enumerate()
                    .map(move |(nr, ev)| {
                        // TODO the tx_number is not consistent with the one from process_tx
                        self.process_event(nr as u64, &ev, slot_num as u64, header.hash)
                    })
                    .flatten()
                    .collect::<Vec<_>>(),
            );
        }
        Ok(res)
    }

    async fn get_finalized_block_number(&self) -> ChainResult<u32> {
        <Self as SovIndexer<MerkleTreeInsertion>>::get_finalized_block_number(self).await
    }

    async fn fetch_logs_by_tx_hash(
        &self,
        tx_hash: H512,
    ) -> ChainResult<Vec<(Indexed<MerkleTreeInsertion>, LogMeta)>> {
        <Self as SovIndexer<MerkleTreeInsertion>>::fetch_logs_by_tx_hash(self, tx_hash).await
    }
}

/// A struct for the Merkle Tree Hook on the Sovereign chain
#[derive(Debug)]
pub struct SovereignMerkleTreeHook {
    domain: HyperlaneDomain,
    provider: SovereignProvider,
}

impl SovereignMerkleTreeHook {
    /// Create a new `SovereignMerkleTreeHook`.
    pub async fn new(
        conf: &ConnectionConf,
        locator: ContractLocator<'_>,
        signer: Option<Signer>,
    ) -> ChainResult<Self> {
        let provider =
            SovereignProvider::new(locator.domain.clone(), &conf.clone(), signer).await?;
        Ok(SovereignMerkleTreeHook {
            domain: locator.domain.clone(),
            provider,
        })
    }
}

impl HyperlaneChain for SovereignMerkleTreeHook {
    fn domain(&self) -> &HyperlaneDomain {
        &self.domain
    }

    fn provider(&self) -> Box<dyn HyperlaneProvider> {
        Box::new(self.provider.clone())
    }
}

/// This divereges from hyperlane protocol as merkle tree hook is a built-in
/// module in sovereign and doesn't have own address.
impl HyperlaneContract for SovereignMerkleTreeHook {
    fn address(&self) -> H256 {
        H256::default()
    }
}

#[async_trait]
impl MerkleTreeHook for SovereignMerkleTreeHook {
    async fn tree(&self, _reorg_period: &ReorgPeriod) -> ChainResult<IncrementalMerkleAtBlock> {
        let slot = self.provider.get_finalized_slot().await?;
        let tree = self.provider.tree(Some(slot)).await?;

        Ok(tree)
    }

    async fn count(&self, _reorg_period: &ReorgPeriod) -> ChainResult<u32> {
        let slot = self.provider.get_finalized_slot().await?;
        let tree = self.provider.tree(Some(slot)).await?;
        Ok(u32::try_from(tree.count)
            .map_err(|e| custom_err!("Tree count overflowed u32: {e:?}"))?)
    }

    async fn latest_checkpoint(
        &self,
        _reorg_period: &ReorgPeriod,
    ) -> ChainResult<CheckpointAtBlock> {
        let slot = self.provider.get_finalized_slot().await?;
        let checkpoint = self
            .provider
            .latest_checkpoint(Some(slot), self.domain.id())
            .await?;

        Ok(checkpoint)
    }

    /// Get the latest checkpoint at a specific block height.
    async fn latest_checkpoint_at_block(&self, height: u64) -> ChainResult<CheckpointAtBlock> {
        let checkpoint = self
            .provider
            .latest_checkpoint(Some(height), self.domain.id())
            .await?;

        Ok(checkpoint)
    }
}
