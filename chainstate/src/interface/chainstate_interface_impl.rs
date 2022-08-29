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

use std::collections::BTreeSet;

use chainstate_storage::BlockchainStorage;
use chainstate_types::PropertyQueryError;
use common::chain::OutPoint;
use common::chain::OutputSpentState;
use common::chain::Transaction;
use common::chain::TxInput;
use common::primitives::Amount;
use common::{
    chain::block::{Block, BlockHeader, GenBlock},
    primitives::{BlockHeight, Id},
};
use utils::eventhandler::EventHandler;

use crate::{
    detail::{self, BlockSource},
    ChainstateError, ChainstateEvent, ChainstateInterface, Locator,
};

pub struct ChainstateInterfaceImpl<S> {
    chainstate: detail::Chainstate<S>,
}

impl<S> ChainstateInterfaceImpl<S> {
    pub fn new(chainstate: detail::Chainstate<S>) -> Self {
        Self { chainstate }
    }
}

impl<S: BlockchainStorage> ChainstateInterface for ChainstateInterfaceImpl<S> {
    fn subscribe_to_events(&mut self, handler: EventHandler<ChainstateEvent>) {
        self.chainstate.subscribe_to_events(handler)
    }

    fn process_block(&mut self, block: Block, source: BlockSource) -> Result<(), ChainstateError> {
        self.chainstate
            .process_block(block, source)
            .map_err(ChainstateError::ProcessBlockError)?;
        Ok(())
    }

    fn preliminary_block_check(&self, block: Block) -> Result<Block, ChainstateError> {
        self.chainstate
            .preliminary_block_check(block)
            .map_err(ChainstateError::ProcessBlockError)
    }

    fn get_best_block_id(&self) -> Result<Id<GenBlock>, ChainstateError> {
        self.chainstate
            .get_best_block_id()
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn is_block_in_main_chain(&self, block_id: &Id<Block>) -> Result<bool, ChainstateError> {
        self.chainstate
            .get_block_height_in_main_chain(&(*block_id).into())
            .map_err(ChainstateError::FailedToReadProperty)
            .map(|ht| ht.is_some())
    }

    fn get_block_height_in_main_chain(
        &self,
        block_id: &Id<GenBlock>,
    ) -> Result<Option<BlockHeight>, ChainstateError> {
        self.chainstate
            .get_block_height_in_main_chain(block_id)
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn get_block_id_from_height(
        &self,
        height: &BlockHeight,
    ) -> Result<Option<Id<GenBlock>>, ChainstateError> {
        self.chainstate
            .get_block_id_from_height(height)
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn get_block(&self, block_id: Id<Block>) -> Result<Option<Block>, ChainstateError> {
        self.chainstate
            .get_block(block_id)
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn get_locator(&self) -> Result<Locator, ChainstateError> {
        self.chainstate.get_locator().map_err(ChainstateError::FailedToReadProperty)
    }

    fn get_headers(&self, locator: Locator) -> Result<Vec<BlockHeader>, ChainstateError> {
        self.chainstate
            .get_headers(locator)
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn filter_already_existing_blocks(
        &self,
        headers: Vec<BlockHeader>,
    ) -> Result<Vec<BlockHeader>, ChainstateError> {
        self.chainstate
            .filter_already_existing_blocks(headers)
            .map_err(ChainstateError::FailedToReadProperty)
    }

    fn get_best_block_height(&self) -> Result<BlockHeight, ChainstateError> {
        let best_block_index = self
            .chainstate
            .get_best_block_index()
            .map_err(ChainstateError::FailedToReadProperty)?
            .expect("Best block index could not be found");
        Ok(best_block_index.block_height())
    }

    fn available_inputs(&self, tx: &Transaction) -> Result<Vec<TxInput>, ChainstateError> {
        eprintln!("available_inputs chainstate");
        let mut available_inputs = Vec::new();
        for input in tx.inputs() {
            let index = self
                .chainstate
                .get_mainchain_tx_index(&input.outpoint().tx_id())
                .map_err(ChainstateError::FailedToReadProperty)?;
            if let Some(index) = index {
                if OutputSpentState::Unspent
                    == index.get_spent_state(input.outpoint().output_index()).map_err(|_| {
                        ChainstateError::FailedToReadProperty(
                            PropertyQueryError::OutpointIndexOutOfRange,
                        )
                    })?
                {
                    available_inputs.push(input.clone())
                }
            }
        }
        Ok(available_inputs)
    }

    fn get_outpoint_value(
        &self,
        _outpoint: &common::chain::OutPoint,
    ) -> Result<common::primitives::Amount, ChainstateError> {
        Ok(Amount::from_atoms(0))
    }

    fn confirmed_outpoints(&self) -> Result<BTreeSet<OutPoint>, ChainstateError> {
        Ok(BTreeSet::new())
    }
}
