# strata-ffi

Production C99 Foreign Function Interface (FFI) for embedding the **Strata** pure-Rust UI engine inside C, C++, Python (`ctypes`/`cffi`), Swift, Kotlin/JNI, Go (`cgo`), and Node-API hosts.

## Capabilities

- **Panic-Safe Status ABI (`StrataStatus`)**: Every exported symbol catches Rust unwinds and records diagnostics readable via `strata_last_error()`.
- **C DOM & View Builders**: Construct `NodeBlueprint` trees (`strata_node_element`, `strata_node_text`, `strata_node_dynamic_text_signal`, `strata_node_set_class`, `strata_node_set_attr`, `strata_node_append_child`, `strata_node_on_event`) and convert to `StrataViewHandle`.
- **Live Reactive Signals**: Thread-safe `i64`, `f64`, `bool`, and UTF-8 `str` signals bridged directly to `strata_dom::mark_dirty()` and live `DynamicText` nodes.
- **Full `AppRuntime` Lifecycle & Input Dispatch**: `strata_runtime_create_from_view`, `update_view`, `resize`, `tick`, `pointer_move`, `pointer_down`, `pointer_up`, `wheel`, `key_down`, `key_up`, `text_input`, length-checked `strata_runtime_render_rgba`, `strata_runtime_export_png`, and `strata_launch_native_window`.
- **Standards-Compliant Tailwind CSS Evaluation**: `strata_eval_tailwind_css_checked` with two-call length probing.
- **Ready-to-Include C99 Header**: `include/strata.h` (`extern "C"` C++ compatible) and `cbindgen.toml`.
