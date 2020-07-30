use crate::core::{
    workflow::produce_dht_ops_workflow::dht_op_light::error::DhtOpConvertError, SourceChainError,
};
use holochain_p2p::HolochainP2pError;
use holochain_serialized_bytes::SerializedBytesError;
use holochain_state::error::DatabaseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CascadeError {
    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error(transparent)]
    DhtOpConvertError(#[from] DhtOpConvertError),

    #[error(transparent)]
    SourceChainError(#[from] SourceChainError),

    #[error(transparent)]
    NetworkError(#[from] HolochainP2pError),

    #[error(transparent)]
    SerializedBytesError(#[from] SerializedBytesError),
}

pub type CascadeResult<T> = Result<T, CascadeError>;
