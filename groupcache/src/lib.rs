#![doc = include_str!("../readme.md")]

mod errors;
mod groupcache;
mod groupcache_builder;
mod groupcache_inner;
mod http;
pub mod metrics;
mod options;
mod routing;
mod service_discovery;

pub use groupcache::{Groupcache, GroupcachePeer, ValueBounds, ValueLoader};
pub use groupcache_builder::GroupcacheBuilder;
pub use groupcache_inner::GroupcacheInner;
pub use groupcache_pb::GroupcacheServer;
pub use service_discovery::{ServiceDiscovery, ServiceDiscoveryError};
