//! Contains generated tonic code for peer to peer communication.
//!
//! Run main.rs to regenerate the code when updating .proto or bumping tonic/prost version.
mod groupcache_pb;

pub use groupcache_pb::groupcache_client::GroupcacheClient;
pub use groupcache_pb::groupcache_server::Groupcache;
use groupcache_pb::groupcache_server::GroupcacheServer as GroupcacheGRPCServer;
pub use groupcache_pb::{GetRequest, GetResponse, RemoveRequest, RemoveResponse};

/// gRPC server implementing groupcache GET to retrieve values.
pub type GroupcacheServer<T> = GroupcacheGRPCServer<T>;
