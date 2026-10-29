use std::ffi::CStr;
use std::sync::Arc;

use crate::configuration::{
    program_slice, task_slice, ConfigurationError, ConfigurationInstance, TaskInstance,
};
use crate::located_image::{LocatedImage, LocatedImageError};
use crate::scan_runtime::{RuntimeTask, RuntimeTaskFault};
use crate::scheduler::TaskSpec;

use super::ProgramApi;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramScanError {
    Configuration(ConfigurationError),
    Image(LocatedImageError),
}

impl ProgramApi {
    /// Bind runtime-owned image storage to the generated program descriptors.
    ///
    /// # Safety
    /// The loaded library must implement the pinned STruC++ ABI.
    pub unsafe fn located_image(self: &Arc<Self>) -> Result<Arc<LocatedImage>, LocatedImageError> {
        LocatedImage::from_program(
            Arc::clone(self),
            (self.get_located_vars)(),
            (self.get_located_var_count)(),
            self.located_globals(),
        )
    }

    /// Build scan tasks with the pinned copy-in, run, copy-out ordering.
    ///
    /// # Safety
    /// The loaded library must implement the pinned STruC++ ABI. Program task
    /// ranges must not overlap writable program-local storage.
    pub unsafe fn runtime_tasks(
        self: &Arc<Self>,
        image: Arc<LocatedImage>,
    ) -> Result<Vec<RuntimeTask>, ConfigurationError> {
        let configuration = ConfigurationInstance::from_raw((self.get_config)())?;
        let mut tasks = Vec::new();
        for resource in configuration.resources()? {
            for task in task_slice(*resource)? {
                tasks.push(build_task(self, &image, task, tasks.len())?);
            }
        }
        Ok(tasks)
    }

    /// Execute one generated task with located-image copy ordering.
    ///
    /// # Errors
    /// Returns a configuration/bridge or located-image failure.
    ///
    /// # Safety
    /// `task` and its program objects must come from this loaded library.
    pub unsafe fn scan_task_once(
        &self,
        image: &LocatedImage,
        task: TaskInstance,
    ) -> Result<(), ProgramScanError> {
        let programs = program_slice(task)?;
        self.scan_programs_once(image, &programs)
    }

    unsafe fn scan_programs_once(
        &self,
        image: &LocatedImage,
        programs: &[crate::configuration::ProgramInstance],
    ) -> Result<(), ProgramScanError> {
        let mut ranges = Vec::with_capacity(programs.len());
        for program in programs {
            ranges.push(program.located_range()?);
        }
        image.copy_in_ranges(ranges.iter().copied())?;
        for program in programs {
            program.run()?;
        }
        image.copy_out_ranges(ranges)?;
        Ok(())
    }
}

unsafe fn build_task(
    owner: &Arc<ProgramApi>,
    image: &Arc<LocatedImage>,
    task: TaskInstance,
    index: usize,
) -> Result<RuntimeTask, ConfigurationError> {
    if task.interval_ns < 0 {
        return Err(ConfigurationError::InvalidInterval(task.interval_ns));
    }
    let programs = program_slice(task)?;
    let name = task_name(task.name, index);
    let owner = Arc::clone(owner);
    let image = Arc::clone(image);
    let body = Box::new(move |time_ns: u64| {
        owner.set_current_time(time_ns.min(i64::MAX as u64) as i64);
        owner
            .scan_programs_once(&image, &programs)
            .map_err(RuntimeTaskFault::Program)
    });
    Ok(RuntimeTask {
        name,
        spec: TaskSpec {
            interval_ns: task.interval_ns,
            priority: task.priority,
        },
        body,
    })
}

impl From<ConfigurationError> for ProgramScanError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

impl From<LocatedImageError> for ProgramScanError {
    fn from(error: LocatedImageError) -> Self {
        Self::Image(error)
    }
}

unsafe fn task_name(raw: *const std::ffi::c_char, index: usize) -> String {
    (!raw.is_null())
        .then(|| CStr::from_ptr(raw).to_str().ok())
        .flatten()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("plc-task-{index}"))
}
