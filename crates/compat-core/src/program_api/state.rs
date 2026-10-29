use super::ProgramApi;
use crate::debug::{MAX_FRAME, SUCCESS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramStateError {
    InvalidEntry {
        array: u8,
        element: u16,
        expected: usize,
        actual: usize,
    },
    RestoreRejected {
        array: u8,
        element: u16,
        operation: &'static str,
        status: u8,
    },
    ConfigResetRejected(u8),
}

#[derive(Default)]
pub(super) struct ProgramState(Vec<EntryState>);

struct EntryState {
    array: u8,
    element: u16,
    value: Vec<u8>,
}

impl ProgramState {
    pub(super) fn capture(api: &ProgramApi) -> Result<Self, ProgramStateError> {
        let mut entries = Vec::new();
        // SAFETY: symbols were resolved from the owned pinned program library.
        for array in 0..unsafe { (api.array_count)() } {
            // SAFETY: `array` is below the library-reported array count.
            for element in 0..unsafe { (api.elem_count)(array) } {
                entries.push(capture_entry(api, array, element)?);
            }
        }
        Ok(Self(entries))
    }

    pub(super) unsafe fn restore(&self, api: &ProgramApi) -> Result<(), ProgramStateError> {
        validate(api, &self.0)?;
        // SAFETY: the caller made all generated configuration users quiescent.
        let status = unsafe { (api.reset_config)() };
        if status != SUCCESS {
            return Err(ProgramStateError::ConfigResetRejected(status));
        }
        for entry in &self.0 {
            // SAFETY: validation matched this captured entry to the live table.
            check(entry, "unforce", unsafe {
                (api.debug_set)(entry.array, entry.element, false, std::ptr::null(), 0)
            })?;
            // SAFETY: the captured byte width matches the live typed entry.
            check(entry, "write", unsafe {
                (api.debug_write)(
                    entry.array,
                    entry.element,
                    entry.value.as_ptr(),
                    entry.value.len() as u16,
                )
            })?;
        }
        Ok(())
    }
}

fn capture_entry(
    api: &ProgramApi,
    array: u8,
    element: u16,
) -> Result<EntryState, ProgramStateError> {
    // SAFETY: the indexes came from the owned library's bounded table counts.
    let expected = usize::from(unsafe { (api.elem_size)(array, element) });
    if expected == 0 || expected > MAX_FRAME {
        return Err(ProgramStateError::InvalidEntry {
            array,
            element,
            expected,
            actual: 0,
        });
    }
    let mut value = vec![0; expected];
    // SAFETY: the destination has the validated library-reported size.
    let actual = usize::from(unsafe { (api.debug_read)(array, element, value.as_mut_ptr()) });
    if actual != expected {
        return Err(ProgramStateError::InvalidEntry {
            array,
            element,
            expected,
            actual,
        });
    }
    Ok(EntryState {
        array,
        element,
        value,
    })
}

fn validate(api: &ProgramApi, entries: &[EntryState]) -> Result<(), ProgramStateError> {
    for entry in entries {
        // SAFETY: the entry indexes were captured from this owned library.
        let actual = usize::from(unsafe { (api.elem_size)(entry.array, entry.element) });
        if actual != entry.value.len() {
            return Err(ProgramStateError::InvalidEntry {
                array: entry.array,
                element: entry.element,
                expected: entry.value.len(),
                actual,
            });
        }
    }
    Ok(())
}

fn check(entry: &EntryState, operation: &'static str, status: u8) -> Result<(), ProgramStateError> {
    (status == SUCCESS)
        .then_some(())
        .ok_or(ProgramStateError::RestoreRejected {
            array: entry.array,
            element: entry.element,
            operation,
            status,
        })
}

impl ProgramApi {
    /// Restore every persistent IEC leaf to its post-construction value.
    ///
    /// # Safety
    /// All scan tasks and external debug writes must be quiescent.
    pub unsafe fn restore_initial_state(&self) -> Result<(), ProgramStateError> {
        // SAFETY: the caller guarantees the program is quiescent.
        unsafe { self.initial_state.restore(self)? };
        // SAFETY: the function came from the still-owned pinned library.
        unsafe { (self.set_current_time)(0) };
        Ok(())
    }
}
