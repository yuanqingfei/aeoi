# Third-party notices

AEOI's own code is `MIT OR Apache-2.0` (LICENSE-MIT, LICENSE-APACHE); the specification and documentation are
CC-BY-4.0 (LICENSE-CC-BY-4.0). This file lists third-party material that is bundled in this repository or linked
into the binaries we build. Regenerate the crate table with `python3 tools/gen_third_party.py`.

## Bundled assets

| Asset | Where | License | Notice |
|---|---|---|---|
| matrixmultiply 0.3.11, patched copy (© bluss and contributors) | `rust/vendor/matrixmultiply` (linked into every binary through `[patch.crates-io]`; only change: the portable kernel is always used, see `AEOI-PATCH.md`) | MIT OR Apache-2.0 | `rust/vendor/matrixmultiply/LICENSE-MIT`, `LICENSE-APACHE` |
| rustfft 6.4.1, patched copy (© Allen Welkie, Elliott Mahler and contributors) | `rust/vendor/rustfft` (linked into every binary through `[patch.crates-io]`; only change: twiddle factors via `libm::cos/sin`, see `AEOI-PATCH.md`) | MIT OR Apache-2.0 | `rust/vendor/rustfft/LICENSE-MIT`, `LICENSE-APACHE` |
| Plotly.js basic 2.35.2 (© 2021 Plotly, Inc) | `rust/aeoi-web/static/vendor/plotly-basic-2.35.2.min.js`, copied to the Pages site `vendor/` | MIT | `rust/aeoi-web/static/vendor/plotly-LICENSE.txt` (shipped next to the file on the site) |

## Reference data and dev-only tools (not linked into any aeoi binary)

| Item | Use | License |
|---|---|---|
| python-control 0.10.2, scipy, numpy | generate `golden/python-control/` reference values (`gen_golden_pycontrol*.py`); values only, no code copied | BSD-3-Clause |
| mpmath | 50-digit reference values in `golden/mpmath/` (`gen_golden_mpmath.py`) | BSD-3-Clause |
| tpt-control (rev 10e4320, © TPT Solutions) | `tools/xcheck-tpt/` cross-check harness, referenced by git rev; no source copied | MIT OR Apache-2.0 |
| EngDesign (commit c8ec338, © 2025 EngDesign Benchmark Team) | 10 control/DSP task prompts, rubrics and reference solutions vendored verbatim in `eval/design/engdesign.json` (`tools/vendor_engdesign.py`); scoring re-implemented, no evaluator code copied | MIT — `eval/design/LICENSE-EngDesign` |
| ControlEval (EnvCommons/ControlEval; data from ControlAgent/ControlAgent) | design tasks downloaded at run time by `eval/design/fetch_controleval.py` into the gitignored `eval/design/upstream/`; nothing vendored, prompts and scoring are our own | none declared (not redistributed) |

