//! # Common library
//!
//! This crate contains commonly used utility functions and a shared [`Config`] struct
//! used to configurate services within the workspace.
//!
//! [`Config`]: config::Config

/// Shared workspace configuration.
pub mod config;

/// Hash utilities.
pub mod hash;

/// Logging utilities.
#[cfg(feature = "logging")]
pub mod logging;

/// AWS S3-compatible storage wrapper.
#[cfg(feature = "s3")]
pub mod s3;

/// Substrate node RPC utilities.
#[cfg(feature = "rpc")]
pub mod rpc;
