# strata-ffi

Universal C-ABI Foreign Function Interface (FFI) for embedding Strata reactive UI, Tailwind evaluation, and telemetry rendering engines in non-Rust host applications.

---

## Architecture: Host Application Embedding

The `strata-ffi` crate exposes a strictly versioned, stable C-compatible Application Binary Interface (ABI). This allows native host applications written in **C, C++, Python, Swift (macOS/iOS), Kotlin/Java (Android/JVM), Go, or Node.js** to embed Strata as an offscreen rendering engine, reactive state coordinator, or high-performance telemetry dashboard without requiring the host application to be written in Rust.

```
       +-------------------------------------------------------+
       |             Host Application Execution Space          |
       |             (C++, Python, Swift, Kotlin, Go)          |
       +---------------------------+---------------------------+
                                   |
                  Universal C ABI Boundaries (CDECL)
                                   v
       +-------------------------------------------------------+
       |                  Strata C-FFI Layer                   |
       |  - strata_app_create() / strata_app_destroy()         |
       |  - strata_window_create() / strata_window_set_title() |
       |  - strata_signal_create_int() / subscribe / set       |
       |  - strata_eval_tailwind_css()                         |
       |  - strata_render_to_buffer()                          |
       +---------------------------+---------------------------+
                                   |
                     Opaque Pointers & Boxed Handles
                                   v
       +-------------------------------------------------------+
       |                 Strata Core Runtimes                  |
       |  - strata_dom (Signals & Reactive Runtime)            |
       |  - strata_style (Tailwind JIT Engine)                 |
       |  - strata_render (Offscreen Framebuffers)             |
       +-------------------------------------------------------+
```

### ABI Design Rules

1. **Opaque Pointer Encapsulation**: All Rust data structures (`StrataAppHandle`, `StrataWindowHandle`, `StrataSignalHandle`) are hidden behind opaque raw pointers (`*mut StrataAppHandle`). Host applications never inspect internal struct layouts directly.
2. **Explicit Memory Ownership**: Memory allocated in Rust must be deallocated in Rust via dedicated destruction functions (`strata_app_destroy`, `strata_signal_destroy`).
3. **Null Pointer Resilience**: Every exported C function validates incoming pointers. Null pointers return defensive error codes (`-1` or `0`) rather than causing segmentation faults.
4. **Caller-Allocated Offscreen Buffers**: Framebuffer rendering and string outputs write into memory allocated by the caller, eliminating unnecessary cross-boundary heap reallocations.

---

## C-ABI Header Reference (`strata.h`)

Below is the complete C99/C++ header specification for `strata-ffi`:

```c
#ifndef STRATA_FFI_H
#define STRATA_FFI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque struct handles */
typedef struct StrataAppHandle StrataAppHandle;
typedef struct StrataWindowHandle StrataWindowHandle;
typedef struct StrataSignalHandle StrataSignalHandle;

/* Reactive callback function signature */
typedef void (*StrataSignalCallbackInt)(int64_t new_value, void* user_data);

/* Runtime introspection */
const char* strata_version(void);

/* Application lifecycle */
StrataAppHandle* strata_app_create(void);
void strata_app_destroy(StrataAppHandle* app);
int32_t strata_app_is_running(const StrataAppHandle* app);

/* Window management */
StrataWindowHandle* strata_window_create(StrataAppHandle* app, const char* title, uint32_t width, uint32_t height);
void strata_window_destroy(StrataWindowHandle* win);
int32_t strata_window_set_title(StrataWindowHandle* win, const char* title);
uint32_t strata_window_get_width(const StrataWindowHandle* win);
uint32_t strata_window_get_height(const StrataWindowHandle* win);
int32_t strata_window_set_visible(StrataWindowHandle* win, int32_t visible);

/* Reactive integer signals */
StrataSignalHandle* strata_signal_create_int(int64_t initial);
int64_t strata_signal_get_int(const StrataSignalHandle* sig);
void strata_signal_set_int(StrataSignalHandle* sig, int64_t value);
uint64_t strata_signal_subscribe_int(StrataSignalHandle* sig, StrataSignalCallbackInt callback, void* user_data);
int32_t strata_signal_unsubscribe_int(StrataSignalHandle* sig, uint64_t sub_id);
void strata_signal_destroy(StrataSignalHandle* sig);

/* Tailwind JIT compiler evaluation */
int32_t strata_eval_tailwind_css(const char* classes, char* out_buf, size_t max_len);

/* Offscreen framebuffer rendering */
int32_t strata_render_to_buffer(
    uint32_t width,
    uint32_t height,
    uint8_t r,
    uint8_t g,
    uint8_t b,
    uint8_t a,
    uint8_t* out_rgba_ptr
);

#ifdef __cplusplus
}
#endif

#endif /* STRATA_FFI_H */
```

