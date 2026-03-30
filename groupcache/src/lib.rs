#![doc = include_str!("../readme.md")]

mod codec;
pub mod discovery;
mod errors;
mod groupcache;
mod groupcache_builder;
mod groupcache_inner;
mod http;
mod invalidation;
pub mod metrics;
mod options;
mod routing;
mod service_discovery;
pub mod status;

pub use groupcache::{Groupcache, GroupcachePeer, ValueBounds, ValueLoader};
pub use groupcache_builder::{CacheKey, GroupcacheBuilder};
pub use groupcache_inner::GroupcacheInner;
pub use groupcache_pb::GroupcacheServer;
pub use service_discovery::ServiceDiscovery;

/// we expose [`moka`](https://crates.io/crates/moka) since it's used in the public api of the library.
pub use moka;

/// Re-export [`CancellationToken`](tokio_util::sync::CancellationToken) since it's
/// required by [`Groupcache::builder`].
pub use tokio_util::sync::CancellationToken;
