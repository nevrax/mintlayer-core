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

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use chainstate_types::Locator;
use common::chain::Transaction;
use common::chain::TxInput;
use common::{
    chain::{Block, GenBlock},
    primitives::{BlockHeight, Id},
};

use crate::{
    chainstate_interface::ChainstateInterface, BlockSource, ChainstateError, ChainstateEvent,
};

impl<
        T: Deref<Target = dyn ChainstateInterface> + DerefMut<Target = dyn ChainstateInterface> + Send,
    > ChainstateInterface for T
{
    fn subscribe_to_events(&mut self, handler: Arc<dyn Fn(ChainstateEvent) + Send + Sync>) {
        self.deref_mut().subscribe_to_events(handler)
    }

    fn process_block(&mut self, block: Block, source: BlockSource) -> Result<(), ChainstateError> {
        self.deref_mut().process_block(block, source)
    }

    fn preliminary_block_check(&self, block: Block) -> Result<Block, ChainstateError> {
        self.deref().preliminary_block_check(block)
    }

    fn get_best_block_id(&self) -> Result<Id<GenBlock>, ChainstateError> {
        self.deref().get_best_block_id()
    }

    fn is_block_in_main_chain(&self, block_id: &Id<Block>) -> Result<bool, ChainstateError> {
        self.deref().is_block_in_main_chain(block_id)
    }

    fn get_block_height_in_main_chain(
        &self,
        block_id: &Id<GenBlock>,
    ) -> Result<Option<BlockHeight>, ChainstateError> {
        self.deref().get_block_height_in_main_chain(block_id)
    }

    fn get_best_block_height(&self) -> Result<BlockHeight, ChainstateError> {
        self.deref().get_best_block_height()
    }

    fn get_block_id_from_height(
        &self,
        height: &BlockHeight,
    ) -> Result<Option<Id<GenBlock>>, ChainstateError> {
        self.deref().get_block_id_from_height(height)
    }

    fn get_block(&self, block_id: Id<Block>) -> Result<Option<Block>, ChainstateError> {
        self.deref().get_block(block_id)
    }

    fn get_locator(&self) -> Result<Locator, ChainstateError> {
        self.deref().get_locator()
    }

    fn get_headers(
        &self,
        locator: Locator,
    ) -> Result<Vec<common::chain::block::BlockHeader>, ChainstateError> {
        self.deref().get_headers(locator)
    }

    fn filter_already_existing_blocks(
        &self,
        headers: Vec<common::chain::block::BlockHeader>,
    ) -> Result<Vec<common::chain::block::BlockHeader>, ChainstateError> {
        self.deref().filter_already_existing_blocks(headers)
    }

    fn available_inputs(&self, tx: &Transaction) -> Result<Vec<TxInput>, ChainstateError> {
        self.deref().available_inputs(tx)
    }

    fn get_outpoint_value(
        &self,
        outpoint: &common::chain::OutPoint,
    ) -> Result<common::primitives::Amount, ChainstateError> {
        self.deref().get_outpoint_value(outpoint)
    }

    fn confirmed_outpoints(
        &self,
    ) -> Result<std::collections::BTreeSet<common::chain::OutPoint>, ChainstateError> {
        self.deref().confirmed_outpoints()
    }
}
#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use chainstate_storage::inmemory::Store;
    use common::{
        chain::{config::create_unit_test_config, ChainConfig},
        primitives::BlockHeight,
    };

    use crate::{
        chainstate_interface::ChainstateInterface, detail::time_getter::TimeGetter,
        make_chainstate, ChainstateConfig,
    };

    fn test_interface_ref<C: ChainstateInterface>(chainstate: &C, chain_config: &ChainConfig) {
        assert_eq!(
            chainstate.get_best_block_id().unwrap(),
            chain_config.genesis_block_id()
        );
        assert_eq!(
            chainstate.get_best_block_height().unwrap(),
            BlockHeight::new(0)
        );
    }

    fn test_interface<C: ChainstateInterface>(chainstate: C, chain_config: &ChainConfig) {
        assert_eq!(
            chainstate.get_best_block_id().unwrap(),
            chain_config.genesis_block_id()
        );
        assert_eq!(
            chainstate.get_best_block_height().unwrap(),
            BlockHeight::new(0)
        );
    }

    #[test]
    fn boxed_interface_call() {
        utils::concurrency::model(|| {
            let chain_config = Arc::new(create_unit_test_config());
            let chainstate_config = ChainstateConfig {
                max_db_commit_attempts: 10,
                max_orphan_blocks: 0,
            };
            let chainstate_storage = Store::new_empty().unwrap();

            let boxed_chainstate: Box<dyn ChainstateInterface> = make_chainstate(
                chain_config.clone(),
                chainstate_config,
                chainstate_storage,
                None,
                TimeGetter::default(),
            )
            .unwrap();

            test_interface_ref(&boxed_chainstate, &*chain_config);
            test_interface(boxed_chainstate, &*chain_config);
        });
    }
}
