use std::sync::Arc;

use crate::lifecycle::PlcState;
use crate::located_image::LocatedImage;
use crate::program_api::ProgramApi;
use crate::scan_runtime::{FrameFaultHook, RuntimeHandle};

use super::{OwnedRuntime, RuntimeOwner, RuntimeOwnerError, StartResult};

impl RuntimeOwner {
    /// Starts one loaded generated program under lifecycle ownership.
    /// # Errors
    /// Returns image, configuration, runtime, state, or reset failures.
    /// # Safety
    /// The program must implement the pinned STruC++ ABI and all external
    /// debug users must stop before this owner is stopped or dropped.
    pub unsafe fn start_program(
        &self,
        program: Arc<ProgramApi>,
    ) -> Result<StartResult, RuntimeOwnerError> {
        if let Some(result) = self.claim_start()? {
            return Ok(result);
        }
        // SAFETY: forwarded from the caller to the pinned generated ABI.
        unsafe { self.start_claimed_program(program) }
    }

    pub(crate) unsafe fn start_claimed_program(
        &self,
        program: Arc<ProgramApi>,
    ) -> Result<StartResult, RuntimeOwnerError> {
        // SAFETY: forwarded from the caller to the pinned generated ABI.
        let runtime = unsafe { self.build_program(program) };
        self.finish_start(runtime)
    }

    pub fn image(&self) -> Result<Option<Arc<LocatedImage>>, RuntimeOwnerError> {
        Ok(self
            .lock_runtime()
            .map_err(RuntimeOwnerError::State)?
            .as_ref()
            .and_then(OwnedRuntime::image))
    }

    unsafe fn build_program(
        &self,
        program: Arc<ProgramApi>,
    ) -> Result<OwnedRuntime, RuntimeOwnerError> {
        // SAFETY: the caller guarantees the loaded program ABI.
        let image = unsafe { program.located_image() }.map_err(RuntimeOwnerError::Image)?;
        // SAFETY: image and task descriptors come from this program.
        let tasks = unsafe { program.runtime_tasks(Arc::clone(&image)) }
            .map_err(RuntimeOwnerError::Configuration)?;
        let handle = RuntimeHandle::prepare_program(
            tasks,
            Arc::clone(&image),
            Arc::clone(&program),
            Arc::clone(&self.debug),
            self.frame_fault_hook(),
        )
        .map_err(RuntimeOwnerError::Runtime)?;
        Ok(OwnedRuntime::program(
            handle,
            program,
            image,
            Arc::clone(&self.debug),
            Arc::clone(&self.stats),
        ))
    }

    fn frame_fault_hook(&self) -> FrameFaultHook {
        let lifecycle = Arc::clone(&self.lifecycle);
        Arc::new(move |_| {
            let _ = lifecycle.publish_final_state(PlcState::Error);
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::lifecycle::PlcState;
    use crate::located_image::LocatedImageError;

    use super::RuntimeOwner;

    #[test]
    fn frame_fault_hook_publishes_error() {
        let owner = RuntimeOwner::new();
        owner.frame_fault_hook()(LocatedImageError::Poisoned);
        assert_eq!(owner.state().unwrap(), PlcState::Error);
    }
}
