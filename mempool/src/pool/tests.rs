// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use chainstate::make_chainstate;
use chainstate::ChainstateConfig;
use common::chain::block::timestamp::BlockTimestamp;
use common::chain::block::BlockReward;
use common::chain::block::ConsensusData;
use common::chain::config::ChainConfig;
use common::chain::signature::inputsig::InputWitness;
use common::chain::transaction::{Destination, TxInput, TxOutput};
use common::chain::OutPointSourceId;
use common::chain::OutputPurpose;
use common::primitives::H256;
use core::panic;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod utils;

use self::utils::*;

const DUMMY_WITNESS_MSG: &[u8] = b"dummy_witness_msg";

#[test]
fn dummy_size() {
    logging::init_logging::<&str>(None);
    log::debug!("1, 1: {}", estimate_tx_size(1, 1));
    log::debug!("1, 2: {}", estimate_tx_size(1, 2));
    log::debug!("1, 400: {}", estimate_tx_size(1, 400));
}

#[tokio::test]
async fn real_size() -> anyhow::Result<()> {
    let mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_inputs(1)
        .with_num_outputs(400)
        .generate_tx(&mempool)
        .await?;
    log::debug!("real size of tx {}", tx.encoded_size());
    Ok(())
}

fn make_outpoint(tx_id: &Id<Transaction>, outpoint_index: u32) -> OutPoint {
    let outpoint_source_id = OutPointSourceId::Transaction(*tx_id);
    OutPoint::new(outpoint_source_id, outpoint_index)
}

fn valued_outpoint(
    tx_id: &Id<Transaction>,
    outpoint_index: u32,
    output: &TxOutput,
) -> ValuedOutPoint {
    let outpoint = make_outpoint(tx_id, outpoint_index);
    let value = match output.value() {
        OutputValue::Coin(coin) => *coin,
    };
    ValuedOutPoint { outpoint, value }
}

impl TxMempoolEntry {
    fn outpoints_created(&self) -> BTreeSet<OutPoint> {
        let id = self.tx.get_id();
        std::iter::repeat(id)
            .zip(self.tx.outputs().iter().enumerate())
            .map(|(id, (index, _output))| make_outpoint(&id, index as u32))
            .collect()
    }
}

impl MempoolStore {
    fn unconfirmed_outpoints(&self) -> BTreeSet<OutPoint> {
        self.txs_by_id
            .values()
            .cloned()
            .flat_map(|entry| entry.outpoints_created())
            .collect()
    }
}

