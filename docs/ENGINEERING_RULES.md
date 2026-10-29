# Engineering Rules

These rules apply to port code and reviews. They are intentionally small and enforceable.

## Lean code

1. One issue refactors one existing OpenPLC module or observable contract into Rust. Do not replace source-defined behavior with a new design or bundle unrelated cleanup.
2. Reuse the standard library before adding a dependency or abstraction.
3. Production files stay at or below 200 lines and functions at or below 40 lines unless generated code or a documented protocol parser is clearer intact.
4. Do not add an interface with one implementation, a configurable value with one valid setting, or code for an unplanned feature.
5. A new dependency needs a ticket note covering purpose, license, maintenance, and why existing code is insufficient.

## Correctness and safety

1. Runtime paths return typed errors. Panic, `unwrap`, and `expect` are limited to tests or documented impossible invariants.
2. Safe Rust is the default. Unsafe code lives at a reviewed ABI/OS boundary, has a `SAFETY` explanation for every block, and is covered by layout and failure tests.
3. FFI cannot unwind across the boundary. Validate pointer, length, ownership, thread, and lifetime assumptions before entering safe code.
4. Validate and bound every upload, network frame, path, configuration value, and plugin message at its trust boundary.
5. Use monotonic time for scheduling. No sleep-based correctness tests, unbounded waits, silent retries, or hidden timing tolerances.
6. Hardware and network writes remain disabled until their milestone gate is approved.

## Tests and review

1. Every parity claim includes a fixture run against the pinned OpenPLC baseline and the Rust port.
2. Compare outputs, state transitions, diagnostics, and timing metadata; store the mismatch when parity fails.
3. Every bug fix adds the smallest regression fixture that reproduces the failure.
4. Public APIs document errors, panic conditions, safety assumptions, and a useful example.
5. macOS is the primary development host. CI must pass formatting, Clippy with warnings denied, tests, and platform builds there, while Linux and Windows 11 remain required compatibility checks.

## Definition of Ready

- Pinned upstream file, symbol, endpoint, or trace is linked.
- Observable behavior and exclusions are explicit.
- Acceptance checks are runnable and unambiguous.
- Dependencies and safety impact are identified.

## Definition of Done

- Rust implementation and differential fixture are reviewed together.
- All specified outputs match or an approved measured tolerance is recorded.
- Linux, macOS, and Windows 11 CI pass where the feature applies.
- Compatibility ledger and user-facing documentation are current.
- No field-readiness or safety claim is added.
- The replaced C/C++ or Python path is identified, parity-tested, and removable without changing public behavior.
- Production-readiness evidence covers operations, recovery, security, observability, upgrades, and rollback for the affected path.

## Sources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html)
- [Clippy usage and CI guidance](https://doc.rust-lang.org/stable/clippy/usage.html)
- [Rustonomicon: safe and unsafe boundaries](https://doc.rust-lang.org/stable/nomicon/safe-unsafe-meaning.html)
- [Rustonomicon: FFI](https://doc.rust-lang.org/nomicon/ffi.html)
- [NIST SP 800-218 Secure Software Development Framework](https://csrc.nist.gov/pubs/sp/800/218/final)
