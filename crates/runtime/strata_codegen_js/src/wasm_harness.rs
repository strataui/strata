//! WebAssembly Canvas Surface Host Harness & Event Pump Generator (Mode A).

/// Generates the HTML5 canvas host page and JavaScript loader for Mode A Web-WASM targets.
pub struct WasmHarnessGenerator;

impl WasmHarnessGenerator {
    /// Generates the HTML5 host page mounting the Web-WASM canvas surface.
    pub fn generate_index_html(
        app_title: &str,
        canvas_id: &str,
        loader_js_path: &str,
    ) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
  <title>{}</title>
  <style>
    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
      -webkit-touch-callout: none;
      -webkit-user-select: none;
      user-select: none;
    }}
    html, body {{
      width: 100%;
      height: 100%;
      overflow: hidden;
      background-color: #09090b;
      touch-action: none;
    }}
    #{} {{
      display: block;
      width: 100vw;
      height: 100vh;
      outline: none;
    }}
  </style>
</head>
<body>
  <canvas id="{}" tabindex="0"></canvas>
  <script type="module" src="{}"></script>
</body>
</html>
"#,
            app_title, canvas_id, canvas_id, loader_js_path
        )
    }

    /// Generates the JavaScript loader script handling WASM instantiating, HiDPI DPR scaling, and event pumps.
    pub fn generate_wasm_loader_js(
        canvas_id: &str,
        wasm_binary_path: &str,
    ) -> String {
        format!(
            r#"// Strata Web-WASM Host Harness & Event Pump
const canvas = document.getElementById('{canvas_id}');
let wasmInstance = null;

function updateCanvasResolution() {{
  const dpr = window.devicePixelRatio || 1;
  const width = window.innerWidth;
  const height = window.innerHeight;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);

  if (wasmInstance && wasmInstance.exports.strata_on_resize) {{
    wasmInstance.exports.strata_on_resize(canvas.width, canvas.height, dpr);
  }}
}}

window.addEventListener('resize', updateCanvasResolution);
window.addEventListener('orientationchange', updateCanvasResolution);

// Event Bridges
function bindEventHandlers() {{
  // Mouse & Pointer
  canvas.addEventListener('pointermove', (e) => {{
    if (wasmInstance && wasmInstance.exports.strata_on_mouse_move) {{
      const dpr = window.devicePixelRatio || 1;
      wasmInstance.exports.strata_on_mouse_move(e.clientX * dpr, e.clientY * dpr);
    }}
  }});

  canvas.addEventListener('pointerdown', (e) => {{
    if (wasmInstance && wasmInstance.exports.strata_on_mouse_down) {{
      const dpr = window.devicePixelRatio || 1;
      wasmInstance.exports.strata_on_mouse_down(e.button, e.clientX * dpr, e.clientY * dpr);
    }}
  }});

  canvas.addEventListener('pointerup', (e) => {{
    if (wasmInstance && wasmInstance.exports.strata_on_mouse_up) {{
      const dpr = window.devicePixelRatio || 1;
      wasmInstance.exports.strata_on_mouse_up(e.button, e.clientX * dpr, e.clientY * dpr);
    }}
  }});

  // Wheel
  canvas.addEventListener('wheel', (e) => {{
    e.preventDefault();
    if (wasmInstance && wasmInstance.exports.strata_on_scroll) {{
      wasmInstance.exports.strata_on_scroll(e.deltaX, e.deltaY);
    }}
  }}, {{ passive: false }});

  // Keyboard
  window.addEventListener('keydown', (e) => {{
    if (wasmInstance && wasmInstance.exports.strata_on_key_down) {{
      wasmInstance.exports.strata_on_key_down(e.keyCode, e.shiftKey ? 1 : 0, e.ctrlKey ? 1 : 0, e.altKey ? 1 : 0);
    }}
  }});

  window.addEventListener('keyup', (e) => {{
    if (wasmInstance && wasmInstance.exports.strata_on_key_up) {{
      wasmInstance.exports.strata_on_key_up(e.keyCode, e.shiftKey ? 1 : 0, e.ctrlKey ? 1 : 0, e.altKey ? 1 : 0);
    }}
  }});
}}

// RequestAnimationFrame Render Loop
function startRenderLoop() {{
  function renderFrame(timestamp) {{
    if (wasmInstance && wasmInstance.exports.strata_render_frame) {{
      wasmInstance.exports.strata_render_frame(timestamp);
    }}
    requestAnimationFrame(renderFrame);
  }}
  requestAnimationFrame(renderFrame);
}}

// Initialization
async function init() {{
  updateCanvasResolution();
  bindEventHandlers();

  try {{
    const response = await fetch('{wasm_binary_path}');
    const {{ instance }} = await WebAssembly.instantiateStreaming(response, {{
      env: {{
        memory: new WebAssembly.Memory({{ initial: 256, maximum: 4096 }}),
        now: () => performance.now(),
        print: (ptr, len) => {{
          // String printing bridge from WASM linear memory
        }}
      }}
    }});
    wasmInstance = instance;

    if (wasmInstance.exports.strata_init) {{
      wasmInstance.exports.strata_init();
    }}
    updateCanvasResolution();
    startRenderLoop();
  }} catch (err) {{
    console.error('Failed to load Strata Web-WASM binary:', err);
  }}
}}

init();
"#,
            canvas_id = canvas_id,
            wasm_binary_path = wasm_binary_path
        )
    }
}
