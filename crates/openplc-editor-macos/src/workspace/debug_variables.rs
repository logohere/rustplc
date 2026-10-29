use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use open_rust_plc_compat_core::lifecycle::PlcState;

use crate::app::AppStore;
use crate::debug_values::DebugValue;
use crate::local_runtime::LocalPlcService;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const MAX_HISTORY_SECONDS: u64 = 600;
const MAX_HISTORY_POINTS: usize = (MAX_HISTORY_SECONDS * 1000 / 200) as usize;

#[derive(Debug, Clone, PartialEq)]
struct DebugTreeNode {
    label: String,
    full_path: String,
    value: Option<DebugValue>,
    children: Vec<DebugTreeNode>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
    at_ms: u64,
    value: f64,
}

#[component]
pub fn DebugVariablesPanel() -> Element {
    let runtime = use_context::<LocalPlcService>();
    let store = use_context::<AppStore>();
    let values = use_signal(Vec::<DebugValue>::new);
    let error = use_signal(|| None::<String>);
    let force_inputs = use_signal(HashMap::<String, String>::new);
    let expanded = use_signal(HashSet::<String>::new);
    let graph_list = use_signal(HashSet::<String>::new);
    let histories = use_signal(HashMap::<String, VecDeque<Sample>>::new);
    let paused = use_signal(|| false);
    let range_seconds = use_signal(|| 10u64);

    let poll_runtime = runtime.clone();
    let project_signal = store.project;
    let plc_state_signal = store.plc_state;
    let remote_debug_signal = store.remote_debug;
    let mut poll_values = values;
    let mut poll_error = error;
    let graph_signal = graph_list;
    let paused_signal = paused;
    let mut history_signal = histories;
    use_future(move || {
        let poll_runtime = poll_runtime.clone();
        async move {
            loop {
                let next = if let Some(session) = remote_debug_signal() {
                    if let Some(project) = project_signal() {
                        match tokio::task::spawn_blocking(move || session.debug_values(&project))
                            .await
                        {
                            Ok(Ok(next)) => Some(Ok(next)),
                            Ok(Err(message)) => Some(Err(message)),
                            Err(join) => Some(Err(format!("Remote debug poll failed: {join}"))),
                        }
                    } else {
                        None
                    }
                } else if plc_state_signal() == PlcState::Running {
                    project_signal().map(|project| poll_runtime.debug_values(&project))
                } else {
                    None
                };

                match next {
                    Some(Ok(next)) => {
                        if !paused_signal() {
                            record_samples(&mut history_signal, &graph_signal(), &next, now_ms());
                        }
                        poll_values.set(next);
                        poll_error.set(None);
                    }
                    Some(Err(message)) => poll_error.set(Some(message)),
                    None => {
                        poll_values.set(Vec::new());
                        poll_error.set(None);
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    });

    let rows = values();
    let tree = build_debug_tree(&rows);
    let plc_state = (store.plc_state)();
    let remote_active = (store.remote_debug)().is_some();
    let debug_active = remote_active || plc_state == PlcState::Running;

    rsx! {
        div { class: "flex h-full min-h-0 w-full flex-col overflow-hidden",
            div { class: "mb-2 flex shrink-0 items-center justify-between gap-2",
                div { class: "font-caption text-[10px] text-neutral-500",
                    if remote_active {
                        "Remote live values · WebSocket · 200 ms"
                    } else if plc_state == PlcState::Running {
                        "Live values · simulator · 200 ms"
                    } else {
                        "Start the simulator or attach a Runtime v4 debugger to read watched variables"
                    }
                }
                if debug_active {
                    span { class: "rounded bg-green-500/10 px-2 py-1 font-caption text-[10px] text-green-600 dark:text-green-400",
                        if remote_active { "REMOTE" } else { "RUNNING" }
                    }
                }
            }
            if let Some(message) = error() {
                div { class: "mb-2 shrink-0 rounded border border-red-200 bg-red-50 px-2 py-1 font-caption text-[10px] text-red-600 dark:border-red-900 dark:bg-red-950/30 dark:text-red-400",
                    "{message}"
                }
            }
            if debug_active && rows.is_empty() && error().is_none() {
                div { class: "flex h-full items-center justify-center font-caption text-xs text-neutral-500",
                    "No variables are marked for debugging. Enable Debug in a POU variable table."
                }
            } else if !rows.is_empty() {
                div { class: "flex min-h-0 flex-1 gap-2 overflow-hidden",
                    div { class: "min-h-0 w-[36%] min-w-[260px] overflow-auto rounded border border-neutral-200 p-1 dark:border-neutral-800",
                        div { class: "sticky top-0 z-10 grid grid-cols-[1fr_72px_82px] border-b border-neutral-200 bg-white px-2 py-1 font-caption text-[9px] font-medium uppercase text-neutral-500 dark:border-neutral-800 dark:bg-neutral-950",
                            span { "Variable" } span { "Type" } span { "Value" }
                        }
                        for node in tree.iter() {
                            DebugTreeRow {
                                key: "{node.full_path}",
                                node: node.clone(),
                                depth: 0,
                                expanded,
                                graph_list,
                                force_inputs,
                                error,
                            }
                        }
                    }
                    div { class: "flex min-h-0 min-w-0 flex-1 flex-col rounded border border-neutral-200 p-2 dark:border-neutral-800",
                        DebuggerHeader { paused, range_seconds, series_count: graph_list().len() }
                        div { class: "mt-2 min-h-0 flex-1 overflow-auto",
                            if graph_list().is_empty() {
                                div { class: "flex h-full items-center justify-center text-center font-caption text-xs text-neutral-500",
                                    "Select the graph button beside a numeric or BOOL variable to trend it."
                                }
                            } else {
                                for path in graph_list().iter() {
                                    TrendChart {
                                        key: "trend-{path}",
                                        path: path.clone(),
                                        samples: histories().get(path).cloned().unwrap_or_default(),
                                        range_seconds: range_seconds(),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DebuggerHeader(
    mut paused: Signal<bool>,
    mut range_seconds: Signal<u64>,
    series_count: usize,
) -> Element {
    rsx! {
        div { class: "flex h-8 shrink-0 items-center justify-between gap-2",
            div { class: "flex items-center gap-2 font-caption text-[10px]",
                span { "Range" }
                select {
                    class: "h-7 rounded border border-neutral-200 bg-transparent px-2 outline-none dark:border-neutral-800",
                    value: "{range_seconds}",
                    onchange: move |event| {
                        if let Ok(value) = event.value().parse::<u64>() {
                            range_seconds.set(value.clamp(1, MAX_HISTORY_SECONDS));
                        }
                    },
                    option { value: "1", "1 second" }
                    option { value: "10", "10 seconds" }
                    option { value: "30", "30 seconds" }
                    option { value: "60", "1 minute" }
                    option { value: "300", "5 minutes" }
                    option { value: "600", "10 minutes" }
                }
                span { class: "text-neutral-500", "{series_count} series" }
            }
            button {
                r#type: "button",
                title: if paused() { "Resume debugger graph" } else { "Pause debugger graph" },
                "aria-label": if paused() { "Resume debugger graph" } else { "Pause debugger graph" },
                class: "h-7 min-w-10 rounded bg-brand px-3 font-caption text-[10px] font-medium text-white",
                onclick: move |_| paused.toggle(),
                if paused() { "Play" } else { "Pause" }
            }
        }
    }
}

#[component]
fn DebugTreeRow(
    node: DebugTreeNode,
    depth: usize,
    mut expanded: Signal<HashSet<String>>,
    mut graph_list: Signal<HashSet<String>>,
    mut force_inputs: Signal<HashMap<String, String>>,
    error: Signal<Option<String>>,
) -> Element {
    let has_children = !node.children.is_empty();
    let is_expanded = expanded().contains(&node.full_path);
    let value = node.value.clone();
    let padding = depth.saturating_mul(12) + 4;
    let graphable = value
        .as_ref()
        .is_some_and(|row| parse_debug_numeric(&row.display).is_some());
    let graphed = graph_list().contains(&node.full_path);
    let type_name = value
        .as_ref()
        .map(|row| row.variable.type_name.clone())
        .unwrap_or_default();
    let display = value
        .as_ref()
        .map(|row| row.display.clone())
        .unwrap_or_default();
    let forced = value.as_ref().is_some_and(|row| row.variable.forced);

    rsx! {
        div {
            div {
                class: "grid min-h-7 grid-cols-[1fr_72px_82px] items-center border-b border-neutral-100 text-[10px] dark:border-neutral-900",
                div { class: "flex min-w-0 items-center gap-1", style: "padding-left: {padding}px",
                    if has_children {
                        button {
                            r#type: "button", title: if is_expanded { "Collapse" } else { "Expand" },
                            "aria-label": if is_expanded { "Collapse variable group" } else { "Expand variable group" },
                            class: "h-5 w-5 shrink-0 rounded text-brand hover:bg-neutral-100 dark:hover:bg-neutral-900",
                            onclick: {
                                let path = node.full_path.clone();
                                move |_| expanded.with_mut(|items| {
                                    if !items.remove(&path) { items.insert(path.clone()); }
                                })
                            },
                            if is_expanded { "▾" } else { "▸" }
                        }
                    } else { span { class: "w-5 shrink-0" } }
                    span { class: "min-w-0 flex-1 truncate font-mono text-neutral-850 dark:text-neutral-200", title: "{node.full_path}", "{node.label}" }
                    if graphable {
                        button {
                            r#type: "button", title: if graphed { "Remove from graph" } else { "Add to graph" },
                            "aria-label": if graphed { "Remove variable from graph" } else { "Add variable to graph" },
                            class: if graphed { "h-5 w-5 rounded bg-blue-500/15 text-blue-600" } else { "h-5 w-5 rounded text-neutral-400 hover:bg-neutral-100 hover:text-blue-600 dark:hover:bg-neutral-900" },
                            onclick: {
                                let path = node.full_path.clone();
                                move |_| graph_list.with_mut(|items| {
                                    if !items.remove(&path) { items.insert(path.clone()); }
                                })
                            },
                            "◉"
                        }
                    }
                }
                span { class: "truncate px-1 text-neutral-500", "{type_name}" }
                span { class: if forced { "truncate px-1 font-mono text-orange-600 dark:text-orange-400" } else { "truncate px-1 font-mono text-neutral-700 dark:text-neutral-300" },
                    "{display}"
                }
            }
            if let Some(row) = value {
                DebugLeafControls { row, force_inputs, error }
            }
            if has_children && is_expanded {
                for child in node.children.iter() {
                    DebugTreeRow {
                        key: "{child.full_path}",
                        node: child.clone(),
                        depth: depth + 1,
                        expanded,
                        graph_list,
                        force_inputs,
                        error,
                    }
                }
            }
        }
    }
}

#[component]
fn DebugLeafControls(
    row: DebugValue,
    mut force_inputs: Signal<HashMap<String, String>>,
    mut error: Signal<Option<String>>,
) -> Element {
    let runtime = use_context::<LocalPlcService>();
    let store = use_context::<AppStore>();
    let path = row.variable.path.clone();
    let is_bool = row.variable.type_name == "BOOL";
    let input_value = force_inputs().get(&path).cloned().unwrap_or_default();
    rsx! {
        div { class: "flex min-h-7 items-center justify-end gap-1 border-b border-neutral-100 px-2 py-1 dark:border-neutral-900",
            if row.variable.forced {
                span { class: "mr-auto rounded bg-orange-500/10 px-1.5 py-0.5 font-caption text-[9px] font-semibold uppercase text-orange-600 dark:text-orange-400", "forced" }
            }
            if is_bool {
                ForceButton { label: "TRUE", onclick: force_handler(runtime.clone(), store, path.clone(), "TRUE".to_owned(), error) }
                ForceButton { label: "FALSE", onclick: force_handler(runtime.clone(), store, path.clone(), "FALSE".to_owned(), error) }
            } else {
                input {
                    class: "h-6 w-28 rounded border border-neutral-200 bg-transparent px-1 font-mono text-[10px] outline-none focus:border-brand dark:border-neutral-800",
                    value: "{input_value}", placeholder: "{row.display}",
                    oninput: { let path = path.clone(); move |event| force_inputs.with_mut(|inputs| { inputs.insert(path.clone(), event.value()); }) }
                }
                ForceButton {
                    label: "Force",
                    onclick: {
                        let runtime = runtime.clone(); let path = path.clone();
                        move |_| {
                            let input = force_inputs().get(&path).cloned().unwrap_or_default();
                            if input.trim().is_empty() { error.set(Some(format!("Enter a force value for {path}."))); return; }
                            dispatch_force(runtime.clone(), store, path.clone(), input, error);
                        }
                    }
                }
            }
            if row.variable.forced {
                button {
                    r#type: "button", class: "h-6 rounded border border-neutral-200 px-2 text-[9px] font-medium text-neutral-600 hover:bg-neutral-100 dark:border-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-900",
                    title: "Release force", "aria-label": "Release force",
                    onclick: { let runtime = runtime.clone(); let path = path.clone(); move |_| dispatch_unforce(runtime.clone(), store, path.clone(), error) },
                    "Release"
                }
            }
        }
    }
}

#[component]
fn TrendChart(path: String, samples: VecDeque<Sample>, range_seconds: u64) -> Element {
    let now = now_ms();
    let cutoff = now.saturating_sub(range_seconds.saturating_mul(1000));
    let visible = samples
        .iter()
        .copied()
        .filter(|sample| sample.at_ms >= cutoff)
        .collect::<Vec<_>>();
    let (min, max) = visible
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), sample| {
            (min.min(sample.value), max.max(sample.value))
        });
    let span = if min.is_finite() && max.is_finite() {
        (max - min).abs().max(1.0)
    } else {
        1.0
    };
    let width = 100.0;
    let height = 32.0;
    let points = visible
        .iter()
        .map(|sample| {
            let x = if range_seconds == 0 {
                width
            } else {
                ((sample.at_ms.saturating_sub(cutoff)) as f64 / (range_seconds as f64 * 1000.0)
                    * width)
                    .clamp(0.0, width)
            };
            let y = if min.is_finite() {
                height - ((sample.value - min) / span * (height - 4.0) + 2.0)
            } else {
                height / 2.0
            };
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let latest = visible.last().map(|sample| sample.value);
    rsx! {
        div { class: "mb-2 rounded-lg border border-neutral-200 p-2 dark:border-neutral-800",
            div { class: "mb-1 flex items-center justify-between",
                span { class: "truncate font-mono text-[10px] text-neutral-700 dark:text-neutral-300", "{path}" }
                span { class: "font-mono text-[10px] font-semibold text-brand", if let Some(value) = latest { "{value:.4}" } else { "—" } }
            }
            svg { view_box: "0 0 100 32", preserve_aspect_ratio: "none", class: "h-20 w-full overflow-visible rounded bg-neutral-50 dark:bg-neutral-900", role: "img", "aria-label": "Trend graph for {path}",
                line { x1: "0", y1: "30", x2: "100", y2: "30", stroke: "currentColor", stroke_opacity: "0.12", stroke_width: "0.4" }
                if !points.is_empty() {
                    polyline { points: "{points}", fill: "none", stroke: "currentColor", stroke_width: "0.8", vector_effect: "non-scaling-stroke", class: "text-blue-500" }
                }
            }
            div { class: "mt-1 flex justify-between font-mono text-[8px] text-neutral-400",
                span { if min.is_finite() { "min {min:.3}" } else { "min —" } }
                span { "{visible.len()} samples" }
                span { if max.is_finite() { "max {max:.3}" } else { "max —" } }
            }
        }
    }
}

fn build_debug_tree(rows: &[DebugValue]) -> Vec<DebugTreeNode> {
    let mut roots = Vec::<DebugTreeNode>::new();
    for row in rows {
        let segments = split_debug_path(&row.variable.path);
        insert_tree_value(&mut roots, &segments, row.clone(), String::new());
    }
    sort_tree(&mut roots);
    roots
}

fn split_debug_path(path: &str) -> Vec<String> {
    path.split('.')
        .flat_map(|part| {
            let mut segments = Vec::new();
            let mut rest = part;
            while let Some(open) = rest.find('[') {
                if open > 0 {
                    segments.push(rest[..open].to_owned());
                }
                if let Some(close_rel) = rest[open..].find(']') {
                    let close = open + close_rel;
                    segments.push(rest[open..=close].to_owned());
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
            if !rest.is_empty() {
                segments.push(rest.to_owned());
            }
            segments
        })
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn insert_tree_value(
    nodes: &mut Vec<DebugTreeNode>,
    segments: &[String],
    row: DebugValue,
    prefix: String,
) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    let full_path = if prefix.is_empty() {
        head.clone()
    } else if head.starts_with('[') {
        format!("{prefix}{head}")
    } else {
        format!("{prefix}.{head}")
    };
    let index = nodes
        .iter()
        .position(|node| node.label == *head)
        .unwrap_or_else(|| {
            nodes.push(DebugTreeNode {
                label: head.clone(),
                full_path: full_path.clone(),
                value: None,
                children: Vec::new(),
            });
            nodes.len() - 1
        });
    if tail.is_empty() {
        nodes[index].value = Some(row);
    } else {
        insert_tree_value(&mut nodes[index].children, tail, row, full_path);
    }
}

fn sort_tree(nodes: &mut [DebugTreeNode]) {
    nodes.sort_by(|left, right| left.label.cmp(&right.label));
    for node in nodes {
        sort_tree(&mut node.children);
    }
}

fn record_samples(
    histories: &mut Signal<HashMap<String, VecDeque<Sample>>>,
    graph_list: &HashSet<String>,
    rows: &[DebugValue],
    at_ms: u64,
) {
    histories.with_mut(|all| record_samples_map(all, graph_list, rows, at_ms));
}

fn record_samples_map(
    all: &mut HashMap<String, VecDeque<Sample>>,
    graph_list: &HashSet<String>,
    rows: &[DebugValue],
    at_ms: u64,
) {
    all.retain(|path, _| graph_list.contains(path));
    for row in rows {
        if !graph_list.contains(&row.variable.path) {
            continue;
        }
        let Some(value) = parse_debug_numeric(&row.display) else {
            continue;
        };
        let series = all.entry(row.variable.path.clone()).or_default();
        series.push_back(Sample { at_ms, value });
        while series.len() > MAX_HISTORY_POINTS {
            series.pop_front();
        }
        let cutoff = at_ms.saturating_sub(MAX_HISTORY_SECONDS * 1000);
        while series.front().is_some_and(|sample| sample.at_ms < cutoff) {
            series.pop_front();
        }
    }
}

fn parse_debug_numeric(value: &str) -> Option<f64> {
    match value.trim().to_ascii_uppercase().as_str() {
        "TRUE" => Some(1.0),
        "FALSE" => Some(0.0),
        _ => value.trim().parse::<f64>().ok(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn force_handler(
    runtime: LocalPlcService,
    store: AppStore,
    path: String,
    value: String,
    error: Signal<Option<String>>,
) -> EventHandler<MouseEvent> {
    EventHandler::new(move |_| {
        dispatch_force(runtime.clone(), store, path.clone(), value.clone(), error)
    })
}

fn dispatch_force(
    runtime: LocalPlcService,
    store: AppStore,
    path: String,
    value: String,
    mut error: Signal<Option<String>>,
) {
    let Some(project) = (store.project)() else {
        return;
    };
    if let Some(session) = (store.remote_debug)() {
        spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                session.force_debug_value(&project, &path, &value)
            })
            .await;
            match result {
                Ok(Ok(())) => error.set(None),
                Ok(Err(message)) => error.set(Some(message)),
                Err(join) => error.set(Some(format!("Remote debug force failed: {join}"))),
            }
        });
    } else {
        match runtime.force_debug_value(&project, &path, &value) {
            Ok(()) => error.set(None),
            Err(message) => error.set(Some(message)),
        }
    }
}

fn dispatch_unforce(
    runtime: LocalPlcService,
    store: AppStore,
    path: String,
    mut error: Signal<Option<String>>,
) {
    let Some(project) = (store.project)() else {
        return;
    };
    if let Some(session) = (store.remote_debug)() {
        spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || session.unforce_debug_value(&project, &path))
                    .await;
            match result {
                Ok(Ok(())) => error.set(None),
                Ok(Err(message)) => error.set(Some(message)),
                Err(join) => error.set(Some(format!("Remote debug release failed: {join}"))),
            }
        });
    } else {
        match runtime.unforce_debug_value(&project, &path) {
            Ok(()) => error.set(None),
            Err(message) => error.set(Some(message)),
        }
    }
}

#[component]
fn ForceButton(label: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "h-6 rounded bg-neutral-100 px-2 text-[9px] font-medium text-neutral-700 hover:bg-neutral-200 dark:bg-neutral-850 dark:text-neutral-200 dark:hover:bg-neutral-800",
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{build_debug_tree, parse_debug_numeric, record_samples_map, MAX_HISTORY_POINTS};
    use crate::debug_values::{DebugValue, DebugVariable};

    fn row(path: &str, value: &str) -> DebugValue {
        DebugValue {
            variable: DebugVariable {
                path: path.to_owned(),
                type_name: "INT".to_owned(),
                size: 2,
                array: 0,
                element: 0,
                forced: false,
            },
            display: value.to_owned(),
        }
    }

    #[test]
    fn debug_tree_groups_nested_and_array_paths() {
        let rows = vec![
            row("INSTANCE0.PUMP.SPEED", "12"),
            row("INSTANCE0.PUMP.ALARMS[2]", "1"),
            row("GLOBAL", "7"),
        ];
        let tree = build_debug_tree(&rows);
        assert_eq!(tree.len(), 2);
        let instance = tree.iter().find(|node| node.label == "INSTANCE0").unwrap();
        let pump = &instance.children[0];
        assert_eq!(pump.label, "PUMP");
        assert!(pump
            .children
            .iter()
            .any(|node| node.label == "SPEED" && node.value.is_some()));
        let alarms = pump
            .children
            .iter()
            .find(|node| node.label == "ALARMS")
            .unwrap();
        assert_eq!(alarms.children[0].label, "[2]");
        assert_eq!(alarms.children[0].full_path, "INSTANCE0.PUMP.ALARMS[2]");
    }

    #[test]
    fn graph_numeric_codec_matches_bool_and_numeric_values() {
        assert_eq!(parse_debug_numeric("TRUE"), Some(1.0));
        assert_eq!(parse_debug_numeric("FALSE"), Some(0.0));
        assert_eq!(parse_debug_numeric("-12.5"), Some(-12.5));
        assert_eq!(parse_debug_numeric("hello"), None);
    }

    #[test]
    fn graph_history_is_bounded_and_drops_unselected_series() {
        let mut histories = HashMap::new();
        let selected = HashSet::from(["A".to_owned()]);
        for index in 0..(MAX_HISTORY_POINTS + 50) {
            record_samples_map(
                &mut histories,
                &selected,
                &[row("A", &index.to_string())],
                index as u64 * 200,
            );
        }
        assert!(histories.get("A").unwrap().len() <= MAX_HISTORY_POINTS);
        record_samples_map(&mut histories, &HashSet::new(), &[], 1_000_000);
        assert!(histories.is_empty());
    }
}
