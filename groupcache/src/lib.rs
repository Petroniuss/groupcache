// todo: probably include markdown.md here
// possibly add

mod errors;
mod groupcache;
mod groupcache_inner;
mod http;
mod metrics;
mod options;
mod routing;

pub use groupcache::{Groupcache, GroupcachePeer, ValueBounds, ValueLoader};
pub use groupcache_inner::GroupcacheInner;
pub use groupcache_pb::GroupcacheServer;
pub use options::{Options, OptionsBuilder};
