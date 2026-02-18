//! Plugin system for Bark file manager
//!
//! Supports multiple types of plugins:
//! - Native plugins: Shared libraries (.so/.dll/.dylib) using C ABI
//! - Script plugins: External executables communicating via JSON over stdin/stdout
//! - Provider plugins: Filesystem providers (FTP, S3, GDrive, etc.)

mod api;
mod manager;
pub mod overlay_script;
pub mod provider_api;
pub mod provider_script;
mod script;

#[allow(unused_imports)]
pub use api::{OverlayPluginInfo, OverlayRenderResult};
pub use api::{StatusContext, ViewerContext};
pub use manager::PluginManager;

// Re-export types from the plugin API crate for external use
#[allow(unused_imports)]
pub use bark_plugin_api::{
    DialogField, DialogFieldType, FileEntry as PluginFileEntry, ProviderConfig, ProviderError,
    ProviderPlugin, ProviderPluginInfo, ProviderResult, ProviderSession,
};

// Re-export the adapter for bridging plugin sessions to panel providers
#[allow(unused_imports)]
pub use provider_api::PluginProviderAdapter;
