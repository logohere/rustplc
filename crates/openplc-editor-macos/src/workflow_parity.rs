#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RuntimeState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EditorState {
    project_open: bool,
    dirty: bool,
    built: bool,
    runtime: RuntimeState,
    debugger_attached: bool,
    forced_value: bool,
    close_modal: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            project_open: false,
            dirty: false,
            built: false,
            runtime: RuntimeState::Stopped,
            debugger_attached: false,
            forced_value: false,
            close_modal: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Action {
    CreateProject,
    Edit,
    Save,
    Build,
    Start,
    AttachDebugger,
    ForceValue,
    ReleaseValue,
    DetachDebugger,
    Stop,
    RequestClose,
    DiscardAndClose,
}

const ACTIONS: [Action; 12] = [
    Action::CreateProject,
    Action::Edit,
    Action::Save,
    Action::Build,
    Action::Start,
    Action::AttachDebugger,
    Action::ForceValue,
    Action::ReleaseValue,
    Action::DetachDebugger,
    Action::Stop,
    Action::RequestClose,
    Action::DiscardAndClose,
];

fn transition(mut state: EditorState, action: Action) -> Result<EditorState, &'static str> {
    match action {
        Action::CreateProject if !state.project_open => {
            state = EditorState {
                project_open: true,
                ..EditorState::default()
            };
        }
        Action::Edit
            if state.project_open
                && state.runtime == RuntimeState::Stopped
                && !state.close_modal =>
        {
            state.dirty = true;
            state.built = false;
            state.debugger_attached = false;
            state.forced_value = false;
        }
        Action::Save if state.project_open && state.dirty && !state.close_modal => {
            state.dirty = false;
        }
        Action::Build
            if state.project_open
                && !state.dirty
                && state.runtime == RuntimeState::Stopped
                && !state.close_modal =>
        {
            state.built = true;
        }
        Action::Start
            if state.project_open
                && state.built
                && !state.dirty
                && state.runtime == RuntimeState::Stopped
                && !state.close_modal =>
        {
            state.runtime = RuntimeState::Running;
        }
        Action::AttachDebugger
            if state.project_open
                && state.runtime == RuntimeState::Running
                && !state.debugger_attached
                && !state.close_modal =>
        {
            state.debugger_attached = true;
        }
        Action::ForceValue if state.debugger_attached && !state.forced_value => {
            state.forced_value = true;
        }
        Action::ReleaseValue if state.debugger_attached && state.forced_value => {
            state.forced_value = false;
        }
        Action::DetachDebugger if state.debugger_attached => {
            state.debugger_attached = false;
            state.forced_value = false;
        }
        Action::Stop if state.runtime == RuntimeState::Running => {
            state.runtime = RuntimeState::Stopped;
            state.debugger_attached = false;
            state.forced_value = false;
        }
        Action::RequestClose
            if state.project_open
                && state.runtime == RuntimeState::Stopped
                && !state.close_modal =>
        {
            if state.dirty {
                state.close_modal = true;
            } else {
                state = EditorState::default();
            }
        }
        Action::DiscardAndClose if state.project_open && state.close_modal => {
            state = EditorState::default();
        }
        _ => return Err("action is not legal in the current editor state"),
    }
    validate(state)?;
    Ok(state)
}

