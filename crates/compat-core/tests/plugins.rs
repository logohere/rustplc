use std::cell::RefCell;
use std::rc::Rc;

use open_rust_plc_compat_core::plugin_config::PluginConfig;
use open_rust_plc_compat_core::plugins::{PluginBackend, PluginManager, PluginPhase};

struct Mock {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail_init: bool,
    fail_start: bool,
    fail_stop: bool,
    cycle: Rc<RefCell<Vec<&'static str>>>,
    command: Option<String>,
}

impl PluginBackend for Mock {
    fn init(&mut self, _: &PluginConfig) -> Result<(), String> {
        self.events.borrow_mut().push("init");
        if self.fail_init {
            Err("init".into())
        } else {
            Ok(())
        }
    }
    fn start(&mut self) -> Result<(), String> {
        self.events.borrow_mut().push("start");
        if self.fail_start {
            Err("start".into())
        } else {
            Ok(())
        }
    }
    fn stop(&mut self) -> Result<(), String> {
        self.events.borrow_mut().push("stop");
        if self.fail_stop {
            Err("stop".into())
        } else {
            Ok(())
        }
    }
    fn cleanup(&mut self) {
        self.events.borrow_mut().push("cleanup");
    }
    fn cycle_start(&mut self) {
        self.cycle.borrow_mut().push("cycle_start");
    }
    fn cycle_end(&mut self) {
        self.cycle.borrow_mut().push("cycle_end");
    }
    fn execute_command(&mut self, command: &str, _: usize) -> Option<Result<String, String>> {
        self.command
            .as_ref()
            .map(|response| Ok(format!("{response}:{command}")))
    }
}

fn config(name: &str, enabled: i32) -> PluginConfig {
    PluginConfig {
        name: name.into(),
        path: "x".into(),
        enabled,
        plugin_type: 1,
        config_path: String::new(),
        venv_path: String::new(),
    }
}

fn mock(events: &Rc<RefCell<Vec<&'static str>>>, fail_init: bool, fail_start: bool) -> Box<Mock> {
    Box::new(Mock {
        events: events.clone(),
        fail_init,
        fail_start,
        fail_stop: false,
        cycle: events.clone(),
        command: None,
    })
}

#[test]
fn initializes_all_starts_enabled_and_cleans_in_reverse() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut manager = PluginManager::new();
    manager
        .add(config("a", 0), mock(&events, false, false))
        .unwrap();
    manager
        .add(config("b", 1), mock(&events, false, false))
        .unwrap();
    manager.initialize_all().unwrap();
    manager.start_enabled().unwrap();
    manager.stop_running();
    assert_eq!(manager.cleanup_initialized(), 2);
    assert_eq!(
        *events.borrow(),
        vec!["init", "init", "start", "stop", "cleanup", "cleanup"]
    );
    assert!(manager
        .entries()
        .iter()
        .all(|entry| entry.phase == PluginPhase::Unloaded));
}

#[test]
fn start_failure_is_reported_without_aborting_other_plugins() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut manager = PluginManager::new();
    manager
        .add(config("bad", 1), mock(&events, false, true))
        .unwrap();
    manager
        .add(config("good", 1), mock(&events, false, false))
        .unwrap();
    manager.initialize_all().unwrap();
    let report = manager.start_enabled().unwrap();
    assert_eq!(report.failed, vec!["bad"]);
    assert_eq!(manager.entries()[1].phase, PluginPhase::Running);
}

#[test]
fn failed_init_rolls_back_prior_plugins() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut manager = PluginManager::new();
    manager
        .add(config("good", 1), mock(&events, false, false))
        .unwrap();
    manager
        .add(config("bad", 1), mock(&events, true, false))
        .unwrap();
    assert!(manager.initialize_all().is_err());
    assert_eq!(*events.borrow(), vec!["init", "init", "cleanup"]);
    assert!(manager
        .entries()
        .iter()
        .all(|entry| entry.phase == PluginPhase::Unloaded));
}

#[test]
fn cycle_hooks_only_run_for_running_plugins() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut manager = PluginManager::new();
    manager
        .add(config("disabled", 0), mock(&events, false, false))
        .unwrap();
    manager
        .add(config("running", 1), mock(&events, false, false))
        .unwrap();
    manager.initialize_all().unwrap();
    manager.start_enabled().unwrap();
    manager.cycle_start();
    manager.cycle_end();
    assert_eq!(
        *events.borrow(),
        vec!["init", "init", "start", "cycle_start", "cycle_end"]
    );
}

#[test]
fn routes_plugin_commands_and_enforces_response_capacity() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut manager = PluginManager::new();
    let mut backend = mock(&events, false, false);
    backend.command = Some("ok".into());
    manager.add(config("driver", 1), backend).unwrap();
    manager.initialize_all().unwrap();
    assert_eq!(
        manager.execute_command("driver", "{}", 32).unwrap(),
        "ok:{}"
    );
    assert!(manager.execute_command("missing", "{}", 32).is_err());
    assert!(manager.execute_command("driver", "{}", 3).is_err());
}
