# Lifecycle port slice

## Source contract

- Upstream commit: `bf82b1b661fd95c9899969f629d692b77b4e1454`
- Source: `core/src/plc_app/plc_state_manager.h`, `core/src/plc_app/plc_state_manager.cpp`, and `core/src/plc_app/unix_socket.c`
- Rust target: `crates/compat-core/src/lifecycle.rs`
- Command target: `crates/compat-core/src/command.rs`
- Located-global target: `crates/compat-core/src/located_globals.rs`
- ABI target: `crates/compat-core/src/abi.rs`
- Configuration ABI target: `crates/compat-core/src/configuration.rs`
- Timing target: `crates/compat-core/src/timing.rs`
- Watchdog target: `crates/compat-core/src/watchdog.rs`
- Journal target: `crates/compat-core/src/journal.rs`
- Process-image target: `crates/compat-core/src/image.rs`
- Located-image target: `crates/compat-core/src/located_image.rs`
- Journal queue target: `crates/compat-core/src/journal/queue.rs`
- Command reader target: `crates/compat-core/src/socket.rs`
- Mode-switch target: `crates/compat-core/src/mode_switch.rs`
- Transition target: `crates/compat-core/src/transition.rs`
- Transition-service target: `crates/compat-core/src/runtime_service.rs`
- Scheduler target: `crates/compat-core/src/scheduler.rs`
- Scan-runtime target: `crates/compat-core/src/scan_runtime.rs`
- Server target: `crates/compat-core/src/server.rs`
- STATS target: `crates/compat-core/src/stats.rs`
- Debug target: `crates/compat-core/src/debug.rs`
- Debug-write target: `crates/compat-core/src/debug_write.rs`
- Loader target: `crates/compat-core/src/loader.rs`, `crates/compat-core/src/program_api.rs`
- Native plugin symbol target: `crates/compat-core/src/native_plugin.rs`
- Plugin runtime-args target: `crates/compat-core/src/plugin_runtime_args.rs`
- Plugin config target: `crates/compat-core/src/plugin_config.rs`
- Plugin lifecycle target: `crates/compat-core/src/plugins.rs`

The Rust slice preserves the seven enum values, STOPPED initialization, requestable targets, dropped redundant/overlapping transitions, transition-direction states, sticky ERROR over STOPPED, guarded RUNNING landing, and exact command-socket status strings.

The command handler preserves PING, STATUS, SWITCH, START, STOP, busy gating during transitions, the mode-switch start gate, and unknown-command responses. It now reads the lifecycle, switch, live STATS registry, and debug provider from one runtime owner. Accepted START/STOP requests run against the configured generated library on tracked background workers.

The located-global slice matches storage identities, skips null descriptors, and preserves aliases like `located_globals_join_ex`. The generated-program adapter validates the optional exported pointer table and builds the configuration-global index without ordering assumptions. Explicit copy-in/out methods require task quiescence; dispatcher ownership and mismatch diagnostics remain pending.

The ABI slice mirrors `LocatedVar`, `LocatedArea`, `LocatedSize`, `IEC_TIMESPEC`, and `IEC_STRING` layout. Raw enum bytes are validated before decoding. A minimal C++17 bridge walks configuration/task/program objects using the compiler ABI and catches every C++ exception before returning a status to Rust. Windows MSVC fails closed because OpenPLC Windows uses the MinGW/GNU ABI; Windows GNU fixture evidence remains pending.

The timing slice uses a monotonic clock, per-task counters, the upstream 2-second EWMA window, and explicit overrun accounting. It does not claim scheduler or real-time performance parity until the differential harness measures both implementations under the same host load.

The watchdog slice mirrors the pinned 2-second policy: transition watchdog forcing only after the 120-second strict bound, no-heartbeat detection while RUNNING, and heartbeat reset while in ERROR. It returns decisions only; a runtime owner still has to apply lifecycle and atomic-heartbeat side effects.

The journal slice preserves the fourteen buffer-type values, BOOL bit range, non-BOOL `0xff` sentinel, enqueue-versus-apply bounds distinction, sequence field, and width masking. It is an entry contract only; lock-free producers, forced-slot ownership, image pointers, and the scan-boundary consumer remain separate work.

The process-image slice applies those entries to bounded typed tables and preserves forced-slot rejection and release. The located-image boundary validates generated descriptors, keeps the loaded library alive, drains the journal before copy-in, copies typed values to program storage, and dirty-diffs non-input values back to the journal after execution. It does not read drivers or write hardware.