fn validate(state: EditorState) -> Result<(), &'static str> {
    if !state.project_open
        && (state.dirty
            || state.built
            || state.runtime != RuntimeState::Stopped
            || state.debugger_attached
            || state.forced_value
            || state.close_modal)
    {
        return Err("closed project retained editor/runtime state");
    }
    if state.runtime == RuntimeState::Running
        && (!state.project_open || !state.built || state.dirty)
    {
        return Err("runtime can run only a clean built project");
    }
    if state.debugger_attached && state.runtime != RuntimeState::Running {
        return Err("debugger attached while runtime is stopped");
    }
    if state.forced_value && !state.debugger_attached {
        return Err("forced value exists without a debugger session");
    }
    if state.close_modal
        && (!state.project_open || !state.dirty || state.runtime != RuntimeState::Stopped)
    {
        return Err("close confirmation exists outside a dirty stopped project");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};
    use std::thread;
    use std::time::{Duration, Instant};

    use open_rust_plc_compat_core::lifecycle::PlcState;

    use super::{transition, Action, EditorState, RuntimeState, ACTIONS};
    use crate::local_runtime::LocalPlcService;
    use crate::plc_model::PouType;
    use crate::project::{Language, NewProjectRequest, ProjectKind, ProjectService};

    #[test]
    fn exhaustive_bounded_walk_preserves_all_editor_lifecycle_invariants() {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([(EditorState::default(), Vec::<Action>::new())]);
        seen.insert(EditorState::default());

        while let Some((state, path)) = queue.pop_front() {
            assert!(
                super::validate(state).is_ok(),
                "invalid state after {path:?}: {state:?}"
            );
            if path.len() == 12 {
                continue;
            }
            for action in ACTIONS {
                if let Ok(next) = transition(state, action) {
                    let mut next_path = path.clone();
                    next_path.push(action);
                    if seen.insert(next) {
                        queue.push_back((next, next_path));
                    }
                }
            }
        }

        assert_eq!(
            seen.len(),
            8,
            "modeled editor lifecycle state count changed"
        );
        assert!(seen
            .iter()
            .any(|state| state.runtime == RuntimeState::Running));
        assert!(seen.iter().any(|state| state.debugger_attached));
        assert!(seen.iter().any(|state| state.forced_value));
        assert!(seen.iter().any(|state| state.close_modal));
    }

    #[test]
    fn canonical_user_workflow_reaches_clean_closed_state() {
        let actions = [
            Action::CreateProject,
            Action::Edit,
            Action::Save,
            Action::Build,
            Action::Start,
            Action::AttachDebugger,
            Action::ForceValue,
            Action::ReleaseValue,
            Action::DetachDebugger,
            Action::Stop,
            Action::RequestClose,
        ];
        let final_state = actions
            .into_iter()
            .try_fold(EditorState::default(), transition)
            .unwrap();
        assert_eq!(final_state, EditorState::default());
    }

    #[test]
    fn dirty_close_is_blocked_until_explicit_discard() {
        let state = [Action::CreateProject, Action::Edit, Action::RequestClose]
            .into_iter()
            .try_fold(EditorState::default(), transition)
            .unwrap();
        assert!(state.project_open);
        assert!(state.dirty);
        assert!(state.close_modal);
        assert!(transition(state, Action::Start).is_err());
        assert_eq!(
            transition(state, Action::DiscardAndClose).unwrap(),
            EditorState::default()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "creates a project and replays the modeled path through the host compiler/runtime"]
    fn modeled_workflow_replays_against_real_project_and_runtime_services() {
        let temp = tempfile::tempdir().unwrap();
        let service = ProjectService::with_history_path(temp.path().join("history.json"));
        let root = temp.path().join("ModeledWorkflow");
        let request = NewProjectRequest {
            kind: ProjectKind::PlcProject,
            name: "ModeledWorkflow".to_owned(),
            path: root.clone(),
            language: Language::St,
            interval: "T#20ms".to_owned(),
        };

        let mut model = EditorState::default();
        let created = service.create(&request).unwrap();
        model = transition(model, Action::CreateProject).unwrap();
        assert!(model.project_open);

        let main = created
            .pous
            .iter()
            .find(|file| file.name == "main")
            .unwrap();
        service
            .save_text(
                main,
                "PROGRAM main\n  VAR\n    x : INT;\n  END_VAR\n  x := x + 1;\nEND_PROGRAM\n",
            )
            .unwrap();
        model = transition(model, Action::Edit).unwrap();
        model = transition(model, Action::Save).unwrap();

        let mut project = service.open(&root).unwrap();
        let parsed = project
            .pous
            .iter_mut()
            .find_map(|file| file.parsed.as_mut())
            .filter(|pou| pou.pou_type == PouType::Program)
            .unwrap();
        parsed.interface.variables[0].debug = Some(true);

        let runtime = LocalPlcService::new();
        let build = runtime.build_project(&project).unwrap();
        assert!(build.output.is_file());
        model = transition(model, Action::Build).unwrap();

        runtime.start().unwrap();
        model = transition(model, Action::Start).unwrap();
        assert_eq!(runtime.state().unwrap(), PlcState::Running);

        let values = runtime.debug_values(&project).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].variable.path, "INSTANCE0.X");
        model = transition(model, Action::AttachDebugger).unwrap();

        runtime
            .force_debug_value(&project, "instance0.x", "123")
            .unwrap();
        model = transition(model, Action::ForceValue).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let forced = loop {
            let values = runtime.debug_values(&project).unwrap();
            assert_eq!(values.len(), 1);
            if values[0].variable.forced && values[0].display == "123" {
                break values;
            }
            assert!(
                Instant::now() < deadline,
                "queued force was not applied at a scan boundary"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(forced[0].display, "123");

        runtime
            .unforce_debug_value(&project, "instance0.x")
            .unwrap();
        model = transition(model, Action::ReleaseValue).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let released = runtime.debug_values(&project).unwrap();
            if !released[0].variable.forced && released[0].display != "123" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "released value did not resume normal scan updates"
            );
            thread::sleep(Duration::from_millis(10));
        }

        model = transition(model, Action::DetachDebugger).unwrap();
        runtime.stop().unwrap();
        model = transition(model, Action::Stop).unwrap();
        assert_eq!(runtime.state().unwrap(), PlcState::Stopped);

        model = transition(model, Action::RequestClose).unwrap();
        assert_eq!(model, EditorState::default());
    }
}
