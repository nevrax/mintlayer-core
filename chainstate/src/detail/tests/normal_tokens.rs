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

use super::anyonecanspend_address;
use crate::detail::tests::test_framework::{TestFramework, TransactionBuilder};
use crate::detail::transaction_verifier::error::ConnectTransactionError;
use crate::detail::CheckBlockTransactionsError;
use crate::{
    detail::{tests::TestBlockInfo, CheckBlockError, TokensError},
    BlockError, BlockSource,
};
use chainstate_types::BlockIndex;
use common::chain::block::BlockReward;
use common::chain::config::TOKEN_MIN_ISSUANCE_FEE;
use common::chain::Destination;
use common::{
    chain::{
        block::{timestamp::BlockTimestamp, Block, ConsensusData},
        signature::inputsig::InputWitness,
        tokens::{token_id, OutputValue, TokenData, TokenId},
        GenBlock, OutPointSourceId, OutputPurpose, Transaction, TxInput, TxOutput,
    },
    primitives::{time, Amount, Id, Idable},
};
// use rstest::rstest;
use std::vec;
// use test_utils::random::{make_seedable_rng, Seed};

enum ParentBlock {
    BestBlock,
    BlockId(Id<GenBlock>),
}

fn process_token(
    test_framework: &mut TestFramework,
    parent_block: ParentBlock,
    values: Vec<OutputValue>,
) -> Result<Option<BlockIndex>, BlockError> {
    process_token_ex(test_framework, vec![], parent_block, values).map(|(_block_id, result)| result)
}

fn is_token_burned(output_value: &OutputValue) -> bool {
    match output_value {
        OutputValue::Coin(_) => false,
        OutputValue::Token(token_data) => match token_data {
            TokenData::TokenTransferV1 {
                token_id: _,
                amount: _,
            } => false,
            TokenData::TokenIssuanceV1 {
                token_ticker: _,
                amount_to_issue: _,
                number_of_decimals: _,
                metadata_uri: _,
            } => false,
            TokenData::TokenBurnV1 {
                token_id: _,
                amount_to_burn: _,
            } => true,
        },
    }
}

fn process_token_ex(
    test_framework: &mut TestFramework,
    additional_inputs: Vec<TxInput>,
    parent_block: ParentBlock,
    values: Vec<OutputValue>,
) -> Result<(Block, Option<BlockIndex>), BlockError> {
    let receiver = anyonecanspend_address();
    let parent_block_id = match parent_block {
        ParentBlock::BestBlock => test_framework.best_block_id(),
        ParentBlock::BlockId(block_id) => block_id,
    };
    let test_block_info = TestBlockInfo::from_id(&test_framework.chainstate, parent_block_id);

    // Create a token issue transaction and block
    let mut inputs: Vec<TxInput> = test_block_info
        .txns
        .iter()
        .flat_map(|(outpoint_source_id, outputs)| {
            outputs
                .iter()
                .enumerate()
                .filter_map(|(output_index, output)| {
                    // We mustn't add burned inputs
                    (!is_token_burned(output.value())).then_some(TxInput::new(
                        outpoint_source_id.clone(),
                        output_index.try_into().unwrap(),
                        InputWitness::NoSignature(None),
                    ))
                })
                .collect::<Vec<TxInput>>()
        })
        .collect();

    inputs.extend(additional_inputs);

    let outputs: Vec<TxOutput> = values
        .into_iter()
        .map(|value| TxOutput::new(value, OutputPurpose::Transfer(receiver.clone())))
        .collect();

    let block = Block::new(
        vec![Transaction::new(0, inputs, outputs, 0).unwrap()],
        parent_block_id,
        BlockTimestamp::from_duration_since_epoch(time::get()),
        ConsensusData::None,
        BlockReward::new(Vec::new()),
    )
    .unwrap();

    // Process it
    test_framework
        .process_block(block.clone(), BlockSource::Local)
        .map(|result| (block, result))
}

