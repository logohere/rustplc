//! OpenPLC command-socket protocol over the lifecycle-owned runtime.

use crate::debug::DebugProtocol;
use crate::lifecycle::{Lifecycle, PlcState, StateError};
use crate::mode_switch::{ModeSwitch, SwitchPosition};
use crate::runtime_service::{BeginResult, RuntimeService};
use crate::stats::StatsRegistry;

/// Command handler for the commands owned by `unix_socket.c`.
pub struct CommandHandler {
    service: RuntimeService,
}

impl CommandHandler {
    /// Create a handler for one pinned generated-program library.
    ///
    /// # Safety
    /// The path must always name a library implementing the pinned generated ABI.
    pub unsafe fn new(program_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            // SAFETY: forwarded from this constructor's caller contract.
            service: unsafe { RuntimeService::new(program_path) },
        }
    }

    /// Access the state arbiter to complete a claimed transition.
    pub fn lifecycle(&self) -> &Lifecycle {
        self.service.owner().lifecycle()
    }

    pub fn mode_switch(&self) -> &ModeSwitch {
        self.service.owner().mode_switch()
    }

    pub fn stats(&self) -> &StatsRegistry {
        self.service.owner().stats()
    }

    pub fn debug(&self) -> &DebugProtocol {
        self.service.owner().debug()
    }

    /// Set the mode-switch gate reported by a plugin.
    pub fn set_switch_allows_run(&self, allows_run: bool) {
        self.mode_switch().set_position(if allows_run {
            SwitchPosition::Run
        } else {
            SwitchPosition::Stop
        });
    }

    fn switch_response(&self) -> String {
        if self.mode_switch().allows_run() {
            "SWITCH:RUN\n".to_owned()
        } else {
            "SWITCH:STOP\n".to_owned()
        }
    }

    fn transition_response(&self, command: &str) -> Result<String, StateError> {
        let response = match command {
            "PING" => "PING:OK\n".to_owned(),
            "STATUS" => self.lifecycle().state()?.status_response().to_owned(),
            "SWITCH" => self.switch_response(),
            _ => "COMMAND:BUSY\n".to_owned(),
        };
        Ok(response)
    }

    fn start_response(&self) -> Result<String, StateError> {
        Ok(match self.service.begin(PlcState::Running)? {
            BeginResult::Accepted => "START:OK\n",
            BeginResult::RejectedSwitch => "START:ERROR_SWITCH_STOP\n",
            BeginResult::AlreadyRunning => "START:ERROR_ALREADY_RUNNING\n",
            BeginResult::Rejected => "START:ERROR\n",
        }
        .to_owned())
    }

    fn stop_response(&self) -> Result<String, StateError> {
        Ok(match self.service.begin(PlcState::Stopped)? {
            BeginResult::Accepted => "STOP:OK\n",
            _ => "STOP:ERROR\n",
        }
        .to_owned())
    }

    /// Handle one newline-free command and return the exact socket response.
    pub fn handle(&self, command: &str) -> Result<String, StateError> {
        let command = command.strip_suffix('\r').unwrap_or(command);
        if self.lifecycle().is_transitioning()? {
            return self.transition_response(command);
        }
        match command {
            "PING" => Ok("PING:OK\n".to_owned()),
            "STATUS" => Ok(self.lifecycle().state()?.status_response().to_owned()),
            "SWITCH" => Ok(self.switch_response()),
            "STATS" => Ok(self.stats().response()),
            command if command.starts_with("DEBUG:") => {
                Ok(self.debug().text_response(&command[6..]))
            }
            "START" => self.start_response(),
            "STOP" => self.stop_response(),
            _ => Ok("COMMAND:ERROR\n".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandHandler, PlcState};

    #[test]
    fn mirrors_start_stop_and_busy_responses() {
        // SAFETY: this test never starts or loads the placeholder path.
        let handler = unsafe { CommandHandler::new("unused") };
        assert_eq!(handler.handle("STATUS").unwrap(), "STATUS:STOPPED\n");
        assert!(handler
            .lifecycle()
            .claim_transition(PlcState::Running)
            .unwrap());
        assert_eq!(handler.handle("STATUS").unwrap(), "STATUS:TRANSITIONING\n");
        assert_eq!(handler.handle("STOP").unwrap(), "COMMAND:BUSY\n");
        handler
            .lifecycle()
            .publish_final_state(PlcState::Stopped)
            .unwrap();
        assert_eq!(handler.handle("STATUS").unwrap(), "STATUS:STOPPED\n");
    }

    #[test]
    fn enforces_switch_gate_and_unknown_command() {
        // SAFETY: the STOP switch rejects start before the path is loaded.
        let handler = unsafe { CommandHandler::new("unused") };
        assert_eq!(handler.handle("STATS").unwrap(), "STATS:{\"tasks\":[]}\n");
        assert_eq!(handler.handle("DEBUG:41").unwrap(), "DEBUG:41 83\n");
        handler.set_switch_allows_run(false);
        assert_eq!(handler.handle("SWITCH").unwrap(), "SWITCH:STOP\n");
        assert_eq!(
            handler.handle("START").unwrap(),
            "START:ERROR_SWITCH_STOP\n"
        );
        assert_eq!(handler.handle("NOPE").unwrap(), "COMMAND:ERROR\n");
    }
}