Ideas borrowed, no code copied: result receipts and verify-by-replay (spec 4.3) from bicmath (https://github.com/nikoma/bicmath, MIT); "no evidence, no success claim" (spec E-11) from matlab-mcp-extensions (https://github.com/zws20070309/matlab-mcp-extensions).


## Rust crates

200 crates across the workspace (127 of them in the browser build `aeoi-wasm`). All are permissively licensed; where a crate offers a choice we use it under MIT or Apache-2.0.

| License expression | Crates |
|---|---|
| MIT OR Apache-2.0 | 107 |
| MIT | 29 |
| Unicode-3.0 | 18 |
| Apache-2.0 OR MIT | 10 |
| MIT/Apache-2.0 | 6 |
| Apache-2.0 | 4 |
| Zlib OR Apache-2.0 OR MIT | 4 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 3 |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 |
| Unlicense OR MIT | 2 |
| Apache-2.0 OR ISC OR MIT | 2 |
| ISC | 2 |
| BSD-2-Clause OR Apache-2.0 OR MIT | 2 |
| BSD-3-Clause | 1 |
| MIT AND BSD-3-Clause | 1 |
| MIT OR Apache-2.0 OR Zlib | 1 |
| Apache-2.0/MIT | 1 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 |
| CDLA-Permissive-2.0 | 1 |
| Apache-2.0 AND ISC | 1 |
| Zlib | 1 |
| Apache-2.0 OR BSL-1.0 | 1 |

| Crate | Version | License | In browser build |
|---|---|---|---|
| ahash | 0.8.12 | MIT OR Apache-2.0 | yes |
| aho-corasick | 1.1.5 | Unlicense OR MIT | yes |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | yes |
| approx | 0.5.1 | Apache-2.0 | yes |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |  |
| axum | 0.8.9 | MIT |  |
| axum-core | 0.5.6 | MIT |  |
| base64 | 0.22.1 | MIT OR Apache-2.0 | yes |
| base64 | 0.23.1 | MIT OR Apache-2.0 |  |
| bit-set | 0.5.3 | MIT/Apache-2.0 | yes |
| bit-vec | 0.6.3 | MIT/Apache-2.0 | yes |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | yes |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | yes |
| bumpalo | 3.20.4 | MIT OR Apache-2.0 | yes |
| bytecount | 0.6.9 | Apache-2.0/MIT | yes |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | yes |
| bytes | 1.12.2 | MIT |  |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | yes |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 |  |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | yes |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |  |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | yes |
| deranged | 0.5.9 | MIT OR Apache-2.0 | yes |
| digest | 0.10.7 | MIT OR Apache-2.0 | yes |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | yes |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | yes |
| fancy-regex | 0.13.0 | MIT | yes |
| foldhash | 0.2.0 | Zlib | yes |
| form_urlencoded | 1.2.3 | MIT OR Apache-2.0 | yes |
| fraction | 0.15.4 | MIT OR Apache-2.0 | yes |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |  |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | yes |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | yes |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | yes |
| generic-array | 0.14.7 | MIT | yes |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | yes |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | yes |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |  |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | yes |
| http | 1.5.0 | MIT OR Apache-2.0 |  |
| http-body | 1.1.0 | MIT |  |
| http-body-util | 0.1.5 | MIT |  |
| httparse | 1.10.1 | MIT OR Apache-2.0 |  |
| httpdate | 1.0.3 | MIT OR Apache-2.0 |  |
| hyper | 1.12.0 | MIT |  |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT |  |
| hyper-util | 0.1.21 | MIT |  |
| icu_collections | 2.1.1 | Unicode-3.0 | yes |
| icu_locale_core | 2.1.1 | Unicode-3.0 | yes |
| icu_normalizer | 2.1.1 | Unicode-3.0 | yes |
| icu_normalizer_data | 2.1.1 | Unicode-3.0 | yes |
| icu_properties | 2.1.2 | Unicode-3.0 | yes |
| icu_properties_data | 2.1.2 | Unicode-3.0 | yes |
| icu_provider | 2.1.1 | Unicode-3.0 | yes |
| idna | 1.1.0 | MIT OR Apache-2.0 | yes |
| idna_adapter | 1.2.1 | Apache-2.0 OR MIT | yes |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | yes |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |  |
| iso8601 | 0.6.6 | MIT | yes |
| itoa | 1.0.19 | MIT OR Apache-2.0 | yes |
| js-sys | 0.3.107 | MIT OR Apache-2.0 | yes |
| jsonschema | 0.18.3 | MIT | yes |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | yes |
| libc | 0.2.191 | MIT OR Apache-2.0 | yes |
| libm | 0.2.16 | MIT | yes |
| litemap | 0.8.3 | Unicode-3.0 | yes |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | yes |
| log | 0.4.35 | MIT OR Apache-2.0 |  |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib |  |
| matchit | 0.8.4 | MIT AND BSD-3-Clause |  |
| matrixmultiply | 0.3.11 | MIT/Apache-2.0 | yes |
| memchr | 2.8.3 | Unlicense OR MIT | yes |
| mime | 0.3.18 | MIT OR Apache-2.0 |  |
| mio | 1.2.5 | MIT |  |
| nalgebra | 0.33.3 | Apache-2.0 | yes |
| nom | 8.0.0 | MIT | yes |
| num | 0.4.3 | MIT OR Apache-2.0 | yes |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | yes |
| num-cmp | 0.1.0 | MIT/Apache-2.0 | yes |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | yes |
| num-conv | 0.1.0 | MIT OR Apache-2.0 | yes |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | yes |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | yes |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | yes |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | yes |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | yes |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | yes |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | yes |
| paste | 1.0.15 | MIT OR Apache-2.0 | yes |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | yes |
| pin-project-lite | 0.2.18 | Apache-2.0 OR MIT | yes |
| potential_utf | 0.1.6 | Unicode-3.0 | yes |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | yes |
| primal-check | 0.3.4 | MIT OR Apache-2.0 | yes |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | yes |
| quinn | 0.11.12 | MIT OR Apache-2.0 |  |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 |  |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 |  |
| quote | 1.0.47 | MIT OR Apache-2.0 | yes |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | yes |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |  |
| rand | 0.10.4 | MIT OR Apache-2.0 |  |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |  |
| rand_pcg | 0.10.3 | MIT OR Apache-2.0 |  |
| rawpointer | 0.2.1 | MIT/Apache-2.0 | yes |
| redox_syscall | 0.5.18 | MIT | yes |
| regex | 1.13.2 | MIT OR Apache-2.0 | yes |
| regex-automata | 0.4.19 | MIT OR Apache-2.0 | yes |
| regex-syntax | 0.8.12 | MIT OR Apache-2.0 | yes |
| reqwest | 0.12.28 | MIT OR Apache-2.0 |  |
| ring | 0.17.14 | Apache-2.0 AND ISC |  |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |  |
| rustfft | 6.4.1 | MIT OR Apache-2.0 | yes |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT |  |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 |  |
| rustls-webpki | 0.103.17 | ISC |  |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |  |
| safe_arch | 0.7.4 | Zlib OR Apache-2.0 OR MIT | yes |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | yes |
| serde | 1.0.229 | MIT OR Apache-2.0 | yes |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | yes |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | yes |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | yes |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 |  |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |  |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | yes |
| simba | 0.9.1 | Apache-2.0 | yes |
| slab | 0.4.12 | MIT | yes |
| smallvec | 1.16.6 | MIT OR Apache-2.0 | yes |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |  |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | yes |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 | yes |
| subtle | 2.6.1 | BSD-3-Clause |  |
| syn | 2.0.119 | MIT OR Apache-2.0 | yes |
| syn | 3.0.10 | MIT OR Apache-2.0 | yes |
| sync_wrapper | 1.0.2 | Apache-2.0 |  |
| synstructure | 0.14.0 | MIT | yes |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |  |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |  |
| time | 0.3.45 | MIT OR Apache-2.0 | yes |
| time-core | 0.1.7 | MIT OR Apache-2.0 | yes |
| time-macros | 0.2.25 | MIT OR Apache-2.0 | yes |
| tinystr | 0.8.5 | Unicode-3.0 | yes |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |  |
| tokio | 1.54.1 | MIT |  |
| tokio-macros | 2.7.2 | MIT |  |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 |  |
| tokio-stream | 0.1.19 | MIT |  |
| tower | 0.5.3 | MIT |  |
| tower-http | 0.6.11 | MIT |  |
| tower-layer | 0.3.3 | MIT |  |
| tower-service | 0.3.3 | MIT |  |
| tracing | 0.1.44 | MIT |  |
| tracing-core | 0.1.36 | MIT |  |
| transpose | 0.2.3 | MIT OR Apache-2.0 | yes |
| try-lock | 0.2.5 | MIT |  |
| typenum | 1.20.1 | MIT OR Apache-2.0 | yes |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | yes |
| untrusted | 0.9.0 | ISC |  |
| url | 2.5.9 | MIT OR Apache-2.0 | yes |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | yes |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | yes |
| uuid | 1.26.1 | Apache-2.0 OR MIT | yes |
| want | 0.3.2 | MIT |  |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | yes |
| wasip2 | 1.0.1+wasi-0.2.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | yes |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | yes |
| wasm-bindgen-futures | 0.4.80 | MIT OR Apache-2.0 |  |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 | yes |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 | yes |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 | yes |
| web-sys | 0.3.107 | MIT OR Apache-2.0 |  |
| web-time | 1.1.0 | MIT OR Apache-2.0 | yes |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 |  |
| wide | 0.7.33 | Zlib OR Apache-2.0 OR MIT | yes |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | yes |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |  |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |  |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |  |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |  |
| wit-bindgen | 0.46.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | yes |
| writeable | 0.6.4 | Unicode-3.0 | yes |
| yoke | 0.8.4 | Unicode-3.0 | yes |
| yoke-derive | 0.8.4 | Unicode-3.0 | yes |
| zerocopy | 0.8.64 | BSD-2-Clause OR Apache-2.0 OR MIT | yes |
| zerocopy-derive | 0.8.64 | BSD-2-Clause OR Apache-2.0 OR MIT | yes |
| zerofrom | 0.1.8 | Unicode-3.0 | yes |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | yes |
| zeroize | 1.9.2 | Apache-2.0 OR MIT |  |
| zerotrie | 0.2.5 | Unicode-3.0 | yes |
| zerovec | 0.11.9 | Unicode-3.0 | yes |
| zerovec-derive | 0.11.7 | Unicode-3.0 | yes |
| zmij | 1.0.23 | MIT | yes |
