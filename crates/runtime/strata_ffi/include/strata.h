/* Strata Universal C99 FFI Header - matches strata-ffi ABI */

#ifndef STRATA_FFI_H
#define STRATA_FFI_H

#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Status & Diagnostics
 * ============================================================================ */

typedef enum StrataStatus {
    STRATA_STATUS_OK = 0,
    STRATA_STATUS_NULL_POINTER = 1,
    STRATA_STATUS_INVALID_UTF8 = 2,
    STRATA_STATUS_BUFFER_TOO_SMALL = 3,
    STRATA_STATUS_INVALID_HANDLE = 4,
    STRATA_STATUS_PANIC = 5,
    STRATA_STATUS_RUNTIME_ERROR = 6,
    STRATA_STATUS_INVALID_ARGUMENT = 7
} StrataStatus;

/* Modifier bitflags for strata_runtime_key_down / strata_runtime_key_up */
#define STRATA_MOD_SHIFT 0x01u
#define STRATA_MOD_CTRL  0x02u
#define STRATA_MOD_ALT   0x04u
#define STRATA_MOD_META  0x08u

/* Opaque handle types */
typedef struct StrataAppHandle StrataAppHandle;
typedef struct StrataWindowHandle StrataWindowHandle;
typedef struct StrataRuntimeHandle StrataRuntimeHandle;
typedef struct StrataSignalHandle StrataSignalHandle;
typedef struct StrataNodeHandle StrataNodeHandle;
typedef struct StrataViewHandle StrataViewHandle;

/* C-compatible event payload passed to strata_node_on_event callbacks */
typedef struct StrataDomEvent {
    uint32_t event_kind;
    double client_x;
    double client_y;
    int16_t button;
    const char *value;
} StrataDomEvent;

typedef void (*StrataEventCallback)(const StrataDomEvent *event, void *user_data);
typedef void (*SignalCallbackInt)(int64_t new_value, void *user_data);

/* Framebuffer geometry metadata returned by strata_runtime_framebuffer_info */
typedef struct StrataFramebufferInfo {
    uint32_t width;
    uint32_t height;
    uint32_t stride;
    size_t byte_len;
} StrataFramebufferInfo;

/* Options passed to strata_launch_native_window */
typedef struct StrataWindowOptions {
    const char *title;
    uint32_t width;
    uint32_t height;
    bool resizable;
    bool headless;
} StrataWindowOptions;

/* Version & thread-local error diagnostics */
const char *strata_version(void);
const char *strata_last_error(void);
void strata_clear_last_error(void);

/* ============================================================================
 * Reactive Signals (i64, f64, bool, UTF-8 string)
 * ============================================================================ */

StrataSignalHandle *strata_signal_create_int(int64_t initial);
int64_t strata_signal_get_int(const StrataSignalHandle *sig);
void strata_signal_set_int(StrataSignalHandle *sig, int64_t value);
uint64_t strata_signal_subscribe_int(
    StrataSignalHandle *sig,
    SignalCallbackInt callback,
    void *user_data
);
int32_t strata_signal_unsubscribe_int(StrataSignalHandle *sig, uint64_t sub_id);

StrataSignalHandle *strata_signal_create_f64(double initial);
double strata_signal_get_f64(const StrataSignalHandle *sig);
StrataStatus strata_signal_set_f64(StrataSignalHandle *sig, double value);

StrataSignalHandle *strata_signal_create_bool(bool initial);
bool strata_signal_get_bool(const StrataSignalHandle *sig);
StrataStatus strata_signal_set_bool(StrataSignalHandle *sig, bool value);

StrataSignalHandle *strata_signal_create_str(const char *initial);
StrataStatus strata_signal_get_str(
    const StrataSignalHandle *sig,
    char *out_buf,
    size_t buf_len,
    size_t *out_written
);
StrataStatus strata_signal_set_str(StrataSignalHandle *sig, const char *value);

void strata_signal_destroy(StrataSignalHandle *sig);

/* ============================================================================
 * DOM & View Builders
 * ============================================================================ */

StrataNodeHandle *strata_node_element(const char *tag);
StrataNodeHandle *strata_node_text(const char *text);
StrataNodeHandle *strata_node_dynamic_text_signal(const StrataSignalHandle *sig);
StrataStatus strata_node_set_class(StrataNodeHandle *node, const char *classes);
StrataStatus strata_node_set_attr(
    StrataNodeHandle *node,
    const char *key,
    const char *value
);
/* Ownership note: takes ownership of `child` on STRATA_STATUS_OK. */
StrataStatus strata_node_append_child(
    StrataNodeHandle *parent,
    StrataNodeHandle *child
);
StrataStatus strata_node_on_event(
    StrataNodeHandle *node,
    const char *event_name,
    StrataEventCallback callback,
    void *user_data
);
/* Only call on nodes NOT transferred via append_child or view_from_root. */
void strata_node_destroy(StrataNodeHandle *node);

/* Ownership note: takes ownership of `root` on non-NULL return. */
StrataViewHandle *strata_view_from_root(StrataNodeHandle *root);
void strata_view_destroy(StrataViewHandle *view);

/* ============================================================================
 * AppRuntime Lifecycle, Input Dispatch, Rendering & Native Window Launch
 * ============================================================================ */