---

## Detailed Symbol Documentation

### Application Lifecycle

#### `strata_app_create` & `strata_app_destroy`
Allocates a new root application coordinator managing the event loop and child window pointers:

```c
StrataAppHandle* app = strata_app_create();
if (strata_app_is_running(app)) {
    /* Main loop execution */
}
strata_app_destroy(app); // Recursively destroys child windows and releases memory
```

### Window Management

#### `strata_window_create`
Spawns a native desktop window handle attached to the application context:

```c
StrataWindowHandle* win = strata_window_create(app, "Cluster Telemetry Monitor", 1920, 1080);
uint32_t w = strata_window_get_width(win);
uint32_t h = strata_window_get_height(win);

strata_window_set_title(win, "Cluster Telemetry Monitor [CONNECTED]");
```

### Reactive Signal Bridge

The reactive signal bridge enables cross-language state observation. When host code or Rust internal updates modify a signal value, registered C-ABI callbacks fire immediately with the updated value and context pointer.

#### Subscription Lifecycle

```c
static void on_active_nodes_changed(int64_t new_count, void* user_data) {
    printf("Telemetry Alert: Active worker node count changed to %lld\n", (long long)new_count);
}

/* Host initialization */
StrataSignalHandle* node_signal = strata_signal_create_int(12);

/* Register callback */
uint64_t sub_id = strata_signal_subscribe_int(node_signal, on_active_nodes_changed, NULL);

/* Mutation triggers callback synchronously */
strata_signal_set_int(node_signal, 16);

/* Cleanup */
strata_signal_unsubscribe_int(node_signal, sub_id);
strata_signal_destroy(node_signal);
```

### Tailwind JIT Evaluation

#### `strata_eval_tailwind_css`
Evaluates Tailwind CSS utility strings using Strata's built-in JIT engine and writes the generated CSS declarations directly into a caller-allocated null-terminated buffer:

```c
char css_buffer[512];
int32_t bytes = strata_eval_tailwind_css("w-96 h-64 bg-neutral-900 border", css_buffer, sizeof(css_buffer));
if (bytes > 0) {
    printf("Generated CSS: %s\n", css_buffer);
}
```

### Direct Offscreen Framebuffer Rendering

#### `strata_render_to_buffer`
Renders an offscreen RGBA8 surface directly into caller-allocated memory. This is critical for embedding Strata inside native game engines, DirectX/Metal textures, or custom video streams:

```c
uint32_t width = 1280;
uint32_t height = 720;
size_t buffer_size = width * height * 4;
uint8_t* rgba_surface = (uint8_t*)malloc(buffer_size);

int32_t bytes_written = strata_render_to_buffer(
    width, height,
    15, 23, 42, 255, // RGBA: Slate 900
    rgba_surface
);

/* Blit rgba_surface to GPU texture or host window */
free(rgba_surface);
```

---

## Multi-Language Host Bindings

### Python Integration (`ctypes`)