// #[rstest]
// #[trace]
// #[case(Seed::from_entropy())]
// fn token_issue_test(#[case] seed: Seed) {
#[test]
fn token_issue_test() {
    utils::concurrency::model(move || {
        let mut tf = TestFramework::default();
        let outpoint_source_id = TestBlockInfo::from_genesis(tf.genesis()).txns[0].0.clone();

        // Ticker is too long
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: b"TRY TO USE THE LONG NAME".to_vec(),
                            amount_to_issue: Amount::from_atoms(52292852472),
                            number_of_decimals: 1,
                            metadata_uri: b"https://some_site.meta".to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorInvalidTicker(_, _)
                ))
            ))
        ));

        // Ticker doesn't exist
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: b"".to_vec(),
                            amount_to_issue: Amount::from_atoms(52292852472),
                            number_of_decimals: 1,
                            metadata_uri: b"https://some_site.meta".to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorInvalidTicker(_, _)
                ))
            ))
        ));

        // Ticker contain non alpha-numeric byte
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: "💖".as_bytes().to_vec(),
                            amount_to_issue: Amount::from_atoms(52292852472),
                            number_of_decimals: 1,
                            metadata_uri: b"https://some_site.meta".to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorInvalidTicker(_, _)
                ))
            ))
        ));

        // Issue amount is too low
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: "SOME".as_bytes().to_vec(),
                            amount_to_issue: Amount::from_atoms(0),
                            number_of_decimals: 1,
                            metadata_uri: b"https://some_site.meta".to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorIncorrectAmount(_, _)
                ))
            ))
        ));

        // Too many decimals
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: "SOME".as_bytes().to_vec(),
                            amount_to_issue: Amount::from_atoms(123456789),
                            number_of_decimals: 123,
                            metadata_uri: b"https://some_site.meta".to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorTooManyDecimals(_, _)
                ))
            ))
        ));

        // URI is too long
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenIssuanceV1 {
                            token_ticker: b"SOME".to_vec(),
                            amount_to_issue: Amount::from_atoms(52292852472),
                            number_of_decimals: 1,
                            metadata_uri: "https://some_site.meta".repeat(1024).as_bytes().to_vec(),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::IssueErrorIncorrectMetadataURI(_, _)
                ))
            ))
        ));

        // Valid case
        let output_value = OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52292852472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        });
        let block_index = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        outpoint_source_id,
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        output_value.clone(),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process()
            .unwrap()
            .unwrap();

        let block = tf.block(*block_index.block_id());
        assert_eq!(block.transactions()[0].outputs()[0].value(), &output_value);
    });
}

#[test]
fn token_transfer_test() {
    utils::concurrency::model(|| {
        let mut tf = TestFramework::default();
        let genesis_outpoint_id = TestBlockInfo::from_genesis(tf.genesis()).txns[0].0.clone();

        // Issue a new token
        let output_value = OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52_292_852_472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        });

        let block_index = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        genesis_outpoint_id,
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        output_value.clone(),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process()
            .unwrap()
            .unwrap();
        let block = tf.block(*block_index.block_id());
        let token_id = token_id(&block.transactions()[0]).unwrap();
        assert_eq!(block.transactions()[0].outputs()[0].value(), &output_value);
        let issuance_outpoint_id = TestBlockInfo::from_block(&block).txns[0].0.clone();

        // Try to transfer exceed amount
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        issuance_outpoint_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenTransferV1 {
                            token_id,
                            amount: Amount::from_atoms(987_654_321_123),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::AttemptToPrintMoney(_, _)
            ))
        ));

        // Try to transfer token with wrong id
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        issuance_outpoint_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenTransferV1 {
                            token_id: TokenId::random(),
                            amount: Amount::from_atoms(123456789),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::MissingOutputOrSpent
            ))
        ));

        // Try to transfer zero amount
        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        issuance_outpoint_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenTransferV1 {
                            token_id,
                            amount: Amount::from_atoms(0),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();

        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::TransferZeroTokens(_, _)
                ))
            ))
        ));

        // Valid case - Transfer tokens
        let _ = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        issuance_outpoint_id,
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        OutputValue::Token(TokenData::TokenTransferV1 {
                            token_id,
                            amount: Amount::from_atoms(123456789),
                        }),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process()
            .unwrap()
            .unwrap();
    })
}

