use std::sync::Arc;

use crate::debug::DebugProtocol;
use crate::located_image::{LocatedImage, LocatedImageError};
use crate::program_api::{ProgramApi, ProgramDebugTarget};

pub(in crate::scan_runtime) struct FrameBoundary {
    pub(super) start: Box<dyn FnMut() -> Result<(), LocatedImageError> + Send>,
    pub(super) end: Box<dyn FnMut() -> Result<(), LocatedImageError> + Send>,
}

impl FrameBoundary {
    pub(in crate::scan_runtime) fn image(image: Arc<LocatedImage>) -> Self {
        let output = Arc::clone(&image);
        Self {
            start: Box::new(move || {
                // SAFETY: the dispatcher calls only when no task is in flight.
                unsafe { image.copy_config_globals_in() }
            }),
            end: Box::new(move || {
                // SAFETY: the last completion made every task quiescent.
                unsafe { output.copy_config_globals_out() }
            }),
        }
    }

    pub(in crate::scan_runtime) fn program(
        image: Arc<LocatedImage>,
        program: Arc<ProgramApi>,
        debug: Arc<DebugProtocol>,
    ) -> Self {
        let output = Arc::clone(&image);
        let end_debug = Arc::clone(&debug);
        Self {
            start: Box::new(move || {
                // SAFETY: the dispatcher calls only when no task is in flight.
                unsafe { image.copy_config_globals_in()? };
                debug.advance_scan_counter();
                Ok(())
            }),
            end: Box::new(move || {
                // SAFETY: the last completion made every task quiescent.
                unsafe { output.copy_config_globals_out()? };
                let target = ProgramDebugTarget::new(&program, &output);
                end_debug.apply_pending(&target)?;
                Ok(())
            }),
        }
    }
}
