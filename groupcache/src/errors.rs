use rmp_serde::decode::Error;
use std::sync::Arc;
use tonic::Status;

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct GroupcacheError {
    #[from]
    error: InternalGroupcacheError,
}

#[derive(thiserror::Error, Debug, Clone)]
#[error(transparent)]
pub(crate) struct DedupedGroupcacheError(pub(crate) Arc<InternalGroupcacheError>);

#[derive(thiserror::Error, Debug)]
pub(crate) enum InternalGroupcacheError {
    #[error("Loading error: '{}'", .0)]
    LocalLoader(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Transport error: '{}'", .0.message())]
    Transport(#[from] Status),

    #[error(transparent)]
    Rmp(#[from] Error),

    #[error(transparent)]
    Deduped(#[from] DedupedGroupcacheError),

    #[error(transparent)]
    CatchAll(#[from] anyhow::Error),
}
