# Vendored sources

Runtime and compiler/editor pins are recorded in UPSTREAM_BASELINE.md. These are public upstream sources; no private project is a build dependency.

- Monaco Editor assets are byte-identical to `monaco-editor` 0.54.0's `min/vs` tree. The npm tarball integrity was checked against the registry's SHA-512 value; 112 vendored asset files matched. Its license and third-party notices are included under LICENSES/.
- Bundled `.stlib` files match the output of the pinned STruC++ v0.6.2 checkout. Their sources and component license boundaries remain upstream-owned; legacy OSCAT has its own license, not EPL-2.0, MIT or GPL relicensing.
- macOS entitlements and AVR support files come from the pinned OpenPLC Editor. AVR license texts remain beside the headers, including the GCC Runtime Library Exception and MPLv2 text.
- Other generated compiler/runtime and standard-FB sources retain the recorded OpenPLC/STruC++ pins and notices.

`vendored-sha256.json` records the reviewed vendored bytes. Changes to these files require explicit provenance, license and dependency review, not an automatic manifest refresh.