StrataRuntimeHandle *strata_runtime_create_from_view(
    const StrataViewHandle *view,
    float width,
    float height,
    float dpr
);
StrataStatus strata_runtime_update_view(
    StrataRuntimeHandle *runtime,
    const StrataViewHandle *view
);
StrataStatus strata_runtime_resize(
    StrataRuntimeHandle *runtime,
    float width,
    float height,
    float dpr
);
StrataStatus strata_runtime_tick(StrataRuntimeHandle *runtime, bool *out_dirty);

StrataStatus strata_runtime_pointer_move(
    StrataRuntimeHandle *runtime,
    float x,
    float y,
    bool *out_dirty
);
StrataStatus strata_runtime_pointer_down(
    StrataRuntimeHandle *runtime,
    float x,
    float y,
    int16_t button,
    bool *out_dirty
);
StrataStatus strata_runtime_pointer_up(
    StrataRuntimeHandle *runtime,
    float x,
    float y,
    int16_t button,
    bool *out_dirty
);
StrataStatus strata_runtime_wheel(
    StrataRuntimeHandle *runtime,
    float x,
    float y,
    float delta_x,
    float delta_y,
    bool *out_dirty
);
StrataStatus strata_runtime_key_down(
    StrataRuntimeHandle *runtime,
    const char *key_name,
    uint32_t modifiers_bits,
    bool *out_dirty
);
StrataStatus strata_runtime_key_up(
    StrataRuntimeHandle *runtime,
    const char *key_name,
    uint32_t modifiers_bits,
    bool *out_dirty
);
StrataStatus strata_runtime_text_input(
    StrataRuntimeHandle *runtime,
    const char *utf8_text,
    bool *out_dirty
);

StrataStatus strata_runtime_framebuffer_info(
    const StrataRuntimeHandle *runtime,
    StrataFramebufferInfo *out_info
);
StrataStatus strata_runtime_render_rgba(
    StrataRuntimeHandle *runtime,
    uint8_t *out_rgba,
    size_t buf_len,
    size_t *out_written
);
StrataStatus strata_runtime_export_png(
    StrataRuntimeHandle *runtime,
    const char *path
);
void strata_runtime_destroy(StrataRuntimeHandle *runtime);

/* Ownership note: takes ownership of `view` on call. */
StrataStatus strata_launch_native_window(
    StrataViewHandle *view,
    const StrataWindowOptions *options
);

/* ============================================================================
 * Styling & Legacy App/Window Compatibility API
 * ============================================================================ */

StrataStatus strata_eval_tailwind_css_checked(
    const char *classes,
    char *out_buf,
    size_t buf_len,
    size_t *out_written
);
int32_t strata_eval_tailwind_css(
    const char *classes,
    char *out_buf,
    size_t max_len
);
int32_t strata_render_to_buffer(
    uint32_t width,
    uint32_t height,
    uint8_t r,
    uint8_t g,
    uint8_t b,
    uint8_t a,
    uint8_t *out_rgba_ptr
);

StrataAppHandle *strata_app_create(void);
void strata_app_destroy(StrataAppHandle *app);
int32_t strata_app_is_running(const StrataAppHandle *app);

StrataWindowHandle *strata_window_create(
    StrataAppHandle *app,
    const char *title,
    uint32_t width,
    uint32_t height
);
int32_t strata_window_set_runtime(
    StrataWindowHandle *win,
    StrataRuntimeHandle *runtime
);
int32_t strata_window_render_frame(StrataWindowHandle *win, uint8_t *out_rgba_ptr);
int32_t strata_window_set_title(StrataWindowHandle *win, const char *title);
uint32_t strata_window_get_width(const StrataWindowHandle *win);
uint32_t strata_window_get_height(const StrataWindowHandle *win);
void strata_window_destroy(StrataWindowHandle *win);

/* ============================================================================
 * Universal Core I/O, Real-Time Voice DSP, Vector PDF, Notifications & Perms
 * ============================================================================ */

typedef struct StrataVoicePipelineHandle StrataVoicePipelineHandle;

typedef struct StrataVoiceStats {
    bool is_speech;
    float speech_probability;
    float estimated_snr_db;
    float erle_db;
    bool double_talk;
    float agc_gain_db;
    float latency_ms;
} StrataVoiceStats;

StrataVoicePipelineHandle *strata_audio_voice_pipeline_new(
    uint32_t sample_rate,
    uint32_t preset_id
);
StrataStatus strata_audio_voice_pipeline_set_ns_preset(
    StrataVoicePipelineHandle *handle,
    uint32_t ns_preset
);
StrataStatus strata_audio_voice_pipeline_process(
    StrataVoicePipelineHandle *handle,
    const float *mic_in,
    const float *far_in,
    float *out_pcm,
    size_t frame_len,
    StrataVoiceStats *out_stats
);
void strata_audio_voice_pipeline_destroy(StrataVoicePipelineHandle *handle);

StrataStatus strata_runtime_export_pdf(
    StrataRuntimeHandle *runtime,
    const char *path
);
StrataStatus strata_notify_send(
    const char *title,
    const char *body,
    uint8_t urgency,
    uint32_t *out_id
);
int32_t strata_permission_check(uint32_t kind_id);

#ifdef __cplusplus
}
#endif

#endif /* STRATA_FFI_H */