impl<T, M> Mempool<T, M>
where
    T: GetTime + Send + std::marker::Sync,
    M: GetMemoryUsage + Send + std::marker::Sync,
{
    async fn available_outpoints(
        &self,
        allow_double_spend: bool,
    ) -> anyhow::Result<BTreeSet<OutPoint>> {
        let mut available = self
            .store
            .unconfirmed_outpoints()
            .into_iter()
            .chain(
                self.chainstate_handle
                    .call(|this| this.confirmed_outpoints().expect("confirmed_outpoints"))
                    .await?
                    .into_iter(),
            )
            .collect::<BTreeSet<_>>();
        if !allow_double_spend {
            available.retain(|outpoint| !self.store.spender_txs.contains_key(outpoint));
        }
        eprintln!("available_outpoints are {:?}", available);
        Ok(available)
    }

    async fn get_input_value(&self, input: &TxInput) -> anyhow::Result<Amount> {
        let allow_double_spend = true;
        let outpoint = self
            .available_outpoints(allow_double_spend)
            .await?
            .into_iter()
            .find(|outpoint| outpoint == input.outpoint())
            .ok_or_else(|| anyhow::anyhow!("No such unconfirmed output"))?;
        Ok(self
            .chainstate_handle
            .call(move |this| this.get_outpoint_value(&outpoint).expect("get_outpoint_value"))
            .await
            .expect("call success"))
    }

    fn get_minimum_rolling_fee(&self) -> FeeRate {
        self.rolling_fee_rate.read().rolling_minimum_fee_rate
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChainStateMock {
    confirmed_txs: HashMap<H256, Transaction>,
    available_outpoints: BTreeSet<OutPoint>,
}

use chainstate::PropertyQueryError;
use chainstate::{BlockSource, ChainstateError, ChainstateEvent, Locator};
use common::{
    chain::{
        block::{Block, BlockHeader, GenBlock},
        Transaction,
    },
    primitives::{BlockHeight, Id},
};
impl ChainstateInterface for ChainStateMock {
    fn subscribe_to_events(&mut self, _handler: Arc<dyn Fn(ChainstateEvent) + Send + Sync>) {
        unimplemented!()
    }
    fn process_block(
        &mut self,
        _block: Block,
        _source: BlockSource,
    ) -> Result<(), ChainstateError> {
        unimplemented!()
    }
    fn preliminary_block_check(&self, _block: Block) -> Result<Block, ChainstateError> {
        unimplemented!()
    }
    fn get_best_block_id(&self) -> Result<Id<GenBlock>, ChainstateError> {
        unimplemented!()
    }
    fn is_block_in_main_chain(&self, _block_id: &Id<Block>) -> Result<bool, ChainstateError> {
        unimplemented!()
    }
    fn get_block_height_in_main_chain(
        &self,
        _block_id: &Id<GenBlock>,
    ) -> Result<Option<BlockHeight>, ChainstateError> {
        unimplemented!()
    }
    fn get_best_block_height(&self) -> Result<BlockHeight, ChainstateError> {
        unimplemented!()
    }
    fn get_block_id_from_height(
        &self,
        _height: &BlockHeight,
    ) -> Result<Option<Id<GenBlock>>, ChainstateError> {
        unimplemented!()
    }
    fn get_block(&self, _block_id: Id<Block>) -> Result<Option<Block>, ChainstateError> {
        unimplemented!()
    }

    /// Returns a list of block headers whose heights distances increase exponentially starting
    /// from the current tip.
    ///
    /// This returns a relatively short sequence even for a long chain. Such sequence can be used
    /// to quickly find a common ancestor between different chains.
    fn get_locator(&self) -> Result<Locator, ChainstateError> {
        unimplemented!()
    }

    /// Returns a list of block headers starting from the last locator's block that is in the main
    /// chain.
    ///
    /// The number of returned headers is limited by the `HEADER_LIMIT` constant. The genesis block
    /// header is returned in case there is no common ancestor with a better block height.
    fn get_headers(&self, _locator: Locator) -> Result<Vec<BlockHeader>, ChainstateError> {
        unimplemented!()
    }

    /// Removes all headers that are already known to the chain from the given vector.
    fn filter_already_existing_blocks(
        &self,
        _headers: Vec<BlockHeader>,
    ) -> Result<Vec<BlockHeader>, ChainstateError> {
        unimplemented!()
    }

    fn available_inputs(&self, tx: &Transaction) -> Vec<TxInput> {
        tx.inputs()
            .iter()
            .filter(|input| self.available_outpoints.contains(input.outpoint()))
            .cloned()
            .collect()
    }

    fn get_outpoint_value(
        &self,
        outpoint: &OutPoint,
    ) -> Result<Amount, chainstate::ChainstateError> {
        self.confirmed_txs
            .get(&outpoint.tx_id().get_tx_id().expect("Not coinbase").get())
            .ok_or(chainstate::ChainstateError::FailedToReadProperty(
                PropertyQueryError::TxNotFound,
            ))
            .and_then(|tx| {
                tx.outputs()
                    .get(outpoint.output_index() as usize)
                    .ok_or(chainstate::ChainstateError::FailedToReadProperty(
                        PropertyQueryError::OutpointNotFound,
                    ))
                    .map(|output| match output.value() {
                        OutputValue::Coin(coin) => *coin,
                    })
            })
    }

    fn confirmed_outpoints(&self) -> Result<BTreeSet<OutPoint>, ChainstateError> {
        Ok(self
            .available_outpoints
            .iter()
            .map(|outpoint| {
                let tx_id = outpoint
                    .tx_id()
                    .get_tx_id()
                    .cloned()
                    .expect("Outpoints in these tests are created from TXs");
                let index = outpoint.output_index();
                make_outpoint(&tx_id, index)
            })
            .collect())
    }
}

/* FIXME The second call in the following flow sometimes returns TransactionAlreadyInMempool
let tx1 = TxGenerator::new(&mempool).generate_tx()?;
mempool.add_transaction(tx1)?;

let tx2 = TxGenerator::new(&mempool).generate_tx()?;
mempool.add_transaction(tx2)?;
*/
struct TxGenerator {
    coin_pool: BTreeSet<ValuedOutPoint>,
    num_inputs: usize,
    num_outputs: usize,
    tx_fee: Option<Amount>,
    replaceable: bool,
    allow_double_spend: bool,
}

impl TxGenerator {
    fn with_num_inputs(mut self, num_inputs: usize) -> Self {
        self.num_inputs = num_inputs;
        self
    }

    fn with_num_outputs(mut self, num_outputs: usize) -> Self {
        self.num_outputs = num_outputs;
        self
    }

    fn replaceable(mut self) -> Self {
        self.replaceable = true;
        self
    }

    fn with_fee(mut self, fee: Amount) -> Self {
        self.tx_fee = Some(fee);
        self
    }

    fn new() -> Self {
        Self {
            coin_pool: BTreeSet::new(),
            num_inputs: 1,
            num_outputs: 1,
            tx_fee: None,
            replaceable: false,
            allow_double_spend: false,
        }
    }

    async fn generate_tx<
        T: GetTime + Send + std::marker::Sync,
        M: GetMemoryUsage + Send + std::marker::Sync,
    >(
        &mut self,
        mempool: &Mempool<T, M>,
    ) -> anyhow::Result<Transaction> {
        for outpoint in mempool.available_outpoints(self.allow_double_spend).await? {
            self.coin_pool.insert(ValuedOutPoint {
                outpoint: outpoint.clone(),
                value: mempool
                    .chainstate_handle
                    .call(move |this| this.get_outpoint_value(&outpoint))
                    .await??,
            });
        }
        let fee = if let Some(tx_fee) = self.tx_fee {
            tx_fee
        } else {
            Amount::from_atoms(get_relay_fee_from_tx_size(estimate_tx_size(
                self.num_inputs,
                self.num_outputs,
            )))
        };
        log::debug!(
            "Trying to build a tx with {} inputs, {} outputs, and a fee of {:?}",
            self.num_inputs,
            self.num_outputs,
            fee
        );
        let valued_inputs = self.generate_tx_inputs(fee)?;
        let outputs = self.generate_tx_outputs(&valued_inputs, fee)?;
        let locktime = 0;
        let flags = if self.replaceable { 1 } else { 0 };
        let (inputs, _): (Vec<TxInput>, Vec<Amount>) = valued_inputs.into_iter().unzip();
        let spent_outpoints = inputs.iter().map(|input| input.outpoint()).collect::<BTreeSet<_>>();
        self.coin_pool
            .retain(|outpoint| !spent_outpoints.iter().any(|spent| **spent == outpoint.outpoint));
        let tx = Transaction::new(flags, inputs, outputs.clone(), locktime)
            .map_err(anyhow::Error::from)?;
        self.coin_pool.extend(
            std::iter::repeat(tx.get_id())
                .zip(outputs.iter().enumerate())
                .map(|(id, (i, output))| valued_outpoint(&id, i as u32, output)),
        );

        Ok(tx)
    }

    fn generate_tx_inputs(&mut self, fee: Amount) -> anyhow::Result<Vec<(TxInput, Amount)>> {
        Ok(self
            .get_unspent_outpoints(self.num_inputs, fee)?
            .iter()
            .map(|valued_outpoint| {
                let ValuedOutPoint { outpoint, value } = valued_outpoint;
                (
                    TxInput::new(
                        outpoint.tx_id(),
                        outpoint.output_index(),
                        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
                    ),
                    *value,
                )
            })
            .collect())
    }

    fn generate_tx_outputs(
        &self,
        inputs: &[(TxInput, Amount)],
        tx_fee: Amount,
    ) -> anyhow::Result<Vec<TxOutput>> {
        if self.num_outputs == 0 {
            return Ok(vec![]);
        }

        let inputs: Vec<_> = inputs.to_owned();
        let (inputs, values): (Vec<TxInput>, Vec<Amount>) = inputs.into_iter().unzip();
        if inputs.is_empty() {
            return Ok(vec![]);
        }
        let sum_of_inputs =
            values.into_iter().sum::<Option<_>>().expect("Overflow in sum of input values");

        let total_to_spend = (sum_of_inputs - tx_fee).ok_or_else(||anyhow::anyhow!(
                "generate_tx_outputs: underflow computing total_to_spend - sum_of_inputs = {:?}, fee = {:?}", sum_of_inputs, tx_fee
            ))?;

        let value = (sum_of_inputs / u128::try_from(self.num_outputs).expect("conversion"))
            .expect("not dividing by zero");

        let mut left_to_spend = total_to_spend;
        let mut outputs = Vec::new();

        for _ in 0..self.num_outputs - 1 {
            outputs.push(TxOutput::new(
                OutputValue::Coin(value),
                OutputPurpose::Transfer(Destination::AnyoneCanSpend),
            ));
            left_to_spend = (left_to_spend - value).expect("subtraction failed");
        }

        outputs.push(TxOutput::new(
            OutputValue::Coin(left_to_spend),
            OutputPurpose::Transfer(Destination::AnyoneCanSpend),
        ));
        Ok(outputs)
    }

    fn get_unspent_outpoints(
        &self,
        num_outputs: usize,
        fee: Amount,
    ) -> anyhow::Result<Vec<ValuedOutPoint>> {
        log::debug!(
            "get_unspent_outpoints: num_outputs: {}, fee: {:?}",
            num_outputs,
            fee
        );
        let num_available_outpoints = self.coin_pool.len();
        let outpoints: Vec<_> = (num_available_outpoints >= num_outputs)
            .then(|| self.coin_pool.iter().take(num_outputs).cloned().collect())
            .ok_or_else(|| anyhow::anyhow!("no outpoints left"))?;
        let sum_of_outputs = outpoints
            .iter()
            .map(|valued_outpoint| valued_outpoint.value)
            .sum::<Option<_>>()
            .expect("sum error");
        if fee > sum_of_outputs {
            Err(anyhow::Error::msg(
                "get_unspent_outpoints:: fee is {:?} but sum of outputs is {:?}",
            ))
        } else {
            Ok(outpoints)
        }
    }
}

fn get_relay_fee_from_tx_size(tx_size: usize) -> u128 {
    u128::try_from(tx_size * RELAY_FEE_PER_BYTE).expect("relay fee overflow")
}

#[tokio::test]
async fn add_single_tx() -> anyhow::Result<()> {
    let mut mempool = setup().await;

    let outpoint_source_id = mempool.chain_config.genesis_block_id().into();

    let flags = 0;
    let locktime = 0;
    let input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let relay_fee = Amount::from_atoms(get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE));
    eprintln!("1");
    let tx = tx_spend_input(&mempool, input, relay_fee, flags, locktime).await?;
    eprintln!("2");

    let tx_clone = tx.clone();
    let tx_id = tx.get_id();
    mempool.add_transaction(tx).await?;
    assert!(mempool.contains_transaction(&tx_id));
    let all_txs = mempool.get_all();
    assert_eq!(all_txs, vec![&tx_clone]);
    mempool.drop_transaction(&tx_id);
    assert!(!mempool.contains_transaction(&tx_id));
    let all_txs = mempool.get_all();
    assert_eq!(all_txs, Vec::<&Transaction>::new());
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn txs_sorted() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let mut tx_generator = TxGenerator::new();
    let target_txs = 10;

    for _ in 0..target_txs {
        match tx_generator.generate_tx(&mempool).await {
            Ok(tx) => {
                mempool.add_transaction(tx.clone()).await?;
            }
            _ => break,
        }
    }

    let mut fees = Vec::new();
    for tx in mempool.get_all() {
        fees.push(mempool.try_get_fee(tx).await?)
    }
    let mut fees_sorted = fees.clone();
    fees_sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(fees, fees_sorted);
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn tx_no_inputs() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_inputs(0)
        .with_fee(Amount::from_atoms(0))
        .generate_tx(&mempool)
        .await
        .expect("generate_tx failed");
    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(TxValidationError::NoInputs))
    ));
    mempool.store.assert_valid();
    Ok(())
}

