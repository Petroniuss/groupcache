//! Built-in service discovery implementations.
//!
//! Enable via feature flags:
//! - `kubernetes` — discover peers via Kubernetes API server
//! - `consul` — discover peers via Consul health API
//!
//! DNS discovery is always available (no extra dependencies).

pub mod dns;

#[cfg(feature = "consul")]
pub mod consul;

#[cfg(feature = "kubernetes")]
pub mod kubernetes;