#[test]
fn multiple_token_issuance_in_one_tx() {
    utils::concurrency::model(|| {
        let mut tf = TestFramework::default();
        let genesis_outpoint_id = TestBlockInfo::from_genesis(tf.genesis()).txns[0].0.clone();

        // Issue a couple of tokens
        let issuance_value = OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52292852472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        });

        let result = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        genesis_outpoint_id.clone(),
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        issuance_value.clone(),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .add_output(TxOutput::new(
                        issuance_value,
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process();
        assert!(matches!(
            result,
            Err(BlockError::CheckBlockFailed(
                CheckBlockError::CheckTransactionFailed(CheckBlockTransactionsError::TokensError(
                    TokensError::MultipleTokenIssuanceInTransaction(_, _)
                ))
            ))
        ));

        // Valid issuance
        let output_value = OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52292852472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        });
        let block_index = tf
            .make_block_builder()
            .add_transaction(
                TransactionBuilder::new()
                    .add_input(TxInput::new(
                        genesis_outpoint_id,
                        0,
                        InputWitness::NoSignature(None),
                    ))
                    .add_output(TxOutput::new(
                        output_value.clone(),
                        OutputPurpose::Transfer(Destination::AnyoneCanSpend),
                    ))
                    .build(),
            )
            .build_and_process()
            .unwrap()
            .unwrap();

        let block = tf.block(*block_index.block_id());
        assert_eq!(block.transactions()[0].outputs()[0].value(), &output_value);
    })
}

#[test]
fn token_issuance_with_insufficient_fee() {
    utils::concurrency::model(|| {
        let mut test_framework = TestFramework::default();

        let parent_block_id = test_framework.best_block_id();
        let test_block_info = TestBlockInfo::from_id(&test_framework.chainstate, parent_block_id);

        let receiver = anyonecanspend_address();
        let value = OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52292852472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        });
        // Create a token issue transaction and block
        let inputs = vec![TxInput::new(
            test_block_info.txns[0].0.clone(),
            0,
            InputWitness::NoSignature(None),
        )];

        let input_coins = match test_block_info.txns[0].1[0].value() {
            OutputValue::Coin(coin) => *coin,
            OutputValue::Token(_) => unreachable!(),
        };

        let outputs = vec![
            TxOutput::new(value, OutputPurpose::Transfer(receiver.clone())),
            TxOutput::new(
                OutputValue::Coin(input_coins),
                OutputPurpose::Transfer(receiver),
            ),
        ];
        let block = Block::new(
            vec![Transaction::new(0, inputs, outputs, 0).unwrap()],
            parent_block_id,
            BlockTimestamp::from_duration_since_epoch(time::get()),
            ConsensusData::None,
            BlockReward::new(Vec::new()),
        )
        .unwrap();

        // Try to process tx with insufficient token fees
        assert!(matches!(
            test_framework.process_block(block, BlockSource::Local),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::TokensError(TokensError::InsufficientTokenFees(_, _))
            ))
        ));

        // Valid issuance
        let values = vec![OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: Amount::from_atoms(52292852472),
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        })];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();
    })
}

#[test]
fn transfer_split_and_combine_tokens() {
    utils::concurrency::model(|| {
        const TOTAL_TOKEN_VALUE: Amount = Amount::from_atoms(52292852472);

        // Issue a new token
        let mut test_framework = TestFramework::default();
        let values = vec![OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: TOTAL_TOKEN_VALUE,
            number_of_decimals: 1,
            metadata_uri: b"https://52292852472.meta".to_vec(),
        })];
        let block_index =
            process_token(&mut test_framework, ParentBlock::BestBlock, values.clone())
                .unwrap()
                .unwrap();
        let block = test_framework.block(*block_index.block_id());
        assert_eq!(block.transactions()[0].outputs()[0].value(), &values[0]);
        let token_id = token_id(&block.transactions()[0]).unwrap();

        // Split tokens in outputs
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: (TOTAL_TOKEN_VALUE - Amount::from_atoms(123456)).unwrap(),
            }),
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: Amount::from_atoms(123456),
            }),
        ];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();

        // Collect these in one output
        let values = vec![OutputValue::Token(TokenData::TokenTransferV1 {
            token_id,
            amount: TOTAL_TOKEN_VALUE,
        })];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();
    })
}

