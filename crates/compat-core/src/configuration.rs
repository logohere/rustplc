use std::ffi::{c_char, c_void, CStr};
use std::slice;

mod bridge;

const MAX_RESOURCES: usize = 256;
const MAX_TASKS: usize = 4096;
const MAX_PROGRAMS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaskInstance {
    pub name: *const c_char,
    pub interval_ns: i64,
    pub priority: i32,
    pub programs: *mut *mut c_void,
    pub program_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResourceInstance {
    pub name: *const c_char,
    pub processor: *const c_char,
    pub tasks: *mut TaskInstance,
    pub task_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigurationError {
    Null,
    UnsupportedAbi,
    BridgeRejected,
    CppException,
    UnknownBridgeStatus(u8),
    CountTooLarge { kind: &'static str, count: usize },
    NullArray(&'static str),
    InvalidInterval(i64),
    InvalidProgram,
}

#[derive(Clone, Copy)]
pub struct ProgramInstance(*mut c_void);

// SAFETY: generated ProgramBase instances are owned by the loaded ProgramApi;
// RuntimeOwner assigns each instance to scan workers and serializes frame
// boundaries so the raw object is not concurrently mutated outside that model.
unsafe impl Send for ProgramInstance {}
// SAFETY: shared access is only used under RuntimeOwner's scheduler/frame
// synchronization; callers still require unsafe to invoke the C++ object.
unsafe impl Sync for ProgramInstance {}

impl ProgramInstance {
    /// Wrap a non-null generated-program object.
    ///
    /// # Safety
    /// `raw` must point to a live C++ `ProgramBase` object whose vtable remains
    /// valid for the wrapper lifetime.
    pub unsafe fn from_raw(raw: *mut c_void) -> Result<Self, ConfigurationError> {
        if raw.is_null() {
            return Err(ConfigurationError::InvalidProgram);
        }
        Ok(Self(raw))
    }

    /// Invoke the generated program's `run()` virtual method.
    ///
    /// # Errors
    /// Returns a bridge status when the C++ call fails or throws.
    ///
    /// # Safety
    /// The object must be live and use the pinned STruC++ vtable layout.
    pub unsafe fn run(self) -> Result<(), ConfigurationError> {
        bridge::program_run(self.0)
    }

    /// Read the generated program's located-image range.
    ///
    /// # Errors
    /// Returns a bridge status when the C++ call fails or throws.
    ///
    /// # Safety
    /// The object must be live and use the pinned STruC++ vtable layout.
    pub unsafe fn located_range(self) -> Result<(u32, u32), ConfigurationError> {
        let mut offset = 0;
        let mut count = 0;
        bridge::program_located_range(self.0, &mut offset, &mut count)?;
        Ok((offset, count))
    }
}

pub struct ConfigurationInstance {
    raw: *mut c_void,
}

// SAFETY: the configuration object is owned by the loaded ProgramApi and
// remains live while wrappers exist; runtime construction reads it before
// task execution and enforces bounded arrays/counts.
unsafe impl Send for ConfigurationInstance {}
// SAFETY: configuration metadata is read-only after generated initialization;
// mutable program execution is synchronized separately by RuntimeOwner.
unsafe impl Sync for ConfigurationInstance {}

impl ConfigurationInstance {
    /// Wrap a non-null generated configuration object.
    ///
    /// # Safety
    /// `raw` must point to a live C++ `ConfigurationInstance` using the pinned
    /// STruC++ ABI and must remain valid for the wrapper lifetime.
    pub unsafe fn from_raw(raw: *mut c_void) -> Result<Self, ConfigurationError> {
        if raw.is_null() {
            return Err(ConfigurationError::Null);
        }
        Ok(Self { raw })
    }

    /// Read the configuration name without taking ownership.
    ///
    /// # Errors
    /// Returns a bridge status when the C++ call fails or throws.
    ///
    /// # Safety
    /// The configuration object and returned C string must remain live.
    pub unsafe fn name(&self) -> Result<Option<&CStr>, ConfigurationError> {
        let mut name = std::ptr::null();
        bridge::configuration_name(self.raw, &mut name)?;
        Ok((!name.is_null()).then(|| CStr::from_ptr(name)))
    }

    /// Read the bounded resource array from the generated configuration.
    ///
    /// # Errors
    /// Returns a bridge status or invalid resource layout.
    ///
    /// # Safety
    /// The configuration object and resource array must remain live and match
    /// the pinned STruC++ layout.
    pub unsafe fn resources(&self) -> Result<&[ResourceInstance], ConfigurationError> {
        let mut count = 0;
        let mut resources = std::ptr::null_mut();
        bridge::configuration_resources(self.raw, &mut resources, &mut count)?;
        check_count("resources", count, MAX_RESOURCES)?;
        if count > 0 && resources.is_null() {
            return Err(ConfigurationError::NullArray("resources"));
        }
        Ok(slice::from_raw_parts(resources, count))
    }
}

/// Copy a bounded resource task array.
///
/// # Safety
/// `resource.tasks` must point to a live array with `task_count` entries using
/// the pinned STruC++ layout.
pub unsafe fn task_slice(
    resource: ResourceInstance,
) -> Result<Vec<TaskInstance>, ConfigurationError> {
    check_count("tasks", resource.task_count, MAX_TASKS)?;
    if resource.task_count > 0 && resource.tasks.is_null() {
        return Err(ConfigurationError::NullArray("tasks"));
    }
    Ok(slice::from_raw_parts(resource.tasks, resource.task_count).to_vec())
}

/// Validate and wrap a bounded program-pointer array.
///
/// # Safety
/// `task.programs` must point to a live array with `program_count` entries;
/// each object must remain live while its wrapper is used.
pub unsafe fn program_slice(
    task: TaskInstance,
) -> Result<Vec<ProgramInstance>, ConfigurationError> {
    check_count("programs", task.program_count, MAX_PROGRAMS)?;
    if task.program_count > 0 && task.programs.is_null() {
        return Err(ConfigurationError::NullArray("programs"));
    }
    (0..task.program_count)
        .map(|index| ProgramInstance::from_raw(*task.programs.add(index)))
        .collect()
}

fn check_count(kind: &'static str, count: usize, max: usize) -> Result<(), ConfigurationError> {
    (count <= max)
        .then_some(())
        .ok_or(ConfigurationError::CountTooLarge { kind, count })
}