The queue slice provides the portable mutex-backed fallback with initialization, sequence reset, bounded capacity, ordered drain, and explicit apply statistics. The pinned runtime’s always-lock-free double-buffer producer, emergency flush behavior, and scan-thread ownership are not claimed yet.

The command-reader slice preserves the pinned 8192-byte buffer limit, newline stripping, carriage-return retention, bounded truncation, and partial-EOF discard. It is transport-neutral and does not create a Unix or TCP socket. The command handler now shares the mode-switch atomic contract rather than maintaining a duplicate gate.

The mode-switch slice preserves the RUN default, atomic edge-only movement record, STOP start gate, movement consumption, and corrective re-arm. The transition service corrects a clean landing after a movement; no hardware polling exists.

The scheduler slice preserves the 20 ms zero-interval fallback, GCD master tick, divisor due test, fastest-task priority tie-break, and binary release/overrun rule. The scan runtime supplies one worker per task, absolute-deadline dispatch, binary release/overrun gating, tracker hooks, lifecycle ownership, and generated-program copy-in/run/copy-out ordering. C++ exceptions become typed program faults; a faulted worker records its first terminal cause, stops, and does not stop peer tasks. Dispatcher-owned global synchronization, lifecycle logging, driver I/O, and real-time performance evidence remain pending.

The server slice provides a sequential Unix-domain command server using the merged reader and lifecycle-owned handler. It safely replaces stale sockets, refuses regular files and live listeners, wakes an active client during shutdown, and removes its socket on drop. The `rustplc` process binds before auto-start, supports safe mode, handles SIGINT/SIGTERM, and is smoke-tested against the pinned generated library on macOS/Linux. Windows still returns an explicit unsupported result for this Unix boundary.

The STATS slice publishes the exact worker-owned trackers and emits OpenPLC's task-array response, including null measurements before a task's first scan. Generated task order/name, live numeric fields, and post-stop cleanup match the pinned Linux oracle; raw timing tolerance and plugin statistics remain unqualified.

The debug slice preserves function-code framing, bounds/status bytes, scan-counter/header shape, and native-endian MD5 sentinel through an explicit provider interface. Pending writes can now be applied only through a cycle-boundary target, with target status reported; no provider means `NOT_LOADED`, and it never invents variable values.

External debug writes use a fixed 128-entry, 256-byte bounded queue and expose drain/apply ownership to the scan boundary. `ProgramApi` is the only current target and routes write/force/unforce through the generated-program symbols; the socket path never mutates generated-program storage directly.

The program API resolves the pinned generated-program runtime symbols (time hooks, configuration/located accessors, optional located-global identity accessors, and debug read/set/write) through a platform-native shared-library boundary. Missing paths, required symbols, invalid descriptors, oversized descriptor tables, C++ exceptions, and unsupported MSVC C++ ABI calls fail closed. CI compiles a real STruC++ v0.6.2 program with program-local and configuration-global `%I`/`%Q` variables, proves both copy paths, then triggers a generated array-bounds exception and verifies the typed terminal task fault on macOS and Linux. This is ABI evidence, not a full upstream differential trace.

The transition slice preserves movement-gated reconciliation: it corrects only clean RUNNING/STOPPED landings, leaves ERROR/EMPTY untouched, and re-arms movement when the corrective claim is refused. The standard-library transition service claims before replying, tracks and joins workers, completes accepted start/stop work, waits through a racing landing, and falls back to synchronous completion if worker spawn fails.

The plugin-config slice preserves the six CSV positions, optional paths, C-compatible integer parsing, line skipping, and fixed buffer limits. Its file helper reports open/read errors but only parses text; path policy, dynamic loading, and plugin start/stop remain separate safety-gated work.

The plugin lifecycle slice preserves the pinned init-all, enabled-only start, running-only stop, reverse cleanup, start-failure reporting, partial-init rollback, running-only cycle-hook, and bounded command-routing rules. Native symbol discovery now requires `init` and records the optional OpenPLC hooks without invoking them. The plugin runtime-args layout is mirrored with opaque callback slots; typed callbacks, Python loading, and live driver I/O remain separate work.

## Deliberate boundary

This remains a partial runtime. The runnable process joins a generated program, scan workers, and the Unix command handler, but there is no upload pipeline, REST/WebSocket/Modbus service, live driver I/O, packaging, or hardware-write path.

```bash
cargo test --workspace
python3 scripts/diff_trace.py --self-test
```
