use crate::chain::block::Block;
use crate::chain::block::ConsensusData;
use crate::chain::transaction::Transaction;
use crate::chain::ChainConfig;
use crate::primitives::id;
use crate::primitives::id::Idable;
use crate::primitives::{Id, H256};

use serialization::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Encode, Decode)]
pub struct BlockHeader {
    pub(super) prev_block_hash: Option<Id<Block>>,
    pub(super) tx_merkle_root: Option<H256>,
    pub(super) witness_merkle_root: Option<H256>,
    pub(super) time: u32,
    pub(super) consensus_data_inner: ConsensusData,
}

impl BlockHeader {
    pub fn consensus_data(&self) -> &ConsensusData {
        &self.consensus_data_inner
    }

    pub fn block_id(&self) -> Id<Block> {
        Id::new(&id::hash_encoded(self))
    }

    pub fn is_genesis(&self, chain_config: &ChainConfig) -> bool {
        self.prev_block_hash == None && chain_config.genesis_block().get_id() == self.block_id()
    }

    pub fn get_prev_block_id(&self) -> &Option<Id<Block>> {
        &self.prev_block_hash
    }

    pub fn block_time(&self) -> u32 {
        self.time
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockV1 {
    pub(super) header: BlockHeader,
    pub(super) transactions: Vec<Transaction>,
}

impl BlockV1 {
    pub fn tx_merkle_root(&self) -> Option<H256> {
        self.header.tx_merkle_root
    }

    pub fn witness_merkle_root(&self) -> Option<H256> {
        self.header.witness_merkle_root
    }

    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    pub fn update_consensus_data(&mut self, consensus_data: ConsensusData) {
        self.header.consensus_data_inner = consensus_data;
    }

    pub fn consensus_data(&self) -> &ConsensusData {
        self.header.consensus_data()
    }

    pub fn block_time(&self) -> u32 {
        self.header.time
    }

    pub fn transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    pub fn get_prev_block_id(&self) -> &Option<Id<Block>> {
        &self.header.prev_block_hash
    }
}
