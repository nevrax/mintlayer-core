use super::*;

#[derive(Debug, PartialEq, Eq, Clone)]
pub(in crate::pool::tests) struct ValuedOutPoint {
    pub(in crate::pool::tests) outpoint: OutPoint,
    pub(in crate::pool::tests) value: Amount,
}

impl std::cmp::PartialOrd for ValuedOutPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.value.partial_cmp(&self.value)
    }
}

impl std::cmp::Ord for ValuedOutPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.value.cmp(&self.value)
    }
}

fn dummy_input() -> TxInput {
    let outpoint_source_id = OutPointSourceId::Transaction(Id::new(H256::zero()));
    let output_index = 0;
    let witness = DUMMY_WITNESS_MSG.to_vec();
    TxInput::new(
        outpoint_source_id,
        output_index,
        InputWitness::NoSignature(Some(witness)),
    )
}

fn dummy_output() -> TxOutput {
    let value = Amount::from_atoms(0);
    let purpose = OutputPurpose::Transfer(Destination::AnyoneCanSpend);
    TxOutput::new(OutputValue::Coin(value), purpose)
}

pub(in crate::pool::tests) fn estimate_tx_size(num_inputs: usize, num_outputs: usize) -> usize {
    let inputs = (0..num_inputs).into_iter().map(|_| dummy_input()).collect();
    let outputs = (0..num_outputs).into_iter().map(|_| dummy_output()).collect();
    let flags = 0;
    let locktime = 0;
    let size = Transaction::new(flags, inputs, outputs, locktime).unwrap().encoded_size();
    // Take twice the encoded size of the dummy tx.Real Txs are larger than these dummy ones,
    // but taking 3 times the size seems to ensure our txs won't fail the minimum relay fee
    // validation (see the function `pays_minimum_relay_fees`)
    let result = 3 * size;
    log::debug!(
        "estimated size for tx with {} inputs and {} outputs: {}",
        num_inputs,
        num_outputs,
        result
    );
    result
}
