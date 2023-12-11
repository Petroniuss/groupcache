#![doc = include_str!("../readme.md")]

mod errors;
mod groupcache;
pub mod groupcache_builder;
mod groupcache_inner;
mod http;
pub mod metrics;
mod options;
mod routing;

pub use groupcache::{Groupcache, GroupcachePeer, ValueBounds, ValueLoader};
pub use groupcache_inner::GroupcacheInner;
pub use groupcache_pb::GroupcacheServer;