#[test]
fn test_burn_tokens() {
    utils::concurrency::model(|| {
        const ISSUED_FUNDS: Amount = Amount::from_atoms(123456788);
        const HALF_ISSUED_FUNDS: Amount = Amount::from_atoms(61728394);
        const QUARTER_ISSUED_FUNDS: Amount = Amount::from_atoms(30864197);

        let mut test_framework = TestFramework::default();
        // Issue a new token
        let values = vec![OutputValue::Token(TokenData::TokenIssuanceV1 {
            token_ticker: b"SOME".to_vec(),
            amount_to_issue: ISSUED_FUNDS,
            number_of_decimals: 1,
            metadata_uri: b"https://some_site.meta".to_vec(),
        })];
        let block_index =
            process_token(&mut test_framework, ParentBlock::BestBlock, values.clone())
                .unwrap()
                .unwrap();
        let block = test_framework.block(*block_index.block_id());
        assert_eq!(block.transactions()[0].outputs()[0].value(), &values[0]);

        // Transfer it
        let token_id = token_id(&block.transactions()[0]).unwrap();
        let values = vec![OutputValue::Token(TokenData::TokenTransferV1 {
            token_id,
            amount: ISSUED_FUNDS,
        })];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();

        // Try burn more than we have in input
        let values = vec![OutputValue::Token(TokenData::TokenBurnV1 {
            token_id,
            amount_to_burn: (ISSUED_FUNDS * 2).unwrap(),
        })];
        assert!(matches!(
            process_token(&mut test_framework, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::AttemptToPrintMoney(_, _)
            ))
        ));

        // Valid case: Burn 25% through burn data, and burn 25% with just don't add output for them, and transfer the rest 50%
        let values = vec![
            OutputValue::Token(TokenData::TokenBurnV1 {
                token_id,
                amount_to_burn: QUARTER_ISSUED_FUNDS,
            }),
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: HALF_ISSUED_FUNDS,
            }),
        ];
        process_token(&mut test_framework, ParentBlock::BestBlock, values).unwrap();

        // Valid case: Burn 50% and 50% transfer
        let values = vec![
            OutputValue::Token(TokenData::TokenBurnV1 {
                token_id,
                amount_to_burn: QUARTER_ISSUED_FUNDS,
            }),
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: QUARTER_ISSUED_FUNDS,
            }),
        ];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();

        // Valid case: Try to burn the rest 50%
        let values = vec![OutputValue::Token(TokenData::TokenBurnV1 {
            token_id,
            amount_to_burn: QUARTER_ISSUED_FUNDS,
        })];
        let block_index = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();

        // Try to transfer burned tokens
        let outpoint_source_id = OutPointSourceId::from(*block_index.block_id());
        let inputs = vec![TxInput::new(outpoint_source_id, 0, InputWitness::NoSignature(None))];
        let values = vec![OutputValue::Token(TokenData::TokenTransferV1 {
            token_id,
            amount: Amount::from_atoms(123456789),
        })];
        assert!(matches!(
            process_token_ex(&mut test_framework, inputs, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::OutputIndexOutOfRange {
                    tx_id: _,
                    source_output_index: _
                }
            ))
        ));
    })
}

