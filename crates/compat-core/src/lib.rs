//! Compatibility-port core. Runtime behavior remains incomplete and unverified for production.

pub mod abi;
pub mod command;
pub mod configuration;
pub mod debug;
pub mod debug_write;
pub mod image;
pub mod journal;
pub mod lifecycle;
pub mod loader;
pub mod located_globals;
pub mod located_image;
pub mod mode_switch;
pub mod native_plugin;
pub mod plugin_config;
pub mod plugin_runtime_args;
pub mod plugins;
pub mod program_api;
pub mod program_archive;
pub mod program_build;
pub mod runtime_owner;
pub mod runtime_service;
pub mod scan_runtime;
pub mod scheduler;
pub mod server;
pub mod socket;
pub mod stats;
pub mod timing;
pub mod transition;
pub mod watchdog;

/// State of the parity work for one feature area.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ParityStatus {
    #[default]
    NotStarted,
    InProgress,
    Verified,
    Blocked,
}

#[cfg(test)]
mod tests {
    use super::ParityStatus;

    #[test]
    fn new_features_start_unverified() {
        assert_eq!(ParityStatus::default(), ParityStatus::NotStarted);
    }
}