// TODO this is copy-pasted from libp2p's test utils. This function should be extracted to an
// external crate to avoid code duplication
pub async fn start_chainstate(
    chain_config: Arc<ChainConfig>,
) -> subsystem::Handle<Box<dyn ChainstateInterface>> {
    let storage = chainstate_storage::inmemory::Store::new_empty().unwrap();
    let mut man = subsystem::Manager::new("TODO");
    let handle = man.add_subsystem(
        "chainstate",
        make_chainstate(
            chain_config,
            ChainstateConfig::new(),
            storage,
            None,
            Default::default(),
        )
        .unwrap(),
    );
    tokio::spawn(async move { man.main().await });
    handle
}

async fn setup() -> Mempool<SystemClock, SystemUsageEstimator> {
    let config = Arc::new(common::chain::config::create_unit_test_config());
    let chainstate_interface = start_chainstate(Arc::clone(&config)).await;
    Mempool::new(
        config,
        chainstate_interface,
        SystemClock {},
        SystemUsageEstimator {},
    )
}

#[tokio::test]
async fn tx_no_outputs() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_outputs(0)
        .generate_tx(&mempool)
        .await
        .expect("generate_tx failed");
    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(TxValidationError::NoOutputs))
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn tx_duplicate_inputs() -> anyhow::Result<()> {
    let mut mempool = setup().await;

    let outpoint_source_id = OutPointSourceId::from(mempool.chain_config.genesis_block_id());
    let input = TxInput::new(
        outpoint_source_id.clone(),
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let witness = b"attempted_double_spend".to_vec();
    let duplicate_input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(witness)),
    );
    let flags = 0;
    let locktime = 0;
    let outputs = tx_spend_input(&mempool, input.clone(), None, flags, locktime)
        .await?
        .outputs()
        .clone();
    let inputs = vec![input, duplicate_input];
    let tx = Transaction::new(flags, inputs, outputs, locktime)?;

    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(TxValidationError::DuplicateInputs))
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn tx_already_in_mempool() -> anyhow::Result<()> {
    let mut mempool = setup().await;

    let outpoint_source_id = OutPointSourceId::from(mempool.chain_config.genesis_block_id());
    let input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    let flags = 0;
    let locktime = 0;
    let tx = tx_spend_input(&mempool, input, None, flags, locktime).await?;

    mempool.add_transaction(tx.clone()).await?;
    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(
            TxValidationError::TransactionAlreadyInMempool
        ))
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn outpoint_not_found() -> anyhow::Result<()> {
    let mut mempool = setup().await;

    let outpoint_source_id = OutPointSourceId::from(mempool.chain_config.genesis_block_id());

    let good_input = TxInput::new(
        outpoint_source_id.clone(),
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let flags = 0;
    let locktime = 0;
    let outputs = tx_spend_input(&mempool, good_input, None, flags, locktime)
        .await?
        .outputs()
        .clone();

    let bad_outpoint_index = 1;
    let bad_input = TxInput::new(
        outpoint_source_id,
        bad_outpoint_index,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    let inputs = vec![bad_input];
    let tx = Transaction::new(flags, inputs, outputs, locktime)?;

    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(
            TxValidationError::OutPointNotFound { .. }
        ))
    ));
    mempool.store.assert_valid();

    Ok(())
}

#[tokio::test]
async fn tx_too_big() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let single_output_size = TxOutput::new(
        OutputValue::Coin(Amount::from_atoms(100)),
        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
    )
    .encoded_size();
    let too_many_outputs = MAX_BLOCK_SIZE_BYTES / single_output_size;
    let tx = TxGenerator::new()
        .with_num_outputs(too_many_outputs)
        .generate_tx(&mempool)
        .await
        .expect("generate_tx failed");
    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::TxValidationError(
            TxValidationError::ExceedsMaxBlockSize
        ))
    ));
    mempool.store.assert_valid();
    Ok(())
}

async fn test_replace_tx(original_fee: Amount, replacement_fee: Amount) -> Result<(), Error> {
    log::debug!(
        "tx_replace_tx: original_fee: {:?}, replacement_fee {:?}",
        original_fee,
        replacement_fee
    );
    let mut mempool = setup().await;
    let outpoint = mempool
        .available_outpoints(true)
        .await
        .expect("available_outpoints")
        .iter()
        .next()
        .expect("there should be an outpoint since setup creates the genesis transaction")
        .clone();

    let outpoint_source_id =
        OutPointSourceId::from(*outpoint.tx_id().get_tx_id().expect("Not Coinbase"));

    let input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let flags = 1;
    let locktime = 0;
    let original = tx_spend_input(&mempool, input.clone(), original_fee, flags, locktime)
        .await
        .expect("should be able to spend here");
    let original_id = original.get_id();
    log::debug!(
        "created a tx with fee {:?}",
        mempool.try_get_fee(&original).await
    );
    mempool.add_transaction(original).await?;

    let flags = 0;
    let replacement = tx_spend_input(&mempool, input, replacement_fee, flags, locktime)
        .await
        .expect("should be able to spend here");
    log::debug!(
        "created a replacement with fee {:?}",
        mempool.try_get_fee(&replacement).await
    );
    mempool.add_transaction(replacement).await?;
    assert!(!mempool.contains_transaction(&original_id));
    mempool.store.assert_valid();

    Ok(())
}

