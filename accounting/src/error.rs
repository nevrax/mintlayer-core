#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum Error {
    #[error("Accounting storage error")]
    StorageError(#[from] chainstate_types::storage_result::Error),
    #[error("Pool already exists by balance")]
    InvariantErrorPoolBalanceAlreadyExists,
    #[error("Pool already exists by data")]
    InvariantErrorPoolDataAlreadyExists,
    #[error("Attempted to decommission a non-existing pool balance")]
    AttemptedDecommissionNonexistingPoolBalance,
    #[error("Attempted to decommission a non-existing pool data")]
    AttemptedDecommissionNonexistingPoolData,
    #[error("Failed to create a delegation because the target pool doesn't exist")]
    DelegationCreationFailedPoolDoesNotExist,
    #[error("Failed to create a delegation because the resulting id already exists")]
    InvariantErrorDelegationCreationFailedIdAlreadyExists,
    #[error("Delegate to a non-existing reward id")]
    DelegateToNonexistingId,
    #[error("Delegate to a non-existing pool")]
    DelegateToNonexistingPool,
    #[error("Delegation arithmetic error")]
    DelegationBalanceAdditionError,
    #[error("Pool balance arithmetic error")]
    PoolBalanceAdditionError,
    #[error("Delegation shares arithmetic error")]
    DelegationSharesAdditionError,
    #[error("Pool creation undo failed; pool balance cannot be found")]
    InvariantErrorPoolCreationReversalFailedBalanceNotFound,
    #[error("Pool creation undo failed; pool balance cannot be found")]
    InvariantErrorPoolCreationReversalFailedDataNotFound,
    #[error("Pledge amount has changed while reversal being done")]
    InvariantErrorPoolCreationReversalFailedAmountChanged,
    #[error("Undo failed as decommission pool balance is still in storage")]
    InvariantErrorDecommissionUndoFailedPoolBalanceAlreadyExists,
    #[error("Undo failed as decommission pool data is still in storage")]
    InvariantErrorDecommissionUndoFailedPoolDataAlreadyExists,
    #[error("Reversal of delegation id creation failed; not found")]
    InvariantErrorDelegationIdUndoFailedNotFound,
    #[error("Reversal of delegation id creation failed; data changed")]
    InvariantErrorDelegationIdUndoFailedDataConflict,
    #[error("Delegation balance arithmetic undo error")]
    InvariantErrorDelegationBalanceAdditionUndoError,
    #[error("Pool balance arithmetic undo error")]
    InvariantErrorPoolBalanceAdditionUndoError,
    #[error("Delegation shares arithmetic undo error")]
    InvariantErrorDelegationSharesAdditionUndoError,
    #[error("Delegation shares arithmetic undo error as it doesn't exist")]
    InvariantErrorDelegationShareNotFound,
    #[error("Arithemetic error; conversion to unsigned failed")]
    ArithmeticErrorToUnsignedFailed,
    #[error("Arithemetic error; conversion to signed failed")]
    ArithmeticErrorToSignedFailed,
    #[error("Arithemetic error; delta signed addition failed")]
    ArithmeticErrorDeltaAdditionFailed,
    #[error("Arithemetic error; sum to unsigned failed")]
    ArithmeticErrorSumToUnsignedFailed,
    #[error("Arithemetic error; sum to signed failed")]
    ArithmeticErrorSumToSignedFailed,
    #[error("Consencutive pool creation")]
    PoolCreatedMultipleTimes,
    #[error("Consencutive pool decommission")]
    PoolDecommissionedMultipleTimes,
    #[error("Consencutive delegation data creation")]
    DelegationDataCreatedMultipleTimes,
    #[error("Consencutive delegation data deletion")]
    DelegationDataDeletedMultipleTimes,
}
