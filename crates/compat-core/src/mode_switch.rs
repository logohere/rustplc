use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// OpenPLC mode-switch wire values.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchPosition {
    Stop = 0,
    Run = 1,
}

/// Atomic run/stop position and edge record from `plc_switch.c`.
pub struct ModeSwitch {
    position: AtomicU8,
    moved: AtomicBool,
}

impl Default for ModeSwitch {
    fn default() -> Self {
        Self::new()
    }
}

impl ModeSwitch {
    /// No switch-aware plugin means RUN is transparent to existing startup.
    pub const fn new() -> Self {
        Self {
            position: AtomicU8::new(SwitchPosition::Run as u8),
            moved: AtomicBool::new(false),
        }
    }

    /// Store a position and record only an actual edge.
    pub fn set_position(&self, position: SwitchPosition) {
        let next = position as u8;
        let previous = self.position.swap(next, Ordering::SeqCst);
        if previous != next {
            self.moved.store(true, Ordering::SeqCst);
        }
    }

    pub fn position(&self) -> SwitchPosition {
        match self.position.load(Ordering::SeqCst) {
            0 => SwitchPosition::Stop,
            _ => SwitchPosition::Run,
        }
    }

    pub fn allows_run(&self) -> bool {
        self.position() == SwitchPosition::Run
    }

    /// Consume the edge record once a transition-completion path observes it.
    pub fn take_movement(&self) -> bool {
        self.moved.swap(false, Ordering::SeqCst)
    }

    /// Re-arm a consumed edge when corrective transition start was refused.
    pub fn note_movement(&self) {
        self.moved.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::{ModeSwitch, SwitchPosition};

    #[test]
    fn defaults_to_run_without_a_plugin_edge() {
        let switch = ModeSwitch::new();
        assert_eq!(switch.position(), SwitchPosition::Run);
        assert!(switch.allows_run());
        assert!(!switch.take_movement());
    }

    #[test]
    fn records_edges_only_and_can_rearm() {
        let switch = ModeSwitch::new();
        switch.set_position(SwitchPosition::Stop);
        switch.set_position(SwitchPosition::Stop);
        assert_eq!(switch.position(), SwitchPosition::Stop);
        assert!(!switch.allows_run());
        assert!(switch.take_movement());
        assert!(!switch.take_movement());
        switch.note_movement();
        assert!(switch.take_movement());
    }
}