#[tokio::test]
async fn try_replace_irreplaceable() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let outpoint = mempool
        .available_outpoints(true)
        .await
        .expect("available_outpoints")
        .iter()
        .next()
        .expect("there should be an outpoint since setup creates the genesis transaction")
        .clone();

    let outpoint_source_id =
        OutPointSourceId::from(*outpoint.tx_id().get_tx_id().expect("Not Coinbase"));

    let input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let flags = 0;
    let locktime = 0;
    let original_fee = Amount::from_atoms(get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE));
    let original = tx_spend_input(&mempool, input.clone(), original_fee, flags, locktime)
        .await
        .expect("should be able to spend here");
    let original_id = original.get_id();
    mempool.add_transaction(original).await?;

    let flags = 0;
    let replacement_fee = (original_fee + Amount::from_atoms(1000)).unwrap();
    let replacement = tx_spend_input(&mempool, input, replacement_fee, flags, locktime)
        .await
        .expect("should be able to spend here");
    assert!(matches!(
        mempool.add_transaction(replacement.clone()).await,
        Err(Error::TxValidationError(
            TxValidationError::ConflictWithIrreplaceableTransaction
        ))
    ));

    mempool.drop_transaction(&original_id);
    mempool.add_transaction(replacement).await?;
    mempool.store.assert_valid();

    Ok(())
}

#[tokio::test]
async fn tx_replace() -> anyhow::Result<()> {
    let relay_fee = get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE);
    let replacement_fee = Amount::from_atoms(relay_fee + 100);
    test_replace_tx(Amount::from_atoms(100), replacement_fee).await?;
    let res = test_replace_tx(Amount::from_atoms(300), replacement_fee).await;
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::InsufficientFeesToRelayRBF
        ))
    ));
    let res = test_replace_tx(Amount::from_atoms(100), Amount::from_atoms(100)).await;
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::ReplacementFeeLowerThanOriginal { .. }
        ))
    ));
    let res = test_replace_tx(Amount::from_atoms(100), Amount::from_atoms(90)).await;
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::ReplacementFeeLowerThanOriginal { .. }
        ))
    ));
    Ok(())
}

#[tokio::test]
async fn tx_replace_child() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .replaceable()
        .generate_tx(&mempool)
        .await
        .expect("generate_replaceable_tx");
    mempool.add_transaction(tx.clone()).await?;

    let outpoint_source_id = OutPointSourceId::Transaction(tx.get_id());
    let child_tx_input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    // We want to test that even though child_tx doesn't signal replaceability directly, it is replaceable because its parent signalled replaceability
    // replaced
    let flags = 0;
    let locktime = 0;
    let child_tx = tx_spend_input(
        &mempool,
        child_tx_input.clone(),
        Amount::from_atoms(100),
        flags,
        locktime,
    )
    .await?;
    mempool.add_transaction(child_tx).await?;

    let relay_fee = get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE);
    let replacement_fee = Amount::from_atoms(relay_fee + 100);
    let replacement_tx =
        tx_spend_input(&mempool, child_tx_input, replacement_fee, flags, locktime).await?;
    mempool.add_transaction(replacement_tx).await?;
    mempool.store.assert_valid();
    Ok(())
}

// To test our validation of BIP125 Rule#4 (replacement transaction pays for its own bandwidth), we need to know the necessary relay fee before creating the transaction. The relay fee depends on the size of the transaction. The usual way to get the size of a transaction is to call `tx.encoded_size` but we cannot do this until we have created the transaction itself. To get around this cycle, we have precomputed the size of all transaction created by `tx_spend_input`. This value will be the same for all transactions created by this function.
const TX_SPEND_INPUT_SIZE: usize = 213;

async fn tx_spend_input<
    T: GetTime + Send + std::marker::Sync,
    M: GetMemoryUsage + Send + std::marker::Sync,
>(
    mempool: &Mempool<T, M>,
    input: TxInput,
    fee: impl Into<Option<Amount>>,
    flags: u32,
    locktime: u32,
) -> anyhow::Result<Transaction> {
    let fee = fee.into().map_or_else(
        || Amount::from_atoms(get_relay_fee_from_tx_size(estimate_tx_size(1, 2))),
        std::convert::identity,
    );
    tx_spend_several_inputs(mempool, &[input], fee, flags, locktime).await
}

async fn tx_spend_several_inputs<
    T: GetTime + Send + std::marker::Sync,
    M: GetMemoryUsage + Send + std::marker::Sync,
>(
    mempool: &Mempool<T, M>,
    inputs: &[TxInput],
    fee: Amount,
    flags: u32,
    locktime: u32,
) -> anyhow::Result<Transaction> {
    let mut input_values = Vec::new();
    for input in inputs {
        input_values.push(mempool.get_input_value(input).await?)
    }
    let input_value = input_values.into_iter().sum::<Option<_>>().ok_or_else(|| {
        let msg = String::from("tx_spend_input: overflow");
        log::error!("{}", msg);
        anyhow::Error::msg(msg)
    })?;

    let available_for_spending = (input_value - fee).ok_or_else(|| {
        let msg = format!(
            "tx_spend_several_inputs: input_value ({:?}) lower than fee ({:?})",
            input_value, fee
        );
        log::error!("{}", msg);
        anyhow::Error::msg(msg)
    })?;
    let spent = (available_for_spending / 2).expect("division error");

    let change = (available_for_spending - spent).ok_or_else(|| {
        let msg = String::from("Error computing change");
        anyhow::Error::msg(msg)
    })?;

    Transaction::new(
        flags,
        inputs.to_owned(),
        vec![
            TxOutput::new(
                OutputValue::Coin(spent),
                OutputPurpose::Transfer(Destination::AnyoneCanSpend),
            ),
            TxOutput::new(
                OutputValue::Coin(change),
                OutputPurpose::Transfer(Destination::AnyoneCanSpend),
            ),
        ],
        locktime,
    )
    .map_err(Into::into)
}

