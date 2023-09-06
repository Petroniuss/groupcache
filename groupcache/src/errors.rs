use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct GroupcacheError {
    pub(crate) error: InternalGroupcacheError,
}

impl From<InternalGroupcacheError> for GroupcacheError {
    fn from(value: InternalGroupcacheError) -> Self {
        GroupcacheError { error: value }
    }
}

#[derive(thiserror::Error, Debug, Clone)]
#[error(transparent)]
pub(crate) struct DedupedGroupcacheError(pub(crate) Arc<anyhow::Error>);

#[derive(thiserror::Error, Debug)]
pub(crate) enum InternalGroupcacheError {
    #[error(transparent)]
    Loader(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error(transparent)]
    Server(#[from] anyhow::Error),

    #[error(transparent)]
    Deduped(#[from] DedupedGroupcacheError),
}