#[test]
fn test_reorg_and_try_to_double_spend_tokens() {
    //     B1 - C1 - D1
    //   /
    // A
    //   \
    //     B2 - C2
    //
    // Where in A, we issue a token, and it becomes part of the utxo-set.
    // Now assuming chain-trust per block is 1, it's obvious that D1 represents the tip.
    // Consider a case where B1 spends the issued token output. If a Block D2 was added
    // to this chain (whose previous block is C2), and D2 contains an input that also spends
    // B1, check that output is spent.

    utils::concurrency::model(|| {
        const ISSUED_FUNDS: Amount = Amount::from_atoms(1_000_000);

        // Issue a new token
        let mut test_framework = TestFramework::default();
        let values = vec![
            OutputValue::Token(TokenData::TokenIssuanceV1 {
                token_ticker: b"SOME".to_vec(),
                amount_to_issue: ISSUED_FUNDS,
                number_of_decimals: 1,
                metadata_uri: b"https://some_site.meta".to_vec(),
            }),
            OutputValue::Coin(Amount::from_atoms(123456)),
        ];
        let (block_a, _) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BestBlock,
            values.clone(),
        )
        .unwrap();
        assert_eq!(block_a.transactions()[0].outputs()[0].value(), &values[0]);
        let token_id = token_id(&block_a.transactions()[0]).unwrap();

        // B1 - burn all tokens in mainchain
        let values = vec![
            OutputValue::Token(TokenData::TokenBurnV1 {
                token_id,
                amount_to_burn: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123455)),
        ];
        let (block_b1, _) =
            process_token_ex(&mut test_framework, vec![], ParentBlock::BestBlock, values).unwrap();
        let _block_b1 = test_framework.block(block_b1.get_id());

        let spent_input = TxInput::new(
            OutPointSourceId::from(block_b1.transactions()[0].get_id()),
            0,
            InputWitness::NoSignature(None),
        );

        // Try to transfer spent tokens
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123454)),
        ];
        assert!(matches!(
            process_token(&mut test_framework, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::MissingOutputOrSpent
            ))
        ));

        // Let's add C1 and D1
        let values = vec![OutputValue::Coin(Amount::from_atoms(123453))];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();
        let values = vec![OutputValue::Coin(Amount::from_atoms(123452))];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();

        // Second chain - B2
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123454)),
        ];
        let (block_b2, block_index) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BlockId(Id::<GenBlock>::from(block_a.get_id())),
            values,
        )
        .unwrap();
        assert!(block_index.is_none(), "Reog is not allowed at this height");

        // C2 - burn all tokens in second chain
        let values = vec![
            OutputValue::Token(TokenData::TokenBurnV1 {
                token_id,
                amount_to_burn: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123453)),
        ];
        let (block_c2, block_index) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BlockId(Id::<GenBlock>::from(block_b2.get_id())),
            values,
        )
        .unwrap();
        assert!(block_index.is_none(), "Reog is not allowed at this height");

        // Now D2 trying to spend tokens from mainchain
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123454)),
        ];
        let (block_d2, block_index) = process_token_ex(
            &mut test_framework,
            vec![spent_input],
            ParentBlock::BlockId(Id::<GenBlock>::from(block_c2.get_id())),
            values,
        )
        .unwrap();
        assert!(block_index.is_none(), "Reog is not allowed at this height");

        // Block E2 will cause reorganization
        let values = vec![OutputValue::Coin(Amount::from_atoms(123453))];
        assert!(matches!(
            process_token_ex(
                &mut test_framework,
                vec![],
                ParentBlock::BlockId(Id::<GenBlock>::from(block_d2.get_id())),
                values
            ),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::MissingOutputOrSpent
            ))
        ));
    })
}