#[tokio::test]
async fn one_ancestor_replaceability_signal_is_enough() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_outputs(2)
        .generate_tx(&mempool)
        .await
        .expect("generate_replaceable_tx");

    mempool.add_transaction(tx.clone()).await?;

    let flags_replaceable = 1;
    let flags_irreplaceable = 0;
    let locktime = 0;

    let outpoint_source_id = OutPointSourceId::Transaction(tx.get_id());
    let ancestor_with_signal = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags_replaceable,
        locktime,
    )
    .await?;

    let ancestor_without_signal = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id,
            1,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags_irreplaceable,
        locktime,
    )
    .await?;

    mempool.add_transaction(ancestor_with_signal.clone()).await?;
    mempool.add_transaction(ancestor_without_signal.clone()).await?;

    let input_with_replaceable_parent = TxInput::new(
        OutPointSourceId::Transaction(ancestor_with_signal.get_id()),
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    let input_with_irreplaceable_parent = TxInput::new(
        OutPointSourceId::Transaction(ancestor_without_signal.get_id()),
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    // TODO compute minimum necessary relay fee instead of just overestimating it
    let original_fee = Amount::from_atoms(200);
    let dummy_output = TxOutput::new(
        OutputValue::Coin(original_fee),
        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
    );
    let replaced_tx = tx_spend_several_inputs(
        &mempool,
        &[input_with_irreplaceable_parent.clone(), input_with_replaceable_parent],
        original_fee,
        flags_irreplaceable,
        locktime,
    )
    .await?;
    let replaced_tx_id = replaced_tx.get_id();

    mempool.add_transaction(replaced_tx).await?;

    let replacing_tx = Transaction::new(
        flags_irreplaceable,
        vec![input_with_irreplaceable_parent],
        vec![dummy_output],
        locktime,
    )?;

    mempool.add_transaction(replacing_tx).await?;
    assert!(!mempool.contains_transaction(&replaced_tx_id));
    mempool.store.assert_valid();

    Ok(())
}

#[tokio::test]
async fn tx_mempool_entry() -> anyhow::Result<()> {
    use common::primitives::time;
    let mut mempool = setup().await;
    // Input different flag values just to make the hashes of these dummy transactions
    // different
    let txs = (1..=6)
        .into_iter()
        .map(|i| Transaction::new(i, vec![], vec![], 0).unwrap_or_else(|_| panic!("tx {}", i)))
        .collect::<Vec<_>>();
    let fee = Amount::from_atoms(0);

    // Generation 1
    let tx1_parents = BTreeSet::default();
    let entry1 = TxMempoolEntry::new(txs.get(0).unwrap().clone(), fee, tx1_parents, time::get());
    let tx2_parents = BTreeSet::default();
    let entry2 = TxMempoolEntry::new(txs.get(1).unwrap().clone(), fee, tx2_parents, time::get());

    // Generation 2
    let tx3_parents = vec![entry1.tx_id(), entry2.tx_id()].into_iter().collect();
    let entry3 = TxMempoolEntry::new(txs.get(2).unwrap().clone(), fee, tx3_parents, time::get());

    // Generation 3
    let tx4_parents = vec![entry3.tx_id()].into_iter().collect();
    let tx5_parents = vec![entry3.tx_id()].into_iter().collect();
    let entry4 = TxMempoolEntry::new(txs.get(3).unwrap().clone(), fee, tx4_parents, time::get());
    let entry5 = TxMempoolEntry::new(txs.get(4).unwrap().clone(), fee, tx5_parents, time::get());

    // Generation 4
    let tx6_parents = vec![entry3.tx_id(), entry4.tx_id(), entry5.tx_id()].into_iter().collect();
    let entry6 = TxMempoolEntry::new(txs.get(5).unwrap().clone(), fee, tx6_parents, time::get());

    let entries = vec![entry1, entry2, entry3, entry4, entry5, entry6];
    let ids = entries.clone().into_iter().map(|entry| entry.tx_id()).collect::<Vec<_>>();

    for entry in entries.into_iter() {
        mempool.store.add_tx(entry)?;
    }

    let entry1 = mempool.store.get_entry(ids.get(0).expect("index")).expect("entry");
    let entry2 = mempool.store.get_entry(ids.get(1).expect("index")).expect("entry");
    let entry3 = mempool.store.get_entry(ids.get(2).expect("index")).expect("entry");
    let entry4 = mempool.store.get_entry(ids.get(3).expect("index")).expect("entry");
    let entry5 = mempool.store.get_entry(ids.get(4).expect("index")).expect("entry");
    let entry6 = mempool.store.get_entry(ids.get(5).expect("index")).expect("entry");
    assert_eq!(entry1.unconfirmed_ancestors(&mempool.store).0.len(), 0);
    assert_eq!(entry2.unconfirmed_ancestors(&mempool.store).0.len(), 0);
    assert_eq!(entry3.unconfirmed_ancestors(&mempool.store).0.len(), 2);
    assert_eq!(entry4.unconfirmed_ancestors(&mempool.store).0.len(), 3);
    assert_eq!(entry5.unconfirmed_ancestors(&mempool.store).0.len(), 3);
    assert_eq!(entry6.unconfirmed_ancestors(&mempool.store).0.len(), 5);

    assert_eq!(entry1.count_with_descendants(), 5);
    assert_eq!(entry2.count_with_descendants(), 5);
    assert_eq!(entry3.count_with_descendants(), 4);
    assert_eq!(entry4.count_with_descendants(), 2);
    assert_eq!(entry5.count_with_descendants(), 2);
    assert_eq!(entry6.count_with_descendants(), 1);

    Ok(())
}

async fn test_bip125_max_replacements<
    T: GetTime + Send + std::marker::Sync,
    M: GetMemoryUsage + Send + std::marker::Sync,
>(
    mempool: &mut Mempool<T, M>,
    num_potential_replacements: usize,
) -> anyhow::Result<()> {
    let tx = TxGenerator::new()
        .with_num_outputs(num_potential_replacements - 1)
        .replaceable()
        .generate_tx(mempool)
        .await
        .expect("generate_tx failed");
    let input = tx.inputs().first().expect("one input").clone();
    let outputs = tx.outputs().clone();
    let tx_id = tx.get_id();
    mempool.add_transaction(tx).await?;

    let flags = 0;
    let locktime = 0;
    let outpoint_source_id = OutPointSourceId::Transaction(tx_id);
    let fee = get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE);
    for (index, _) in outputs.iter().enumerate() {
        let input = TxInput::new(
            outpoint_source_id.clone(),
            index.try_into().unwrap(),
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        );
        let tx = tx_spend_input(mempool, input, Amount::from_atoms(fee), flags, locktime).await?;
        mempool.add_transaction(tx).await?;
    }
    let mempool_size_before_replacement = mempool.store.txs_by_id.len();

    let replacement_fee = Amount::from_atoms(1000) * fee;
    let replacement_tx = tx_spend_input(mempool, input, replacement_fee, flags, locktime).await?;
    mempool.add_transaction(replacement_tx).await?;
    let mempool_size_after_replacement = mempool.store.txs_by_id.len();

    assert_eq!(
        mempool_size_after_replacement,
        mempool_size_before_replacement - num_potential_replacements + 1
    );
    Ok(())
}

