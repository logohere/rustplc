# Third-party notices

## OpenPLC Editor / STruC++ GPL-derived port surface

`crates/openplc-editor-macos` is a direct Rust port of source-defined behavior,
component structure, CSS usage, and assets from OpenPLC Editor at commit
`32e324c7c8278cbf95e8b519fb6b935312ffcfc4`. The pinned OpenPLC Editor source
is GPL-3.0 licensed. The Rust editor crate is therefore marked `GPL-3.0-only`.

`crates/compiler-core` directly ports/adapts STruC++ compiler behavior from the
pinned v0.6.2 baseline (`e48bc707dcce00572c07b07160a4437a5e5aed45`) and
vendors the exact IEC standard function-block ST sources used by that compiler.
The pinned STruC++ source is GPL-3.0 licensed. The Rust compiler crate is
therefore marked `GPL-3.0-only`.

The GPL-3.0 license text is included at `LICENSES/GPL-3.0.txt`.

These GPL-derived crates coexist with MIT-derived OpenPLC Runtime compatibility
work in this repository. The compatibility-code MIT license must not be interpreted as
relicensing the GPL-derived Editor/compiler portions.

## Other dependencies and notices

The vendored Monaco Editor 0.54.0 assets are MIT licensed. Its full license and
third-party notices are included at `LICENSES/MONACO-MIT.txt` and
`LICENSES/MONACO-THIRD-PARTY-NOTICES.txt`.

The vendored AVR C++ headers retain their original notices. Their distributed
license texts are kept beside them as `COPYING3`, `COPYING.RUNTIME`, and `COPYING.MPLv2` under
`crates/openplc-editor-macos/resources/sources/avr-libstdcpp`.

The macOS entitlements file is copied from the pinned GPL-3.0 OpenPLC Editor
source at `assets/entitlements.mac.plist`.

`ctrlc` 3.5.2 provides portable SIGINT/SIGTERM registration for the runnable
process. It is distributed under MIT or Apache-2.0 terms. Source and license:
https://github.com/Detegr/rust-ctrlc/tree/v3.5.2

`zip` 8.0.0 provides bounded ZIP parsing and extraction support for uploaded
program archives. It is distributed under MIT terms. Source and license:
https://github.com/zip-rs/zip2/tree/v8.0.0

`crates/compat-core/native/strucpp_bridge.cpp` adapts type declarations from
OpenPLC Runtime v4 `core/src/lib/strucpp_abi.hpp` at commit
`bf82b1b661fd95c9899969f629d692b77b4e1454`.

MIT License

Copyright (c) 2025 Autonomy-Logic

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Bundled PLC libraries

RustPLC Editor embeds the pinned STruC++ 0.6.2 library archives used by the
upstream editor/compiler baseline. The STruC++ README states that its C++
runtime and standard libraries are GPL-3.0 with the STruC++ Runtime Library
Exception. A verbatim copy of that exception is included at
`LICENSES/STRUCPP-RUNTIME-EXCEPTION.txt`.

The bundled STruC++ standard-library archives are:

- `iec-standard-fb.stlib`
- `iec-std-functions.stlib`
- `additional-function-blocks.stlib`

`oscat-basic.stlib` is derived from OSCAT Basic 3.35. OSCAT is an open-source
community automation library. The original license agreement is included at
`LICENSES/OSCAT-BASIC-LEGACY.md`. Redistribution of this legacy component must
remain free of charge and retain the OSCAT reference: https://www.oscat.de/ .
It is not relicensed as MIT, GPL, or the newer OSCAT EPL-2.0.

`plcopen-softmotion.stlib` contains/adapts code identified by the pinned source
as coming from:

- OpenSML (`https://github.com/feecat/OpenSML`), GPL-3.0. The GPL-3.0 text is
  already included at `LICENSES/GPL-3.0.txt`.
- S7RTT (`https://github.com/feecat/S7RTT`), Apache License 2.0. The vendored
  source files retain their upstream Apache-2.0 attribution/license headers;
  the Apache-2.0 text is included at `LICENSES/APACHE-2.0.txt`.

RustPLC does not claim ownership of these third-party library implementations.
Their names and licenses remain visible in this notice and in their library
metadata.
