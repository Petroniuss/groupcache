use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Client operations (Maelstrom workload -> node)
// ---------------------------------------------------------------------------

/// Request to load a value into the cache for a given key.
#[derive(Debug, Deserialize)]
pub struct LoadRequest {
    pub key: String,
}

/// Successful response to a `load` request.
#[derive(Debug, Serialize)]
pub struct LoadResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub value: String,
}

impl LoadResponse {
    pub fn new(value: String) -> Self {
        Self {
            msg_type: "load_ok".to_string(),
            value,
        }
    }
}

/// Request to read a cached value for a given key.
#[derive(Debug, Deserialize)]
pub struct ReadRequest {
    pub key: String,
}

/// Successful response to a `read` request.
#[derive(Debug, Serialize)]
pub struct ReadResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub value: String,
}

impl ReadResponse {
    pub fn new(value: String) -> Self {
        Self {
            msg_type: "read_ok".to_string(),
            value,
        }
    }
}

/// Request to invalidate a cached entry for a given key.
#[derive(Debug, Deserialize)]
pub struct InvalidateRequest {
    pub key: String,
}

/// Successful response to an `invalidate` request.
#[derive(Debug, Serialize)]
pub struct InvalidateResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
}

impl Default for InvalidateResponse {
    fn default() -> Self {
        Self {
            msg_type: "invalidate_ok".to_string(),
        }
    }
}

impl InvalidateResponse {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Inter-node messages (node <-> node, groupcache protocol)
// ---------------------------------------------------------------------------

/// Request a value from a peer's local cache.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcGet {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
}

impl GcGet {
    pub fn new(key: String) -> Self {
        Self {
            msg_type: "gc_get".to_string(),
            key,
        }
    }
}

/// Response to `gc_get` — the value may be absent if the peer doesn't have it.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcGetOk {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
    pub value: Option<String>,
}

impl GcGetOk {
    pub fn new(key: String, value: Option<String>) -> Self {
        Self {
            msg_type: "gc_get_ok".to_string(),
            key,
            value,
        }
    }
}

/// Ask a peer to load (fetch from origin) a value for the given key.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcLoad {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
}

impl GcLoad {
    pub fn new(key: String) -> Self {
        Self {
            msg_type: "gc_load".to_string(),
            key,
        }
    }
}

/// Response to `gc_load` with the loaded value.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcLoadOk {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
    pub value: String,
}

impl GcLoadOk {
    pub fn new(key: String, value: String) -> Self {
        Self {
            msg_type: "gc_load_ok".to_string(),
            key,
            value,
        }
    }
}

/// Ask a peer to remove a key from its local cache.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcRemove {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
}

impl GcRemove {
    pub fn new(key: String) -> Self {
        Self {
            msg_type: "gc_remove".to_string(),
            key,
        }
    }
}

/// Acknowledgement that the key was removed.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcRemoveOk {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
}

impl GcRemoveOk {
    pub fn new(key: String) -> Self {
        Self {
            msg_type: "gc_remove_ok".to_string(),
            key,
        }
    }
}

/// Broadcast event telling peers that a key has been invalidated.
#[derive(Debug, Serialize, Deserialize)]
pub struct GcInvalidationEvent {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub key: String,
}

impl GcInvalidationEvent {
    pub fn new(key: String) -> Self {
        Self {
            msg_type: "gc_invalidation_event".to_string(),
            key,
        }
    }
}
