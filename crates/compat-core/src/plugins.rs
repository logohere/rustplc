use crate::plugin_config::PluginConfig;

pub const MAX_PLUGINS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginPhase {
    Unloaded,
    Initialized,
    Running,
    Degraded,
}

pub trait PluginBackend {
    fn init(&mut self, config: &PluginConfig) -> Result<(), String>;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn cleanup(&mut self);
    fn cycle_start(&mut self) {}
    fn cycle_end(&mut self) {}
    fn execute_command(&mut self, _: &str, _: usize) -> Option<Result<String, String>> {
        None
    }
}

pub struct PluginEntry {
    pub config: PluginConfig,
    pub phase: PluginPhase,
    backend: Box<dyn PluginBackend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    TooManyPlugins,
    NotInitialized(String),
    InitFailed { name: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartReport {
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StopReport {
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCommandError {
    InvalidArguments,
    NotFound(String),
    Unavailable(String),
    Unsupported(String),
    Failed(String),
    ResponseTooLarge,
}

pub struct PluginManager {
    entries: Vec<PluginEntry>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        config: PluginConfig,
        backend: Box<dyn PluginBackend>,
    ) -> Result<(), PluginError> {
        if self.entries.len() == MAX_PLUGINS {
            return Err(PluginError::TooManyPlugins);
        }
        self.entries.push(PluginEntry {
            config,
            phase: PluginPhase::Unloaded,
            backend,
        });
        Ok(())
    }

    pub fn entries(&self) -> &[PluginEntry] {
        &self.entries
    }

    pub fn initialize_all(&mut self) -> Result<(), PluginError> {
        for index in 0..self.entries.len() {
            let entry = &mut self.entries[index];
            if let Err(reason) = entry.backend.init(&entry.config) {
                let name = entry.config.name.clone();
                self.cleanup_initialized();
                return Err(PluginError::InitFailed { name, reason });
            }
            entry.phase = PluginPhase::Initialized;
        }
        Ok(())
    }

    pub fn start_enabled(&mut self) -> Result<StartReport, PluginError> {
        let mut report = StartReport::default();
        for entry in &mut self.entries {
            if entry.config.enabled == 0 {
                continue;
            }
            if entry.phase != PluginPhase::Initialized {
                return Err(PluginError::NotInitialized(entry.config.name.clone()));
            }
            if entry.backend.start().is_ok() {
                entry.phase = PluginPhase::Running;
            } else {
                report.failed.push(entry.config.name.clone());
            }
        }
        Ok(report)
    }

    pub fn stop_running(&mut self) -> StopReport {
        let mut report = StopReport::default();
        for entry in &mut self.entries {
            if entry.phase != PluginPhase::Running {
                continue;
            }
            if entry.backend.stop().is_err() {
                report.failed.push(entry.config.name.clone());
            }
            entry.phase = PluginPhase::Initialized;
        }
        report
    }

    pub fn cycle_start(&mut self) {
        for entry in &mut self.entries {
            if entry.phase == PluginPhase::Running {
                entry.backend.cycle_start();
            }
        }
    }

    pub fn cycle_end(&mut self) {
        for entry in &mut self.entries {
            if entry.phase == PluginPhase::Running {
                entry.backend.cycle_end();
            }
        }
    }

    pub fn execute_command(
        &mut self,
        plugin_name: &str,
        command_json: &str,
        response_size: usize,
    ) -> Result<String, PluginCommandError> {
        if plugin_name.is_empty() || command_json.is_empty() || response_size == 0 {
            return Err(PluginCommandError::InvalidArguments);
        }
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.config.name == plugin_name)
        else {
            return Err(PluginCommandError::NotFound(plugin_name.to_owned()));
        };
        if entry.phase == PluginPhase::Degraded {
            return Err(PluginCommandError::Unavailable(plugin_name.to_owned()));
        }
        match entry.backend.execute_command(command_json, response_size) {
            Some(Ok(response)) if response.len() < response_size => Ok(response),
            Some(Ok(_)) => Err(PluginCommandError::ResponseTooLarge),
            Some(Err(reason)) => Err(PluginCommandError::Failed(reason)),
            None => Err(PluginCommandError::Unsupported(plugin_name.to_owned())),
        }
    }

    pub fn cleanup_initialized(&mut self) -> usize {
        let mut cleaned = 0;
        for entry in self.entries.iter_mut().rev() {
            if matches!(
                entry.phase,
                PluginPhase::Initialized | PluginPhase::Degraded
            ) {
                entry.backend.cleanup();
                entry.phase = PluginPhase::Unloaded;
                cleaned += 1;
            }
        }
        cleaned
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
