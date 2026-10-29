use crate::lifecycle::PlcState;
use crate::mode_switch::SwitchPosition;

/// Action for the transition worker after its claimed operation lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileAction {
    None,
    Begin(PlcState),
    ReArmMovement,
}

/// Decide whether a switch movement requires a corrective transition.
pub const fn reconcile(
    movement_observed: bool,
    landed: PlcState,
    switch_position: SwitchPosition,
) -> ReconcileAction {
    if !movement_observed || !matches!(landed, PlcState::Running | PlcState::Stopped) {
        return ReconcileAction::None;
    }
    let wanted = match switch_position {
        SwitchPosition::Run => PlcState::Running,
        SwitchPosition::Stop => PlcState::Stopped,
    };
    match (landed, wanted) {
        (PlcState::Running, PlcState::Running) | (PlcState::Stopped, PlcState::Stopped) => {
            ReconcileAction::None
        }
        _ => ReconcileAction::Begin(wanted),
    }
}

/// Preserve the movement record when the corrective claim cannot start.
pub const fn correction_result(begin_succeeded: bool) -> ReconcileAction {
    if begin_succeeded {
        ReconcileAction::None
    } else {
        ReconcileAction::ReArmMovement
    }
}

#[cfg(test)]
mod tests {
    use super::{correction_result, reconcile, ReconcileAction};
    use crate::lifecycle::PlcState;
    use crate::mode_switch::SwitchPosition;

    #[test]
    fn corrects_clean_mismatch_only() {
        assert_eq!(
            reconcile(true, PlcState::Stopped, SwitchPosition::Run),
            ReconcileAction::Begin(PlcState::Running)
        );
        assert_eq!(
            reconcile(true, PlcState::Running, SwitchPosition::Run),
            ReconcileAction::None
        );
        assert_eq!(
            reconcile(true, PlcState::Error, SwitchPosition::Run),
            ReconcileAction::None
        );
    }

    #[test]
    fn rearms_when_correction_is_refused() {
        assert_eq!(correction_result(false), ReconcileAction::ReArmMovement);
        assert_eq!(
            reconcile(false, PlcState::Running, SwitchPosition::Stop),
            ReconcileAction::None
        );
    }
}
