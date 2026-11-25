/* Pure C99 smoke harness verifying include/strata.h compiles cleanly under
 * `-std=c99 -Wall -Wextra -Werror` and exercises the complete strata-ffi ABI. */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "strata.h"

typedef struct ClickState {
    StrataSignalHandle *counter_sig;
    int clicks;
} ClickState;

static void on_button_click(const StrataDomEvent *ev, void *user_data) {
    ClickState *state = (ClickState *)user_data;
    assert(ev != NULL);
    assert(ev->event_kind == 1u);
    state->clicks += 1;
    int64_t next = strata_signal_get_int(state->counter_sig) + 1;
    strata_signal_set_int(state->counter_sig, next);
}

int main(void) {
    /* 1. Version & error diagnostics */
    const char *ver = strata_version();
    assert(ver != NULL && strlen(ver) > 0);
    strata_clear_last_error();
    assert(strcmp(strata_last_error(), "") == 0);

    /* 2. Typed signals (i64, f64, bool, str + two-call length probe) */
    StrataSignalHandle *count = strata_signal_create_int(10);
    StrataSignalHandle *ratio = strata_signal_create_f64(1.5);
    StrataSignalHandle *flag = strata_signal_create_bool(true);
    StrataSignalHandle *label = strata_signal_create_str("Strata C99");
    assert(count != NULL && ratio != NULL && flag != NULL && label != NULL);

    assert(strata_signal_get_int(count) == 10);
    assert(strata_signal_set_f64(ratio, 2.25) == STRATA_STATUS_OK);
    assert(strata_signal_get_f64(ratio) > 2.24 && strata_signal_get_f64(ratio) < 2.26);
    assert(strata_signal_get_bool(flag) == true);
    assert(strata_signal_set_bool(flag, false) == STRATA_STATUS_OK);
    assert(strata_signal_get_bool(flag) == false);

    size_t needed = 0;
    assert(strata_signal_get_str(label, NULL, 0, &needed) == STRATA_STATUS_OK);
    assert(needed == strlen("Strata C99") + 1);
    char str_buf[64];
    size_t written = 0;
    assert(strata_signal_get_str(label, str_buf, sizeof(str_buf), &written) == STRATA_STATUS_OK);
    assert(strcmp(str_buf, "Strata C99") == 0);

    /* 3. Standards-compliant Tailwind CSS evaluation (no Rust Debug "Px(...)") */
    size_t css_needed = 0;
    assert(
        strata_eval_tailwind_css_checked("w-64 h-32 bg-blue-500 rounded-md", NULL, 0, &css_needed)
        == STRATA_STATUS_OK
    );
    assert(css_needed > 20);
    char *css_buf = (char *)malloc(css_needed);
    assert(css_buf != NULL);
    size_t css_written = 0;
    assert(
        strata_eval_tailwind_css_checked(
            "w-64 h-32 bg-blue-500 rounded-md",
            css_buf,
            css_needed,
            &css_written
        ) == STRATA_STATUS_OK
    );
    assert(strstr(css_buf, "width: 256px;") != NULL);
    assert(strstr(css_buf, "height: 128px;") != NULL);
    assert(strstr(css_buf, "background-color: rgba(59, 130, 246, 1);") != NULL);
    assert(strstr(css_buf, "Px(") == NULL);
    free(css_buf);

    /* 4. C DOM & View builders with live dynamic text + click handler */
    ClickState state;
    state.counter_sig = count;
    state.clicks = 0;

    StrataNodeHandle *root = strata_node_element("div");
    assert(strata_node_set_class(root, "w-[160px] h-[120px] bg-red-500 flex flex-col") == STRATA_STATUS_OK);

    StrataNodeHandle *btn = strata_node_element("button");
    assert(strata_node_set_attr(btn, "id", "inc-btn") == STRATA_STATUS_OK);
    assert(strata_node_set_class(btn, "w-[100px] h-[60px] bg-blue-500") == STRATA_STATUS_OK);
    assert(strata_node_on_event(btn, "click", on_button_click, &state) == STRATA_STATUS_OK);

    StrataNodeHandle *dyn_txt = strata_node_dynamic_text_signal(count);
    assert(dyn_txt != NULL);
    assert(strata_node_append_child(btn, dyn_txt) == STRATA_STATUS_OK);
    assert(strata_node_append_child(root, btn) == STRATA_STATUS_OK);

    StrataViewHandle *view = strata_view_from_root(root);
    assert(view != NULL);

    /* 5. AppRuntime creation, click dispatch, tick, checked RGBA render & PNG export */
    StrataRuntimeHandle *rt = strata_runtime_create_from_view(view, 160.0f, 120.0f, 1.0f);
    assert(rt != NULL);

    bool dirty = false;
    assert(strata_runtime_pointer_move(rt, 30.0f, 25.0f, &dirty) == STRATA_STATUS_OK);
    assert(strata_runtime_pointer_down(rt, 30.0f, 25.0f, 0, &dirty) == STRATA_STATUS_OK);
    assert(strata_runtime_pointer_up(rt, 30.0f, 25.0f, 0, &dirty) == STRATA_STATUS_OK);
    assert(state.clicks == 1);
    assert(strata_signal_get_int(count) == 11);

    assert(strata_runtime_tick(rt, &dirty) == STRATA_STATUS_OK);

    /* Checked RGBA render: verify BufferTooSmall guard before full render */
    StrataFramebufferInfo info;
    assert(strata_runtime_framebuffer_info(rt, &info) == STRATA_STATUS_OK);
    assert(info.width == 160 && info.height == 120 && info.stride == 640);
    assert(info.byte_len == 160u * 120u * 4u);

    uint8_t tiny[16];
    size_t req_bytes = 0;
    assert(
        strata_runtime_render_rgba(rt, tiny, sizeof(tiny), &req_bytes)
        == STRATA_STATUS_BUFFER_TOO_SMALL
    );
    assert(req_bytes == info.byte_len);
    assert(strlen(strata_last_error()) > 0);

    uint8_t *pixels = (uint8_t *)malloc(info.byte_len);
    assert(pixels != NULL);
    size_t rendered = 0;
    assert(
        strata_runtime_render_rgba(rt, pixels, info.byte_len, &rendered)
        == STRATA_STATUS_OK
    );
    assert(rendered == info.byte_len);

    /* Top-left pixel inside blue button (20,20) must be blue-dominant; (140,100) red-dominant */
    size_t blue_off = (20u * info.width + 20u) * 4u;
    size_t red_off = (100u * info.width + 140u) * 4u;
    assert(pixels[blue_off + 2] > 180 && pixels[blue_off + 0] < 120);
    assert(pixels[red_off + 0] > 180 && pixels[red_off + 2] < 120);
    free(pixels);

    /* Resize and headless native window launch */
    assert(strata_runtime_resize(rt, 200.0f, 150.0f, 1.0f) == STRATA_STATUS_OK);
    assert(strata_runtime_framebuffer_info(rt, &info) == STRATA_STATUS_OK);
    assert(info.width == 200 && info.height == 150);

    strata_runtime_destroy(rt);
    strata_view_destroy(view);
    strata_signal_destroy(count);
    strata_signal_destroy(ratio);
    strata_signal_destroy(flag);
    strata_signal_destroy(label);

    printf("C99 strata-ffi smoke test passed (v%s)\n", ver);
    return 0;
}
