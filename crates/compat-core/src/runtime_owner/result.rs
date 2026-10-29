use crate::configuration::ConfigurationError;
use crate::debug_write::QueueError as DebugQueueError;
use crate::lifecycle::StateError;
use crate::loader::LoadError;
use crate::located_image::LocatedImageError;
use crate::program_api::ProgramStateError;
use crate::scan_runtime::RuntimeError;
use crate::timing::TimingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartResult {
    Started,
    RejectedSwitch,
    AlreadyRunning,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopResult {
    Stopped,
    NotRunning,
}

#[derive(Debug)]
pub enum RuntimeOwnerError {
    State(StateError),
    Runtime(RuntimeError),
    Configuration(ConfigurationError),
    Image(LocatedImageError),
    Load(LoadError),
    Debug(DebugQueueError),
    Timing(TimingError),
    ProgramState(ProgramStateError),
    Occupied,
}
