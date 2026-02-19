#![allow(unsafe_code)]

pub mod ffi;
pub mod client;
pub mod auth;
pub mod adapter;

pub use adapter::TdlibAdapter;
pub use auth::AuthConfig;
pub use client::TdClient;