#[tokio::test]
async fn too_many_conflicts() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let num_potential_replacements = MAX_BIP125_REPLACEMENT_CANDIDATES + 1;
    let err = test_bip125_max_replacements(&mut mempool, num_potential_replacements)
        .await
        .expect_err("expected error TooManyPotentialReplacements")
        .downcast()
        .expect("failed to downcast");
    assert!(matches!(
        err,
        Error::TxValidationError(TxValidationError::TooManyPotentialReplacements)
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn not_too_many_conflicts() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let num_potential_replacements = MAX_BIP125_REPLACEMENT_CANDIDATES;
    test_bip125_max_replacements(&mut mempool, num_potential_replacements).await
}

#[tokio::test]
async fn spends_new_unconfirmed() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_outputs(2)
        .replaceable()
        .generate_tx(&mempool)
        .await
        .expect("generate_replaceable_tx");
    let outpoint_source_id = OutPointSourceId::Transaction(tx.get_id());
    mempool.add_transaction(tx).await?;

    let input1 = TxInput::new(
        outpoint_source_id.clone(),
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );
    let input2 = TxInput::new(
        outpoint_source_id,
        1,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    let locktime = 0;
    let flags = 0;
    let original_fee = Amount::from_atoms(100);
    let replaced_tx =
        tx_spend_input(&mempool, input1.clone(), original_fee, flags, locktime).await?;
    mempool.add_transaction(replaced_tx).await?;
    let relay_fee = get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE);
    let replacement_fee = Amount::from_atoms(100 + relay_fee);
    let incoming_tx = tx_spend_several_inputs(
        &mempool,
        &[input1, input2],
        replacement_fee,
        flags,
        locktime,
    )
    .await?;

    let res = mempool.add_transaction(incoming_tx).await;
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::SpendsNewUnconfirmedOutput
        ))
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn pays_more_than_conflicts_with_descendants() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new().generate_tx(&mempool).await.expect("generate_replaceable_tx");
    let tx_id = tx.get_id();
    mempool.add_transaction(tx).await?;

    let outpoint_source_id = OutPointSourceId::Transaction(tx_id);
    let input = TxInput::new(
        outpoint_source_id,
        0,
        InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
    );

    let locktime = 0;
    let rbf = 1;
    let no_rbf = 0;

    // Create transaction that we will attempt to replace
    let original_fee = Amount::from_atoms(100);
    let replaced_tx = tx_spend_input(&mempool, input.clone(), original_fee, rbf, locktime).await?;
    let replaced_tx_fee = mempool.try_get_fee(&replaced_tx).await?;
    let replaced_id = replaced_tx.get_id();
    mempool.add_transaction(replaced_tx).await?;

    // Create some children for this transaction
    let descendant_outpoint_source_id = OutPointSourceId::Transaction(replaced_id);

    let descendant1_fee = Amount::from_atoms(100);
    let descendant1 = tx_spend_input(
        &mempool,
        TxInput::new(
            descendant_outpoint_source_id.clone(),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        descendant1_fee,
        no_rbf,
        locktime,
    )
    .await?;
    let descendant1_id = descendant1.get_id();
    mempool.add_transaction(descendant1).await?;

    let descendant2_fee = Amount::from_atoms(100);
    let descendant2 = tx_spend_input(
        &mempool,
        TxInput::new(
            descendant_outpoint_source_id,
            1,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        descendant2_fee,
        no_rbf,
        locktime,
    )
    .await?;
    let descendant2_id = descendant2.get_id();
    mempool.add_transaction(descendant2).await?;

    //Create a new incoming transaction that conflicts with `replaced_tx` because it spends
    //`input`. It will be rejected because its fee exactly equals (so is not greater than) the
    //sum of the fees of the conflict together with its descendants
    let insufficient_rbf_fee = [replaced_tx_fee, descendant1_fee, descendant2_fee]
        .into_iter()
        .sum::<Option<_>>()
        .unwrap();
    let incoming_tx = tx_spend_input(
        &mempool,
        input.clone(),
        insufficient_rbf_fee,
        no_rbf,
        locktime,
    )
    .await?;

    assert!(matches!(
        mempool.add_transaction(incoming_tx).await,
        Err(Error::TxValidationError(
            TxValidationError::TransactionFeeLowerThanConflictsWithDescendants
        ))
    ));

    let relay_fee = get_relay_fee_from_tx_size(TX_SPEND_INPUT_SIZE);
    let sufficient_rbf_fee = insufficient_rbf_fee + Amount::from_atoms(relay_fee);
    let incoming_tx = tx_spend_input(&mempool, input, sufficient_rbf_fee, no_rbf, locktime).await?;
    mempool.add_transaction(incoming_tx).await?;

    assert!(!mempool.contains_transaction(&replaced_id));
    assert!(!mempool.contains_transaction(&descendant1_id));
    assert!(!mempool.contains_transaction(&descendant2_id));
    mempool.store.assert_valid();
    Ok(())
}

#[derive(Clone)]
struct MockClock {
    time: Arc<AtomicU64>,
}

impl MockClock {
    fn new() -> Self {
        Self {
            time: Arc::new(AtomicU64::new(0)),
        }
    }

    fn set(&self, time: Time) {
        self.time.store(time.as_secs(), Ordering::SeqCst)
    }

    fn increment(&self, inc: Time) {
        self.time.store(
            self.time.load(Ordering::SeqCst) + inc.as_secs(),
            Ordering::SeqCst,
        )
    }
}

impl GetTime for MockClock {
    fn get_time(&self) -> Time {
        Duration::new(self.time.load(Ordering::SeqCst), 0)
    }
}

#[tokio::test]
async fn only_expired_entries_removed() -> anyhow::Result<()> {
    let mock_clock = MockClock::new();
    let config = Arc::new(common::chain::config::create_unit_test_config());
    let chainstate_interface = start_chainstate(Arc::clone(&config)).await;

    let mut mempool = Mempool::new(
        config,
        chainstate_interface,
        mock_clock.clone(),
        SystemUsageEstimator {},
    );

    let num_inputs = 1;
    let num_outputs = 2;
    let big_fee = get_relay_fee_from_tx_size(estimate_tx_size(num_inputs, num_outputs)) + 100;
    let parent = TxGenerator::new()
        .with_num_inputs(num_inputs)
        .with_num_outputs(num_outputs)
        .with_fee(Amount::from_atoms(big_fee))
        .generate_tx(&mempool)
        .await?;
    let parent_id = parent.get_id();
    mempool.add_transaction(parent).await?;

    let flags = 0;
    let locktime = 0;
    let outpoint_source_id = OutPointSourceId::Transaction(parent_id);
    let child_0 = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags,
        locktime,
    )
    .await?;

    let child_1 = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id,
            1,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags,
        locktime,
    )
    .await?;
    let child_1_id = child_1.get_id();

    let expired_tx_id = child_0.get_id();
    mempool.add_transaction(child_0).await?;

    // Simulate the parent being added to a block
    // We have to do this because if we leave this parent in the mempool then it will be
    // expired, and so removed along with both its children, and thus the addition of child_1 to
    // the mempool will fail
    let block = Block::new(
        vec![],
        Id::new(H256::zero()),
        BlockTimestamp::from_int_seconds(0),
        ConsensusData::None,
        BlockReward::new(vec![]),
    )
    .map_err(|_| anyhow::Error::msg("block creation error"))?;

    mempool
        .chainstate_handle
        .call_mut(|this| this.process_block(block, BlockSource::Local))
        .await??;
    mock_clock.set(DEFAULT_MEMPOOL_EXPIRY + Duration::new(1, 0));

    mempool.add_transaction(child_1).await?;
    assert!(!mempool.contains_transaction(&expired_tx_id));
    assert!(mempool.contains_transaction(&child_1_id));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn rolling_fee() -> anyhow::Result<()> {
    logging::init_logging::<&str>(None);
    let mock_clock = MockClock::new();
    let mut mock_usage = MockGetMemoryUsage::new();
    // Add parent
    // Add first child
    mock_usage.expect_get_memory_usage().times(2).return_const(0usize);
    // Add second child, triggering the trimming process
    mock_usage
        .expect_get_memory_usage()
        .times(1)
        .return_const(MAX_MEMPOOL_SIZE_BYTES + 1);
    // After removing one entry, cause the code to exit the loop by showing a small usage
    mock_usage.expect_get_memory_usage().return_const(0usize);

    let config = Arc::new(common::chain::config::create_unit_test_config());
    let chainstate_interface = start_chainstate(Arc::clone(&config)).await;

    let mut mempool = Mempool::new(config, chainstate_interface, mock_clock.clone(), mock_usage);

    let num_inputs = 1;
    let num_outputs = 3;

    // Use a higher than default fee because we don't want this transction to be evicted during
    // the trimming process
    let parent = TxGenerator::new()
        .with_num_inputs(num_inputs)
        .with_num_outputs(num_outputs)
        .generate_tx(&mempool)
        .await?;
    let parent_id = parent.get_id();
    log::debug!("parent_id: {}", parent_id.get());
    log::debug!("before adding parent");
    mempool.add_transaction(parent).await?;
    log::debug!("after adding parent");

    let flags = 0;
    let locktime = 0;
    let outpoint_source_id = OutPointSourceId::Transaction(parent_id);

    // child_0 has the lower fee so it will be evicted when memory usage is too high
    let child_0 = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags,
        locktime,
    )
    .await?;
    let child_0_id = child_0.get_id();
    log::debug!("child_0_id {}", child_0_id.get());

    let big_fee = Amount::from_atoms(
        get_relay_fee_from_tx_size(estimate_tx_size(num_inputs, num_outputs)) + 100,
    );
    let child_1 = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            1,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        big_fee,
        flags,
        locktime,
    )
    .await?;
    let child_1_id = child_1.get_id();
    log::debug!("child_1_id {}", child_1_id.get());
    mempool.add_transaction(child_0.clone()).await?;
    log::debug!("added child_0");
    mempool.add_transaction(child_1).await?;
    log::debug!("added child_1");

    assert_eq!(mempool.store.txs_by_id.len(), 2);
    assert!(mempool.contains_transaction(&child_1_id));
    assert!(!mempool.contains_transaction(&child_0_id));
    let rolling_fee = mempool.get_minimum_rolling_fee();
    let child_0_fee = mempool.try_get_fee(&child_0).await?;
    log::debug!("FeeRate of child_0 {:?}", child_0_fee);
    assert_eq!(
        rolling_fee,
        *INCREMENTAL_RELAY_FEE_RATE
            + FeeRate::from_total_tx_fee(child_0_fee, child_0.encoded_size())
    );
    assert_eq!(rolling_fee, FeeRate::new(Amount::from_atoms(3591)));
    log::debug!(
        "minimum rolling fee after child_0's eviction {:?}",
        rolling_fee
    );
    assert_eq!(
        rolling_fee,
        FeeRate::from_total_tx_fee(mempool.try_get_fee(&child_0).await?, child_0.encoded_size())
            + *INCREMENTAL_RELAY_FEE_RATE
    );

    // Now that the minimum rolling fee has been bumped up, a low-fee tx will not pass
    // validation
    let child_2 = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            2,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags,
        locktime,
    )
    .await?;
    log::debug!(
        "before child2: fee = {:?}, size = {}, minimum fee rate = {:?}",
        mempool.try_get_fee(&child_2).await?,
        child_2.encoded_size(),
        mempool.get_minimum_rolling_fee()
    );
    let res = mempool.add_transaction(child_2).await;
    log::debug!("result of adding child2 {:?}", res);
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::RollingFeeThresholdNotMet { .. }
        ))
    ));

    // We provide a sufficient fee for the tx to pass the minimum rolling fee requirement
    let child_2_high_fee = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id,
            2,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        mempool.get_minimum_rolling_fee().compute_fee(estimate_tx_size(1, 1)),
        flags,
        locktime,
    )
    .await?;
    log::debug!("before child2_high_fee");
    mempool.add_transaction(child_2_high_fee).await?;

    // We simulate a block being accepted so the rolling fee will begin to decay
    let block = Block::new(
        vec![],
        Id::new(H256::zero()),
        BlockTimestamp::from_int_seconds(0),
        ConsensusData::None,
        BlockReward::new(vec![]),
    )
    .map_err(|_| anyhow::Error::msg("block creation error"))?;

    mempool
        .chainstate_handle
        .call_mut(|this| this.process_block(block, BlockSource::Local))
        .await??;
    // Because the rolling fee is only updated when we attempt to add a tx to the mempool
    // we need to submit a "dummy" tx to trigger these updates.

    // Since memory usage is now zero, it is less than 1/4 of the max size
    // and ROLLING_FEE_BASE_HALFLIFE / 4 is the time it will take for the fee to halve
    // We are going to submit dummy txs to the mempool incrementing time by this halflife
    // between txs. Finally, when the fee rate falls under INCREMENTAL_RELAY_THRESHOLD, we
    // observer that it is set to zero
    let halflife = ROLLING_FEE_BASE_HALFLIFE / 4;
    mock_clock.increment(halflife);
    let dummy_tx = TxGenerator::new()
        .with_fee(Amount::from_atoms(100))
        .generate_tx(&mempool)
        .await?;
    log::debug!(
        "First attempt to add dummy which pays a fee of {:?}",
        mempool.try_get_fee(&dummy_tx).await?
    );
    let res = mempool.add_transaction(dummy_tx.clone()).await;

    log::debug!("Result of first attempt to add dummy: {:?}", res);
    assert!(matches!(
        res,
        Err(Error::TxValidationError(
            TxValidationError::RollingFeeThresholdNotMet { .. }
        ))
    ));
    log::debug!(
        "minimum rolling fee after first attempt to add dummy: {:?}",
        mempool.get_minimum_rolling_fee()
    );
    assert_eq!(
        mempool.get_minimum_rolling_fee(),
        rolling_fee / std::num::NonZeroU128::new(2).expect("nonzero")
    );

    mock_clock.increment(halflife);
    log::debug!("Second attempt to add dummy");
    mempool.add_transaction(dummy_tx).await?;
    log::debug!(
        "minimum rolling fee after first second to add dummy: {:?}",
        mempool.get_minimum_rolling_fee()
    );
    assert_eq!(
        mempool.get_minimum_rolling_fee(),
        rolling_fee / std::num::NonZeroU128::new(4).expect("nonzero")
    );
    log::debug!(
        "After successful addition of dummy, rolling fee rate is {:?}",
        mempool.get_minimum_rolling_fee()
    );

    // Add another dummmy until rolling feerate drops to zero
    mock_clock.increment(halflife);
    let another_dummy = TxGenerator::new()
        .with_fee(Amount::from_atoms(100))
        .generate_tx(&mempool)
        .await?;
    mempool.add_transaction(another_dummy).await?;
    assert_eq!(
        mempool.get_minimum_rolling_fee(),
        FeeRate::new(Amount::from_atoms(0))
    );

    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn different_size_txs() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let initial_tx = TxGenerator::new()
        .with_num_inputs(1)
        .with_num_outputs(10_000)
        .generate_tx(&mempool)
        .await?;
    mempool.add_transaction(initial_tx).await?;

    let target_txs = 100;
    for i in 0..target_txs {
        let num_inputs = i + 1;
        let num_outputs = i + 1;
        let tx = TxGenerator::new()
            .with_num_inputs(num_inputs)
            .with_num_outputs(num_outputs)
            .generate_tx(&mempool)
            .await?;
        mempool.add_transaction(tx).await?;
    }

    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn descendant_score() -> anyhow::Result<()> {
    let mut mempool = setup().await;
    let tx = TxGenerator::new()
        .with_num_outputs(2)
        .generate_tx(&mempool)
        .await
        .expect("generate_replaceable_tx");
    let tx_id = tx.get_id();
    mempool.add_transaction(tx).await?;

    let outpoint_source_id = OutPointSourceId::Transaction(tx_id);

    let flags = 0;
    let locktime = 0;

    let tx_b_fee = Amount::from_atoms(get_relay_fee_from_tx_size(estimate_tx_size(1, 2)));
    let tx_a_fee = (tx_b_fee + Amount::from_atoms(1000)).unwrap();
    let tx_c_fee = (tx_a_fee + Amount::from_atoms(1000)).unwrap();
    let tx_a = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id.clone(),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        tx_a_fee,
        flags,
        locktime,
    )
    .await?;
    let tx_a_id = tx_a.get_id();
    log::debug!("tx_a_id : {}", tx_a_id.get());
    log::debug!("tx_a fee : {:?}", mempool.try_get_fee(&tx_a).await?);
    mempool.add_transaction(tx_a).await?;

    let tx_b = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id,
            1,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        tx_b_fee,
        flags,
        locktime,
    )
    .await?;
    let tx_b_id = tx_b.get_id();
    log::debug!("tx_b_id : {}", tx_b_id.get());
    log::debug!("tx_b fee : {:?}", mempool.try_get_fee(&tx_b).await?);
    mempool.add_transaction(tx_b).await?;

    let tx_c = tx_spend_input(
        &mempool,
        TxInput::new(
            OutPointSourceId::Transaction(tx_b_id),
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        tx_c_fee,
        flags,
        locktime,
    )
    .await?;
    let tx_c_id = tx_c.get_id();
    log::debug!("tx_c_id : {}", tx_c_id.get());
    log::debug!("tx_c fee : {:?}", mempool.try_get_fee(&tx_c).await?);
    mempool.add_transaction(tx_c).await?;

    let entry_a = mempool.store.txs_by_id.get(&tx_a_id.get()).expect("tx_a");
    log::debug!("entry a has score {:?}", entry_a.fees_with_descendants);
    let entry_b = mempool.store.txs_by_id.get(&tx_b_id.get()).expect("tx_b");
    log::debug!("entry b has score {:?}", entry_b.fees_with_descendants);
    let entry_c = mempool.store.txs_by_id.get(&tx_c_id.get()).expect("tx_c").clone();
    log::debug!("entry c has score {:?}", entry_c.fees_with_descendants);
    assert_eq!(entry_a.fee, entry_a.fees_with_descendants);
    assert_eq!(
        entry_b.fees_with_descendants,
        (entry_b.fee + entry_c.fee).unwrap()
    );
    assert!(!mempool.store.txs_by_descendant_score.contains_key(&tx_b_fee.into()));
    log::debug!(
        "raw_txs_by_descendant_score {:?}",
        mempool.store.txs_by_descendant_score
    );
    check_txs_sorted_by_descendant_sore(&mempool);

    mempool.drop_transaction(&entry_c.tx.get_id());
    assert!(!mempool.store.txs_by_descendant_score.contains_key(&tx_c_fee.into()));
    let entry_b = mempool.store.txs_by_id.get(&tx_b_id.get()).expect("tx_b");
    assert_eq!(entry_b.fees_with_descendants, entry_b.fee);

    check_txs_sorted_by_descendant_sore(&mempool);
    mempool.store.assert_valid();

    Ok(())
}