```python
import ctypes
from ctypes import c_char_p, c_int32, c_int64, c_uint64, c_void_p, CFUNCTYPE

# Load compiled shared library (.so / .dylib / .dll)
strata = ctypes.CDLL("./libstrata_ffi.so")

# Configure return types and argument types
strata.strata_version.restype = c_char_p
strata.strata_signal_create_int.argtypes = [c_int64]
strata.strata_signal_create_int.restype = c_void_p
strata.strata_signal_set_int.argtypes = [c_void_p, c_int64]
strata.strata_signal_get_int.argtypes = [c_void_p]
strata.strata_signal_get_int.restype = c_int64

SIGNAL_CALLBACK = CFUNCTYPE(None, c_int64, c_void_p)
strata.strata_signal_subscribe_int.argtypes = [c_void_p, SIGNAL_CALLBACK, c_void_p]
strata.strata_signal_subscribe_int.restype = c_uint64

print(f"Strata Engine Version: {strata.strata_version().decode('utf-8')}")

@SIGNAL_CALLBACK
def py_telemetry_listener(new_val, user_data):
    print(f"[Python] Ingested cluster load event: {new_val}%")

sig = strata.strata_signal_create_int(25)
sub_id = strata.strata_signal_subscribe_int(sig, py_telemetry_listener, None)

strata.strata_signal_set_int(sig, 78)  # Triggers py_telemetry_listener
strata.strata_signal_destroy(sig)
```

### Swift Integration (macOS / iOS)

```swift
import Foundation

// Assuming bridging header imports "strata.h"
class StrataTelemetryBridge {
    private var signal: OpaquePointer?
    private var subscriptionId: UInt64 = 0

    init(initialMetric: Int64) {
        self.signal = strata_signal_create_int(initialMetric)
        
        let callback: StrataSignalCallbackInt = { newVal, userData in
            print("Swift received telemetry signal update: \(newVal)")
        }
        
        self.subscriptionId = strata_signal_subscribe_int(self.signal, callback, nil)
    }

    func updateMetric(val: Int64) {
        guard let sig = self.signal else { return }
        strata_signal_set_int(sig, val)
    }

    deinit {
        if let sig = self.signal {
            strata_signal_unsubscribe_int(sig, self.subscriptionId)
            strata_signal_destroy(sig)
        }
    }
}
```

### Kotlin / JVM Integration (Foreign Function & Memory API)

```kotlin
import java.lang.foreign.*
import java.lang.invoke.MethodHandle

class StrataNativeBridge {
    companion object {
        init {
            System.loadLibrary("strata_ffi")
        }
    }

    private val arena = Arena.ofConfined()
    private val linker = Linker.nativeLinker()
    private val lookup = SymbolLookup.loaderLookup()

    fun getEngineVersion(): String {
        val versionSymbol = lookup.find("strata_version").orElseThrow()
        val handle = linker.downcallHandle(versionSymbol, FunctionDescriptor.of(ValueLayout.ADDRESS))
        val addr = handle.invoke() as MemorySegment
        return addr.getString(0)
    }
}
```

---

## Memory Safety & Lifetime Invariants

| Invariant | Safety Guarantee | Violation Consequence |
| :--- | :--- | :--- |
| **Handle Deallocation** | Only deallocate handles via `strata_app_destroy` and `strata_signal_destroy`. | Native memory leak or heap fragmentation. |
| **Use-After-Free Prevention** | Host application must discard raw pointer references immediately after calling destroy. | Access violation / Segmentation fault. |
| **Callback Lifetime** | The `user_data` pointer passed to `strata_signal_subscribe_int` must outlive the subscription or be explicitly freed after `unsubscribe`. | Dangling pointer access when callback triggers. |
| **Buffer Overflow Protection** | `strata_eval_tailwind_css` bounds checks written bytes against `max_len`, guaranteeing null termination. | Truncated CSS string; returns `-1` if insufficient buffer size. |
