// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod builder;
pub mod emission_schedule;

pub use builder::Builder;
pub use emission_schedule::{EmissionSchedule, EmissionScheduleTabular, Mlt};

use std::{collections::BTreeMap, time::Duration};

use hex::FromHex;

use crate::{
    chain::{
        block::{timestamp::BlockTimestamp, Block, ConsensusData},
        signature::inputsig::InputWitness,
        transaction::{Destination, Transaction},
        upgrades::NetUpgrades,
        OutputPurpose, PoWChainConfig, UpgradeVersion,
    },
    primitives::{semver::SemVer, Amount, BlockHeight id::{Id, H256}},
};

pub const DEFAULT_TARGET_BLOCK_SPACING: Duration = Duration::from_secs(120);
pub const VERSION: SemVer = SemVer::new(0, 1, 0);

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    strum::Display,
    strum::EnumVariantNames,
    strum::EnumString,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ChainType {
    Mainnet,
    Testnet,
    Regtest,
    Signet,
}

impl ChainType {
    const fn address_prefix(&self) -> &'static str {
        match self {
            ChainType::Mainnet => "mtc",
            ChainType::Testnet => "tmt",
            ChainType::Regtest => "rmt",
            ChainType::Signet => "smt",
        }
    }

    pub const fn magic_bytes(&self) -> [u8; 4] {
        match self {
            ChainType::Mainnet => [0x1a, 0x64, 0xe5, 0xf1],
            ChainType::Testnet => [0x2b, 0x7e, 0x19, 0xf6],
            ChainType::Regtest => [0xaa, 0xbb, 0xcc, 0xdd],
            ChainType::Signet => [0xf3, 0xf7, 0x7b, 0x45],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChainConfig {
    chain_type: ChainType,
    net_upgrades: NetUpgrades<UpgradeVersion>,
    genesis_block: Block,
    pub target_block_spacing: Duration,
    coin_decimals: u8,
    emission_schedule: EmissionSchedule,
}

impl ChainConfig {
    pub fn new(chain_type: ChainType) -> Self {
        Builder::new(chain_type).build()
    }

    pub fn address_prefix(&self) -> &str {
        &self.chain_type.address_prefix()
    }

    pub fn genesis_block_id(&self) -> Id<Block> {
        self.genesis_block_id.clone()
    }

    pub fn genesis_block(&self) -> &Block {
        &self.genesis_block
    }

    pub fn magic_bytes(&self) -> &[u8; 4] {
        &self.chain_type.magic_bytes()
    }

    pub fn magic_bytes_as_u32(&self) -> u32 {
        u32::from_le_bytes(*self.magic_bytes())
    }

    pub fn chain_type(&self) -> &ChainType {
        &self.chain_type
    }

    pub fn net_upgrade(&self) -> &NetUpgrades<UpgradeVersion> {
        &self.net_upgrades
    }

    pub fn height_checkpoints(&self) -> &BTreeMap<BlockHeight, Id<Block>> {
        &self.height_checkpoint_data
    }

    pub fn target_block_spacing(&self) -> &Duration {
        &self.target_block_spacing
    }

    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.emission_schedule
    }

    pub fn coin_decimals(&self) -> u8 {
        self.coin_decimals
    }

    pub fn block_subsidy_at_height(&self, height: &BlockHeight) -> Amount {
        self.emission_schedule().subsidy(*height).to_amount_atoms()
    }

    // TODO: this should be part of net-upgrades. There should be no canonical definition of PoW for any chain config
    pub const fn get_proof_of_work_config(&self) -> PoWChainConfig {
        PoWChainConfig::new(self.chain_type)
    }
}

fn create_mainnet_genesis() -> Block {
    use crate::chain::transaction::{TxInput, TxOutput};

    // TODO: replace this with our mint key
    // Private key: "0080732e24bb0b704cb455e233b539f2c63ab411989a54984f84a6a2eb2e933e160f"
    // Pubub key:  "008090f5aee58be97ce2f7c014fa97ffff8c459a0c491f8124950724a187d134e25c"
    // Public key hash:  "8640e6a3d3d53c7dffe2790b0e147c9a77197033"
    // Destination:  "008640e6a3d3d53c7dffe2790b0e147c9a77197033"
    let genesis_mint_pubkeyhash_hex_encoded = "008640e6a3d3d53c7dffe2790b0e147c9a77197033";
    let genesis_mint_pubkeyhash_encoded = Vec::from_hex(genesis_mint_pubkeyhash_hex_encoded)
        .expect("Hex decoding of pubkeyhash shouldn't fail");
    let genesis_mint_destination = <Destination as parity_scale_codec::DecodeAll>::decode_all(
        &mut genesis_mint_pubkeyhash_encoded.as_slice(),
    )
    .expect("Decoding genesis mint destination shouldn't fail");

    let genesis_message = b"".to_vec();
    let input = TxInput::new(
        Id::<Transaction>::new(H256::zero()).into(),
        0,
        InputWitness::NoSignature(Some(genesis_message)),
    );
    // TODO: replace this with the real genesis mint value
    let output = TxOutput::new(
        Amount::from_atoms(100000000000000),
        OutputPurpose::Transfer(genesis_mint_destination),
    );
    let tx = Transaction::new(0, vec![input], vec![output], 0)
        .expect("Failed to create genesis coinbase transaction");

    Block::new(
        vec![tx],
        None,
        BlockTimestamp::from_int_seconds(1639975460),
        ConsensusData::None,
    )
    .expect("Error creating genesis block")
}

fn create_unit_test_genesis(premine_destination: Destination) -> Block {
    use crate::chain::transaction::{TxInput, TxOutput};

    let genesis_message = b"".to_vec();
    let input = TxInput::new(
        Id::<Transaction>::new(H256::zero()).into(),
        0,
        InputWitness::NoSignature(Some(genesis_message)),
    );

    let output = TxOutput::new(
        Amount::from_atoms(100000000000000),
        OutputPurpose::Transfer(premine_destination),
    );
    let tx = Transaction::new(0, vec![input], vec![output], 0)
        .expect("Failed to create genesis coinbase transaction");

    Block::new(
        vec![tx],
        None,
        BlockTimestamp::from_int_seconds(1639975460),
        ConsensusData::None,
    )
    .expect("Error creating genesis block")
}

pub fn create_mainnet() -> ChainConfig {
    Builder::new(ChainType::Mainnet).build()
}

pub fn create_regtest() -> ChainConfig {
    Builder::new(ChainType::Regtest).build()
}

pub fn create_unit_test_config() -> ChainConfig {
    Builder::new(ChainType::Mainnet)
        .net_upgrades(NetUpgrades::unit_tests())
        .genesis_unittest(Destination::AnyoneCanSpend)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mainnet_creation() {
        let config = create_mainnet();

        assert!(!config.net_upgrades.is_empty());
        assert_eq!(2, config.net_upgrades.len());
        assert_eq!(config.chain_type(), &ChainType::Mainnet);
    }

    #[test]
    fn chain_type_names() {
        use strum::VariantNames;

        assert_eq!(&ChainType::Mainnet.to_string(), "mainnet");
        assert_eq!(&ChainType::Testnet.to_string(), "testnet");

        for chain_type_str in ChainType::VARIANTS {
            let chain_type: ChainType = chain_type_str.parse().expect("cannot parse chain type");
            assert_eq!(&chain_type.to_string(), chain_type_str);
        }
    }
}
