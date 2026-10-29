use std::sync::{Arc, MutexGuard};

use crate::debug::{DebugProtocol, DebugProvider};
use crate::located_image::LocatedImage;
use crate::program_api::ProgramApi;
use crate::scan_runtime::RuntimeHandle;
use crate::stats::StatsRegistry;

use super::{RuntimeOwnerError, RuntimeOwnerError::Debug, RuntimeOwnerError::Timing};

pub(super) struct OwnedRuntime {
    handle: Option<RuntimeHandle>,
    program: Option<Arc<ProgramApi>>,
    image: Option<Arc<LocatedImage>>,
    debug: Option<Arc<DebugProtocol>>,
    stats: Option<Arc<StatsRegistry>>,
}

impl OwnedRuntime {
    pub(super) fn tasks(handle: RuntimeHandle, stats: Arc<StatsRegistry>) -> Self {
        Self {
            handle: Some(handle),
            program: None,
            image: None,
            debug: None,
            stats: Some(stats),
        }
    }

    pub(super) fn program(
        handle: RuntimeHandle,
        program: Arc<ProgramApi>,
        image: Arc<LocatedImage>,
        debug: Arc<DebugProtocol>,
        stats: Arc<StatsRegistry>,
    ) -> Self {
        Self {
            handle: Some(handle),
            program: Some(program),
            image: Some(image),
            debug: Some(debug),
            stats: Some(stats),
        }
    }

    pub(super) fn prepare(&mut self) -> Result<(), RuntimeOwnerError> {
        if let (Some(handle), Some(stats)) = (&self.handle, &self.stats) {
            stats.publish(handle.task_stats()).map_err(Timing)?;
        }
        if let (Some(program), Some(debug)) = (&self.program, &self.debug) {
            debug.reset().map_err(Debug)?;
            let provider: Arc<dyn DebugProvider> = program.clone();
            debug.set_provider(provider);
        }
        Ok(())
    }

    pub(super) fn activate(&mut self) -> Result<(), RuntimeOwnerError> {
        if let Some(handle) = self.handle.as_mut() {
            handle.activate().map_err(RuntimeOwnerError::Runtime)?;
        }
        Ok(())
    }

    pub(super) fn image(&self) -> Option<Arc<LocatedImage>> {
        self.image.as_ref().map(Arc::clone)
    }

    pub(super) fn shutdown(&mut self) -> Result<(), RuntimeOwnerError> {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
        let stats = self.stats.take().map_or(Ok(()), |stats| stats.clear());
        let debug = self.debug.take().map_or(Ok(()), |debug| debug.reset());
        let program = self.program.as_ref().map_or(Ok(()), |program| {
            // SAFETY: stopping the owned handle joined every scan user.
            unsafe { program.restore_initial_state() }
        });
        self.image = None;
        self.program = None;
        program.map_err(RuntimeOwnerError::ProgramState)?;
        debug.map_err(Debug)?;
        stats.map_err(Timing)
    }
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub(super) fn discard(mut slot: MutexGuard<'_, Option<OwnedRuntime>>) {
    let runtime = slot.take();
    drop(slot);
    drop(runtime);
}
