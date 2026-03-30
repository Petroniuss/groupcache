use rmp_serde::decode::Error as RmpError;
use std::sync::Arc;
use tonic::Status;

/// Error type for groupcache operations.
///
/// Variants are public so callers can match on error causes to distinguish
/// loader failures from network issues, deserialization problems, etc.
#[derive(thiserror::Error, Debug)]
pub enum GroupcacheError {
    /// The [`ValueLoader`](crate::ValueLoader) returned an error.
    #[error("Loading error: '{0}'")]
    Loading(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// gRPC transport error when communicating with a peer.
    #[error("Transport error: '{}'", .0.message())]
    Transport(Status),

    /// Failed to deserialize a value received from a peer.
    #[error("Deserialization error: '{0}'")]
    Deserialization(RmpError),

    /// Failed to establish a connection to one or more peers.
    #[error("Connection error: '{0}'")]
    Connection(tonic::transport::Error),

    /// Multiple connection errors when updating the peer set.
    #[error("Connection errors: {0:?}")]
    ConnectionErrors(Vec<GroupcacheError>),

    /// A peer returned an empty response.
    #[error("Peer returned empty value for key '{0}'")]
    EmptyResponse(String),

    /// Internal or unexpected error.
    #[error(transparent)]
    Internal(anyhow::Error),
}

impl From<InternalGroupcacheError> for GroupcacheError {
    fn from(err: InternalGroupcacheError) -> Self {
        match err {
            InternalGroupcacheError::LocalLoader(e) => GroupcacheError::Loading(e),
            InternalGroupcacheError::Transport(e) => GroupcacheError::Transport(e),
            InternalGroupcacheError::Rmp(e) => GroupcacheError::Deserialization(e),
            InternalGroupcacheError::Connection(e) => GroupcacheError::Connection(e),
            InternalGroupcacheError::EmptyResponse(key) => GroupcacheError::EmptyResponse(key),
            InternalGroupcacheError::ConnectionErrors(errs) => {
                GroupcacheError::ConnectionErrors(errs.into_iter().map(Into::into).collect())
            }
            InternalGroupcacheError::Anyhow(e) => GroupcacheError::Internal(e),
            InternalGroupcacheError::Deduped(DedupedGroupcacheError(arc)) => {
                // Try to unwrap the Arc to preserve the original error variant.
                // Falls back to string representation if other references exist
                // (e.g. singleflight distributed the error to multiple callers).
                match Arc::try_unwrap(arc) {
                    Ok(inner) => inner.into(),
                    Err(arc) => GroupcacheError::Internal(anyhow::anyhow!("{}", arc)),
                }
            }
        }
    }
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
    Rmp(#[from] RmpError),

    #[error(transparent)]
    Deduped(#[from] DedupedGroupcacheError),

    #[error(transparent)]
    Connection(#[from] tonic::transport::Error),

    #[error("Connection errors: {0:?}")]
    ConnectionErrors(Vec<InternalGroupcacheError>),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Peer returned empty value for key '{0}'")]
    EmptyResponse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_conversion() {
        let status = Status::unavailable("peer down");
        let internal = InternalGroupcacheError::Transport(status);
        let public: GroupcacheError = internal.into();
        assert!(matches!(public, GroupcacheError::Transport(_)));
    }

    #[test]
    fn rmp_deserialization_error_conversion() {
        let rmp_err: RmpError = rmp_serde::from_slice::<String>(&[0xFF]).unwrap_err();
        let internal = InternalGroupcacheError::Rmp(rmp_err);
        let public: GroupcacheError = internal.into();
        assert!(matches!(public, GroupcacheError::Deserialization(_)));
    }

    #[test]
    fn empty_response_error_conversion() {
        let internal = InternalGroupcacheError::EmptyResponse("some-key".to_string());
        let public: GroupcacheError = internal.into();
        assert!(matches!(public, GroupcacheError::EmptyResponse(_)));
    }

    #[test]
    fn anyhow_error_conversion() {
        let internal = InternalGroupcacheError::Anyhow(anyhow::anyhow!("unexpected"));
        let public: GroupcacheError = internal.into();
        assert!(matches!(public, GroupcacheError::Internal(_)));
    }

    #[test]
    fn deduped_error_fallback_when_arc_shared() {
        let inner = Arc::new(InternalGroupcacheError::EmptyResponse("key".to_string()));
        let _extra_ref = inner.clone(); // keep a second reference so try_unwrap fails
        let internal = InternalGroupcacheError::Deduped(DedupedGroupcacheError(inner));
        let public: GroupcacheError = internal.into();
        assert!(matches!(public, GroupcacheError::Internal(_)));
    }
}