#[test]
fn test_attempt_to_print_tokens() {
    utils::concurrency::model(|| {
        const ISSUED_FUNDS: Amount = Amount::from_atoms(987_654_321);

        // Issue a new token
        let mut test_framework = TestFramework::default();
        let values = vec![
            OutputValue::Token(TokenData::TokenIssuanceV1 {
                token_ticker: b"SOME".to_vec(),
                amount_to_issue: ISSUED_FUNDS,
                number_of_decimals: 1,
                metadata_uri: b"https://some_site.meta".to_vec(),
            }),
            OutputValue::Coin(Amount::from_atoms(123456)),
        ];
        let (block_a, _) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BestBlock,
            values.clone(),
        )
        .unwrap();
        assert_eq!(block_a.transactions()[0].outputs()[0].value(), &values[0]);
        let token_id = token_id(&block_a.transactions()[0]).unwrap();

        // Try to transfer a bunch of outputs where each separately do not exceed input tokens value, but a sum of outputs larger than inputs.
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123454)),
        ];

        assert!(matches!(
            process_token(&mut test_framework, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::AttemptToPrintMoney(_, _)
            ))
        ));

        // Valid case - try to transfer correct amount of tokens
        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Coin(Amount::from_atoms(123454)),
        ];
        let _ = process_token(&mut test_framework, ParentBlock::BestBlock, values)
            .unwrap()
            .unwrap();
    });
}

#[test]
fn test_attempt_to_mix_input_tokens() {
    utils::concurrency::model(|| {
        const ISSUED_FUNDS: Amount = Amount::from_atoms(987_654_321);
        // Issuance a few different tokens
        let mut test_framework = TestFramework::default();
        let values = vec![
            OutputValue::Token(TokenData::TokenIssuanceV1 {
                token_ticker: b"SOME".to_vec(),
                amount_to_issue: ISSUED_FUNDS,
                number_of_decimals: 1,
                metadata_uri: b"https://some_site.meta".to_vec(),
            }),
            OutputValue::Coin((TOKEN_MIN_ISSUANCE_FEE * 2).unwrap()),
        ];
        let (block_a, _) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BestBlock,
            values.clone(),
        )
        .unwrap();
        assert_eq!(block_a.transactions()[0].outputs()[0].value(), &values[0]);
        let first_token_id = token_id(&block_a.transactions()[0]).unwrap();

        let values = vec![
            OutputValue::Token(TokenData::TokenTransferV1 {
                token_id: first_token_id,
                amount: ISSUED_FUNDS,
            }),
            OutputValue::Token(TokenData::TokenIssuanceV1 {
                token_ticker: b"SOME".to_vec(),
                amount_to_issue: ISSUED_FUNDS,
                number_of_decimals: 1,
                metadata_uri: b"https://123.meta".to_vec(),
            }),
            OutputValue::Coin(TOKEN_MIN_ISSUANCE_FEE),
        ];
        let (block_a, _) = process_token_ex(
            &mut test_framework,
            vec![],
            ParentBlock::BestBlock,
            values.clone(),
        )
        .unwrap();
        assert_eq!(block_a.transactions()[0].outputs()[0].value(), &values[0]);
        let second_token_id = token_id(&block_a.transactions()[0]).unwrap();

        // Try to spend sum of input tokens
        let values = vec![OutputValue::Token(TokenData::TokenTransferV1 {
            token_id: first_token_id,
            amount: (ISSUED_FUNDS * 2).unwrap(),
        })];

        assert!(matches!(
            process_token(&mut test_framework, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::AttemptToPrintMoney(_, _)
            ))
        ));

        let values = vec![OutputValue::Token(TokenData::TokenTransferV1 {
            token_id: second_token_id,
            amount: (ISSUED_FUNDS * 2).unwrap(),
        })];

        assert!(matches!(
            process_token(&mut test_framework, ParentBlock::BestBlock, values),
            Err(BlockError::StateUpdateFailed(
                ConnectTransactionError::AttemptToPrintMoney(_, _)
            ))
        ));
    })
}

#[test]
fn test_tokens_storage() {
    utils::concurrency::model(|| {
        // TODO: Test tokens records in the storage before and after token issuance, also after reorg
    })
}

#[test]
fn snapshot_testing_tokens_data() {
    utils::concurrency::model(|| {
        // TODO: Add tests, that will prevent change fields order
    })
}

//TODO: Due to much change in Test Framework, this file should be updated according to new features like TxBuilder
