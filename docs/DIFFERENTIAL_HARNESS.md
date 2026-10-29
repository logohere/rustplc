# Differential trace harness

The harness compares two non-interactive commands that receive the same fixture path and emit one JSON trace on stdout. Commands are executed without a shell. Each trace must contain:

```json
{
  "metadata": {
    "baseline_commit": "<40-character SHA>",
    "os": "<host OS>",
    "architecture": "<host architecture>",
    "clock": "monotonic_ns",
    "scheduler": "<scheduler name>",
    "build_mode": "<debug or release>"
  },
  "observations": []
}
```

Run the deterministic harness self-check:

```bash
python3 scripts/diff_trace.py --self-test
python3 scripts/diff_trace.py --self-test --expect-mismatch
```

The second command intentionally changes one state and verifies that a focused mismatch artifact is written under `artifacts/differential/`. The self-test runner is only a harness check; it is not an OpenPLC oracle and does not close the parity gate.

When the pinned OpenPLC executable and the Rust executable are available, pass both commands with the required `{fixture}` token:

```bash
python3 scripts/diff_trace.py fixtures/lifecycle-basic.json \
  --upstream-cmd '/path/to/openplc-trace-runner {fixture}' \
  --rust-cmd '/path/to/rustplc-trace-runner {fixture}'
```

The checked-in upstream adapter captures raw replies from a running pinned Unix-domain command socket. It requires an explicit state-change opt-in because START/STOP can operate a configured runtime:

```bash
python3 scripts/openplc_socket_trace.py fixtures/lifecycle-basic.json \
  --socket /run/runtime/plc_runtime.socket --allow-state-change
```

It refuses unsupported fixture commands, non-Unix hosts, oversized replies, and START/STOP without the opt-in. It does not build, configure, or claim safety for the runtime; run it only against an isolated test instance with hardware writes disabled.

The real upstream adapter must drive the pinned runtime or its existing lifecycle suite and record socket replies, state transitions, diagnostics, process-image values, and measured timing metadata. No fixture runner may invent OpenPLC behavior. Hardware and network writes stay disabled.