fn check_txs_sorted_by_descendant_sore(mempool: &Mempool<SystemClock, SystemUsageEstimator>) {
    let txs_by_descendant_score =
        mempool.store.txs_by_descendant_score.values().flatten().collect::<Vec<_>>();
    for i in 0..(txs_by_descendant_score.len() - 1) {
        log::debug!("i =  {}", i);
        let tx_id = txs_by_descendant_score.get(i).unwrap();
        let next_tx_id = txs_by_descendant_score.get(i + 1).unwrap();
        let entry_score = mempool.store.txs_by_id.get(&tx_id.get()).unwrap().fees_with_descendants;
        let next_entry_score =
            mempool.store.txs_by_id.get(&next_tx_id.get()).unwrap().fees_with_descendants;
        log::debug!("entry_score: {:?}", entry_score);
        log::debug!("next_entry_score: {:?}", next_entry_score);
        assert!(entry_score <= next_entry_score)
    }
}

#[tokio::test]
async fn descendant_of_expired_entry() -> anyhow::Result<()> {
    let mock_clock = MockClock::new();
    logging::init_logging::<&str>(None);

    let config = Arc::new(common::chain::config::create_unit_test_config());
    let chainstate_interface = start_chainstate(Arc::clone(&config)).await;

    let mut mempool = Mempool::new(
        config,
        chainstate_interface,
        mock_clock.clone(),
        SystemUsageEstimator {},
    );

    let num_inputs = 1;
    let num_outputs = 2;
    let fee = get_relay_fee_from_tx_size(estimate_tx_size(num_inputs, num_outputs));
    let parent = TxGenerator::new()
        .with_num_inputs(num_inputs)
        .with_num_outputs(num_outputs)
        .with_fee(Amount::from_atoms(fee))
        .generate_tx(&mempool)
        .await?;
    let parent_id = parent.get_id();
    mempool.add_transaction(parent).await?;

    let flags = 0;
    let locktime = 0;
    let outpoint_source_id = OutPointSourceId::Transaction(parent_id);
    let child = tx_spend_input(
        &mempool,
        TxInput::new(
            outpoint_source_id,
            0,
            InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
        ),
        None,
        flags,
        locktime,
    )
    .await?;
    let child_id = child.get_id();
    mock_clock.set(DEFAULT_MEMPOOL_EXPIRY + Duration::new(1, 0));

    assert!(matches!(
        mempool.add_transaction(child).await,
        Err(Error::TxValidationError(
            TxValidationError::DescendantOfExpiredTransaction
        ))
    ));

    assert!(!mempool.contains_transaction(&parent_id));
    assert!(!mempool.contains_transaction(&child_id));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn mempool_full() -> anyhow::Result<()> {
    logging::init_logging::<&str>(None);
    let mut mock_usage = MockGetMemoryUsage::new();
    mock_usage
        .expect_get_memory_usage()
        .times(1)
        .return_const(MAX_MEMPOOL_SIZE_BYTES + 1);

    let config = Arc::new(common::chain::config::create_unit_test_config());
    let chainstate_interface = start_chainstate(Arc::clone(&config)).await;

    let mut mempool = Mempool::new(config, chainstate_interface, SystemClock, mock_usage);

    let tx = TxGenerator::new().generate_tx(&mempool).await?;
    log::debug!("mempool_full: tx has is {}", tx.get_id().get());
    assert!(matches!(
        mempool.add_transaction(tx).await,
        Err(Error::MempoolFull)
    ));
    mempool.store.assert_valid();
    Ok(())
}

#[tokio::test]
async fn no_empty_bags_in_descendant_score_index() -> anyhow::Result<()> {
    let mut mempool = setup().await;

    let num_inputs = 1;
    let num_outputs = 100;
    let fee = get_relay_fee_from_tx_size(estimate_tx_size(num_inputs, num_outputs));
    let parent = TxGenerator::new()
        .with_num_inputs(num_inputs)
        .with_num_outputs(num_outputs)
        .with_fee(Amount::from_atoms(fee))
        .generate_tx(&mempool)
        .await?;
    let parent_id = parent.get_id();

    let outpoint_source_id = OutPointSourceId::Transaction(parent.get_id());
    mempool.add_transaction(parent).await?;
    let num_child_txs = num_outputs;
    let flags = 0;
    let locktime = 0;
    let mut txs = Vec::new();
    for i in 0..num_child_txs {
        txs.push(
            tx_spend_input(
                &mempool,
                TxInput::new(
                    outpoint_source_id.clone(),
                    u32::try_from(i).unwrap(),
                    InputWitness::NoSignature(Some(DUMMY_WITNESS_MSG.to_vec())),
                ),
                Amount::from_atoms(fee + u128::try_from(i).unwrap()),
                flags,
                locktime,
            )
            .await?,
        )
    }
    let ids = txs.iter().map(|tx| tx.get_id()).collect::<Vec<_>>();

    for tx in txs {
        mempool.add_transaction(tx).await?;
    }

    mempool.drop_transaction(&parent_id);
    for id in ids {
        mempool.drop_transaction(&id)
    }
    assert!(mempool.store.txs_by_descendant_score.is_empty());
    mempool.store.assert_valid();
    Ok(())
}
