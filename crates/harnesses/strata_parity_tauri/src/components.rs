//! Inline HTML fixtures for the three-engine visual parity suite.
//!
//! Every fixture is a self-contained document with fixed pixel dimensions and
//! no network resources. The font stack is pinned to DejaVu Sans and DejaVu Sans
//! Mono, which are available to Chromium, WebKitGTK, and Pango on Linux.

/// Shared SSIM threshold for in-scope fixtures.
pub const DEFAULT_SSIM_THRESHOLD: f64 = 0.95;
/// Shared perceptual-match threshold, expressed as a percentage.
pub const DEFAULT_PERCEPTUAL_THRESHOLD: f64 = 95.0;

/// A single visual regression fixture.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Stable fixture name used on the command line and in output directories.
    pub name: &'static str,
    /// Short human-readable description printed by `--list-fixtures`.
    pub description: &'static str,
    /// Complete inline HTML document, including its stylesheet.
    pub html: &'static str,
    /// Whether the Chromium-vs-Strata result contributes to the pass gate.
    pub in_scope: bool,
    /// Minimum accepted SSIM score against Chromium.
    pub ssim_threshold: f64,
    /// Minimum accepted perceptual match percentage against Chromium.
    pub perceptual_threshold: f64,
    /// Fixed document scroll offset captured for both engines.
    pub scroll_y: f32,
    /// Optional note describing a known parity gap for an out-of-scope fixture.
    pub known_gap: Option<&'static str>,
}

/// Returns every fixture in a deterministic order.
pub fn suite_fixtures() -> Vec<Fixture> {
    vec![
        block_box(),
        inline_text(),
        flex_space_between(),
        grid_tracks(),
        float_table(),
        positioned_overlay(),
        scroll_fixed_header(),
        form_controls(),
        opacity_layers(),
        border_styles(),
        image_paint(),
        default_line_height(),
    ]
}

/// Looks up a fixture by its stable name.
pub fn find_fixture(name: &str) -> Option<Fixture> {
    suite_fixtures()
        .into_iter()
        .find(|fixture| fixture.name == name)
}

fn block_box() -> Fixture {
    Fixture {
        name: "block_box",
        description: "Block box model, borders, padding, and rounded corners",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 40px; font-family: "DejaVu Sans", sans-serif; color: #e5e7eb; }
  .frame { width: 720px; height: 360px; padding: 24px; background: #18181b; border: 2px solid #3f3f46; border-radius: 18px; }
  .frame-title { font-size: 22px; line-height: 28px; font-weight: 700; color: #fafafa; margin-bottom: 16px; }
  .bar { width: 640px; height: 72px; background: #27272a; border-left: 6px solid #38bdf8; border-radius: 8px; }
  .bar-label { width: 600px; height: 72px; font-size: 16px; line-height: 72px; font-weight: 600; color: #bae6fd; padding-left: 18px; }
  .footer { width: 720px; height: 92px; margin-top: 20px; padding: 16px; background: #111827; border: 1px solid #334155; border-radius: 10px; }
  .footer-label { font-size: 14px; line-height: 20px; font-weight: 400; color: #94a3b8; }
  .footer-value { font-size: 18px; line-height: 26px; font-weight: 700; color: #f8fafc; margin-top: 4px; }
</style>
</head>
<body>
  <div class="frame">
    <div class="frame-title">Block Box Model</div>
    <div class="bar"><div class="bar-label">Border, padding, radius</div></div>
    <div class="footer">
      <div class="footer-label">Computed content box</div>
      <div class="footer-value">640 x 72 pixels</div>
    </div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn inline_text() -> Fixture {
    Fixture {
        name: "inline_text",
        description: "Inline text runs with mixed weights, italics, code, and line height",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 36px; font-family: "DejaVu Sans", sans-serif; color: #cbd5e1; }
  .heading { width: 900px; font-size: 26px; line-height: 34px; font-weight: 700; color: #f8fafc; margin-bottom: 18px; }
  .copy { width: 900px; font-size: 18px; line-height: 30px; font-weight: 400; color: #cbd5e1; }
  .copy .light { font-weight: 300; color: #94a3b8; }
  .copy .medium { font-weight: 500; color: #e2e8f0; }
  .copy .bold { font-weight: 700; color: #fafafa; }
  .copy .italic { font-style: italic; color: #fcd34d; }
  .copy code { font-family: "DejaVu Sans Mono", monospace; font-size: 16px; line-height: 24px; color: #7dd3fc; }
  .rule { width: 900px; height: 1px; background: #334155; margin: 22px 0; }
  .caption { width: 900px; font-size: 14px; line-height: 22px; font-weight: 400; color: #64748b; }
</style>
</head>
<body>
  <div class="heading">Inline Formatting Context</div>
  <p class="copy">
    <span class="light">A light run establishes the baseline.</span>
    <span class="medium"> A medium run follows on the same line box.</span>
    <span class="bold"> A bold run adds emphasis.</span>
    <span class="italic"> An italic run changes posture.</span>
    The paragraph continues with normal weight text so wrapping and leading are exercised across several lines.
  </p>
  <div class="rule"></div>
  <p class="copy">
    Code spans use <code>DejaVu Sans Mono</code> and mixed prose uses <span class="bold">DejaVu Sans</span>.
    Line height stays at <span class="medium">30 pixels</span> while font sizes differ, which verifies baseline
    placement and half-leading calculation inside the inline formatting context.
  </p>
  <div class="rule"></div>
  <div class="caption">Caption text at 14 pixels with a 22 pixel line height.</div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn flex_space_between() -> Fixture {
    Fixture {
        name: "flex_space_between",
        description: "Flex row with space-between distribution and a pill badge",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 40px; font-family: "DejaVu Sans", sans-serif; color: #e5e7eb; }
  .header { width: 1100px; height: 84px; display: flex; justify-content: space-between; align-items: center; padding: 0 24px; background: #18181b; border: 1px solid #3f3f46; border-radius: 12px; }
  .title { font-size: 24px; line-height: 32px; font-weight: 700; color: #fafafa; }
  .subtitle { font-size: 13px; line-height: 18px; font-weight: 400; color: #94a3b8; margin-top: 2px; }
  .badge { width: 186px; height: 30px; padding: 6px 14px; border-radius: 999px; background: #052e16; border: 1px solid #166534; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; text-align: center; }
  .row { width: 1100px; height: 72px; display: flex; justify-content: space-between; align-items: center; margin-top: 20px; padding: 0 20px; background: #111827; border: 1px solid #334155; border-radius: 10px; }
  .row-label { font-size: 16px; line-height: 24px; font-weight: 500; color: #cbd5e1; }
  .row-value { font-size: 16px; line-height: 24px; font-weight: 700; color: #f8fafc; }
</style>
</head>
<body>
  <div class="header">
    <div>
      <div class="title">Edge Network Health</div>
      <div class="subtitle">Three regions reporting</div>
    </div>
    <div class="badge">All Systems Operational</div>
  </div>
  <div class="row">
    <div class="row-label">Requests per second</div>
    <div class="row-value">184,220</div>
  </div>
  <div class="row">
    <div class="row-label">Median response time</div>
    <div class="row-value">18 ms</div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn grid_tracks() -> Fixture {
    Fixture {
        name: "grid_tracks",
        description: "Explicit CSS Grid tracks, gap, and a two-column span",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 36px; font-family: "DejaVu Sans", sans-serif; color: #e5e7eb; }
  .heading { width: 1000px; font-size: 24px; line-height: 32px; font-weight: 700; color: #f8fafc; margin-bottom: 18px; }
  .grid { width: 1000px; display: grid; grid-template-columns: 260px 420px 260px; grid-template-rows: 150px 150px; gap: 16px; }
  .card { background: #18181b; border: 1px solid #3f3f46; border-radius: 12px; padding: 16px; }
  .wide { grid-column: span 2; }
  .card-title { font-size: 14px; line-height: 20px; font-weight: 500; color: #94a3b8; }
  .card-value { font-size: 28px; line-height: 36px; font-weight: 700; color: #f8fafc; margin-top: 8px; }
  .card-note { font-size: 12px; line-height: 18px; font-weight: 400; color: #64748b; margin-top: 6px; }
</style>
</head>
<body>
  <div class="heading">Grid Track Layout</div>
  <div class="grid">
    <div class="card wide">
      <div class="card-title">Primary region</div>
      <div class="card-value">us-east-1</div>
      <div class="card-note">Spans the first two explicit columns</div>
    </div>
    <div class="card">
      <div class="card-title">Latency</div>
      <div class="card-value">12 ms</div>
    </div>
    <div class="card">
      <div class="card-title">Requests</div>
      <div class="card-value">42,018</div>
    </div>
    <div class="card wide">
      <div class="card-title">Secondary region</div>
      <div class="card-value">eu-central-1</div>
      <div class="card-note">Row two, two-column span</div>
    </div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn float_table() -> Fixture {
    Fixture {
        name: "float_table",
        description: "Right float with wrapped prose and a collapsed-border table",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 32px; font-family: "DejaVu Sans", sans-serif; color: #cbd5e1; }
  .article { width: 1100px; font-size: 16px; line-height: 24px; font-weight: 400; color: #cbd5e1; }
  .float-card { float: right; width: 300px; height: 180px; margin: 0 0 16px 24px; padding: 16px; background: #1e293b; border: 1px solid #475569; border-radius: 10px; }
  .float-title { font-size: 16px; line-height: 22px; font-weight: 700; color: #f8fafc; }
  .float-value { font-size: 30px; line-height: 38px; font-weight: 700; color: #7dd3fc; margin-top: 10px; }
  .float-note { font-size: 12px; line-height: 18px; font-weight: 400; color: #94a3b8; margin-top: 8px; }
  .paragraph { margin-bottom: 14px; }
  .table { width: 1000px; margin-top: 20px; border-collapse: collapse; }
  .table th, .table td { border: 1px solid #475569; padding: 8px 10px; text-align: left; font-size: 14px; line-height: 20px; }
  .table th { background: #1e293b; font-weight: 700; color: #f8fafc; }
  .table td { font-weight: 400; color: #cbd5e1; }
</style>
</head>
<body>
  <div class="article">
    <div class="float-card">
      <div class="float-title">Ingress Volume</div>
      <div class="float-value">94.8 GB</div>
      <div class="float-note">Trailing twelve hours across all edges</div>
    </div>
    <p class="paragraph">The float is taken out of normal flow and positioned against the right edge of the containing block. Inline content wraps around its margin box on every line that intersects the float, which exercises line box width adjustment and float clearance together.</p>
    <p class="paragraph">A second paragraph continues alongside the float. Text must not overlap the float, and the first line after the float bottom must return to the full content width. This paragraph is intentionally long enough to cross that boundary so the change in available width is visible in the capture.</p>
    <p class="paragraph">Tables below use the automatic table layout algorithm with collapsed borders. Column widths derive from intrinsic min-content and max-content contributions, while header cells carry a distinct background and weight.</p>
    <table class="table">
      <thead>
        <tr><th>Region</th><th>Requests</th><th>Latency</th><th>Status</th></tr>
      </thead>
      <tbody>
        <tr><td>us-east-1</td><td>420,110</td><td>12 ms</td><td>Healthy</td></tr>
        <tr><td>eu-central-1</td><td>218,540</td><td>18 ms</td><td>Healthy</td></tr>
        <tr><td>ap-northeast-1</td><td>176,202</td><td>24 ms</td><td>Degraded</td></tr>
      </tbody>
    </table>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn positioned_overlay() -> Fixture {
    Fixture {
        name: "positioned_overlay",
        description: "Absolutely positioned overlay with explicit stacking order",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 36px; font-family: "DejaVu Sans", sans-serif; color: #e5e7eb; }
  .heading { width: 1000px; font-size: 24px; line-height: 32px; font-weight: 700; color: #f8fafc; margin-bottom: 18px; }
  .stage { position: relative; width: 1000px; height: 420px; background: #111827; border: 1px solid #374151; border-radius: 16px; overflow: hidden; }
  .layer { position: absolute; border-radius: 12px; }
  .base { left: 40px; top: 48px; width: 420px; height: 220px; background: #1e3a8a; z-index: 1; }
  .middle { left: 220px; top: 120px; width: 420px; height: 220px; background: #7c2d12; z-index: 2; }
  .top { left: 400px; top: 190px; width: 400px; height: 180px; background: #14532d; border: 2px solid #22c55e; z-index: 3; }
  .layer-label { position: absolute; left: 16px; top: 14px; font-size: 15px; line-height: 20px; font-weight: 700; color: #f8fafc; }
  .layer-note { position: absolute; left: 16px; top: 38px; font-size: 12px; line-height: 18px; font-weight: 400; color: #dbeafe; }
</style>
</head>
<body>
  <div class="heading">Positioned Overlay</div>
  <div class="stage">
    <div class="layer base">
      <div class="layer-label">Base layer</div>
      <div class="layer-note">z-index 1</div>
    </div>
    <div class="layer middle">
      <div class="layer-label">Middle layer</div>
      <div class="layer-note">z-index 2</div>
    </div>
    <div class="layer top">
      <div class="layer-label">Top layer</div>
      <div class="layer-note">z-index 3</div>
    </div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn scroll_fixed_header() -> Fixture {
    Fixture {
        name: "scroll_fixed_header",
        description: "Dedicated scroll container with a fixed header at a fixed offset",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html { width: 1280px; height: 720px; background: #0b0d10; color: #e5e7eb; }
  body { width: 1280px; height: 720px; padding: 40px; overflow: hidden; background: #0b0d10; color: #e5e7eb; font-family: "DejaVu Sans", sans-serif; }
  .scroll-wrap { position: relative; width: 1000px; height: 520px; }
  .viewport { width: 1000px; height: 520px; overflow-y: scroll; position: relative; background: #0f172a; border: 1px solid #374151; border-radius: 12px; }
  .topbar { position: fixed; top: 40px; left: 40px; width: 998px; height: 56px; background: #111827; border-bottom: 1px solid #374151; border-radius: 12px 12px 0 0; z-index: 10; }
  .topbar-title { position: absolute; left: 20px; top: 16px; font-size: 18px; line-height: 24px; font-weight: 700; color: #f8fafc; }
  .topbar-badge { position: absolute; right: 20px; top: 14px; width: 120px; height: 28px; padding: 4px 12px; border-radius: 999px; background: #052e16; border: 1px solid #166534; color: #4ade80; font-size: 12px; line-height: 18px; font-weight: 600; text-align: center; }
  .row { width: 958px; height: 100px; margin: 0 0 12px 20px; padding: 14px 16px; background: #18181b; border: 1px solid #3f3f46; border-radius: 10px; }
  .row-title { font-size: 18px; line-height: 24px; font-weight: 700; color: #f8fafc; }
  .row-meta { font-size: 14px; line-height: 20px; font-weight: 400; color: #94a3b8; margin-top: 4px; }
  .row-value { font-size: 16px; line-height: 22px; font-weight: 600; color: #7dd3fc; margin-top: 2px; }
</style>
</head>
<body>
  <div class="scroll-wrap">
    <div class="viewport" id="parity-scroll-root">
      <div class="row"><div class="row-title">Row 01</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0101</div></div>
    <div class="row"><div class="row-title">Row 02</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0202</div></div>
    <div class="row"><div class="row-title">Row 03</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0303</div></div>
    <div class="row"><div class="row-title">Row 04</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0404</div></div>
    <div class="row"><div class="row-title">Row 05</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0505</div></div>
    <div class="row"><div class="row-title">Row 06</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0606</div></div>
    <div class="row"><div class="row-title">Row 07</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0707</div></div>
    <div class="row"><div class="row-title">Row 08</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0808</div></div>
    <div class="row"><div class="row-title">Row 09</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 0909</div></div>
    <div class="row"><div class="row-title">Row 10</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 1010</div></div>
    <div class="row"><div class="row-title">Row 11</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 1111</div></div>
      <div class="row"><div class="row-title">Row 12</div><div class="row-meta">Scrolled container content</div><div class="row-value">value 1212</div></div>
    </div>
    <div class="topbar">
      <div class="topbar-title">Fixed Header</div>
      <div class="topbar-badge">Live</div>
    </div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 600.0,
        known_gap: None,
    }
}

fn form_controls() -> Fixture {
    Fixture {
        name: "form_controls",
        description: "Text input, button, select, and checkbox with user-agent defaults",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html, body { width: 1280px; height: 720px; overflow: hidden; background: #0b0d10; }
  body { padding: 40px; font-family: "DejaVu Sans", sans-serif; color: #e5e7eb; }
  .form { width: 640px; padding: 24px; background: #f8fafc; color: #111827; border: 1px solid #d4d4d8; border-radius: 12px; }
  .form-title { font-size: 22px; line-height: 30px; font-weight: 700; color: #111827; margin-bottom: 18px; }
  .field { margin-bottom: 18px; }
  .label { display: block; font-size: 14px; line-height: 20px; font-weight: 600; color: #334155; margin-bottom: 6px; }
  input, button, select { font-family: "DejaVu Sans", sans-serif; }
  .text-input { width: 320px; height: 36px; font-size: 16px; line-height: 24px; }
  .select { width: 220px; height: 36px; font-size: 16px; line-height: 24px; }
  .button { height: 40px; padding: 0 18px; font-size: 15px; line-height: 24px; font-weight: 600; }
  .check { width: 18px; height: 18px; margin-right: 8px; vertical-align: middle; }
  .check-label { display: inline-block; font-size: 15px; line-height: 24px; font-weight: 400; color: #334155; vertical-align: middle; }
</style>
</head>
<body>
  <div class="form">
    <div class="form-title">Form Controls</div>
    <div class="field">
      <label class="label" for="name">Display name</label>
      <input class="text-input" id="name" type="text" value="Strata User">
    </div>
    <div class="field">
      <label class="label" for="region">Region</label>
      <select class="select" id="region">
        <option>us-east-1</option>
        <option>eu-central-1</option>
        <option>ap-northeast-1</option>
      </select>
    </div>
    <div class="field">
      <input class="check" id="active" type="checkbox" checked>
      <label class="check-label" for="active">Enable telemetry</label>
    </div>
    <div class="field">
      <button class="button" type="button">Save changes</button>
    </div>
  </div>
</body>
</html>"#,
        in_scope: false,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: Some("Strata does not render native checkbox, radio, select, range, or date widgets; select flattens to a label and controls have no native appearance."),
    }
}

fn opacity_layers() -> Fixture {
    Fixture {
        name: "opacity-layers",
        description: "Three overlapping squares at opacity 0.5, 0.75, and 1.0",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html { width: 1280px; height: 720px; background: #ffffff; color: #111111; }
  body { width: 1280px; height: 720px; overflow: hidden; background: #ffffff; color: #111111; }
  .stage { position: relative; width: 1000px; height: 440px; margin: 40px; background: #f8fafc; border: 1px solid #cbd5e1; border-radius: 12px; }
  .square { position: absolute; width: 260px; height: 260px; border-radius: 8px; }
  .low { left: 80px; top: 80px; background: #dc2626; opacity: 0.5; }
  .mid { left: 260px; top: 120px; background: #2563eb; opacity: 0.75; }
  .full { left: 440px; top: 80px; background: #16a34a; opacity: 1.0; }
  .label { position: absolute; left: 16px; top: 12px; font-family: "DejaVu Sans", sans-serif; font-size: 15px; font-weight: 700; color: #111111; }
  .note { position: absolute; left: 16px; bottom: 12px; font-family: "DejaVu Sans", sans-serif; font-size: 12px; font-weight: 400; color: #475569; }
</style>
</head>
<body>
  <div class="stage">
    <div class="label">Opacity Composition</div>
    <div class="square low"></div>
    <div class="square mid"></div>
    <div class="square full"></div>
    <div class="note">Red 0.50, blue 0.75, green 1.00 against an explicit white canvas</div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn border_styles() -> Fixture {
    Fixture {
        name: "border-styles",
        description: "Solid, dashed, dotted, and double borders at four pixels",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html { width: 1280px; height: 720px; background: #ffffff; color: #111111; }
  body { width: 1280px; height: 720px; overflow: hidden; padding: 40px; background: #ffffff; color: #111111; }
  .title { width: 1100px; font-family: "DejaVu Sans", sans-serif; font-size: 24px; font-weight: 700; color: #111111; margin-bottom: 20px; }
  .row { display: flex; gap: 20px; width: 1100px; }
  .box { width: 240px; height: 180px; padding: 16px; background: #f1f5f9; border: 4px solid #0f172a; border-radius: 6px; }
  .solid { border-style: solid; }
  .dashed { border-style: dashed; }
  .dotted { border-style: dotted; }
  .double { border-style: double; }
  .box-label { font-family: "DejaVu Sans", sans-serif; font-size: 15px; font-weight: 700; color: #0f172a; }
  .box-note { font-family: "DejaVu Sans", sans-serif; font-size: 12px; font-weight: 400; color: #475569; margin-top: 8px; }
</style>
</head>
<body>
  <div class="title">Border Style Rendering</div>
  <div class="row">
    <div class="box solid"><div class="box-label">Solid</div><div class="box-note">border-style: solid</div></div>
    <div class="box dashed"><div class="box-label">Dashed</div><div class="box-note">border-style: dashed</div></div>
    <div class="box dotted"><div class="box-label">Dotted</div><div class="box-note">border-style: dotted</div></div>
    <div class="box double"><div class="box-label">Double</div><div class="box-note">border-style: double</div></div>
  </div>
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn image_paint() -> Fixture {
    Fixture {
        name: "image_paint",
        description: "Raster PNG data URI painted at exact intrinsic size",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html { width: 1280px; height: 720px; background: #ffffff; color: #111111; }
  body { width: 1280px; height: 720px; overflow: hidden; padding: 40px; background: #ffffff; color: #111111; font-family: "DejaVu Sans", sans-serif; }
  .title { width: 1100px; font-family: "DejaVu Sans", sans-serif; font-size: 24px; font-weight: 700; color: #111111; margin-bottom: 20px; }
  .caption { width: 1100px; font-family: "DejaVu Sans", sans-serif; font-size: 14px; font-weight: 400; color: #334155; margin-bottom: 16px; }
  .swatch { display: block; width: 240px; height: 160px; }
</style>
</head>
<body>
  <div class="title">Raster Image Paint</div>
  <div class="caption">Embedded PNG data URI, 240 by 160 pixels</div>
  <img class="swatch" width="240" height="160" alt="Color bands" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAPAAAACgCAYAAAAy2+FlAAADGUlEQVR42u3TIQGAMBQFwGWhBvbnmFoEEsySizxz2BEC+U5chGuraic5nh7lnG+Ucd1RmsACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyywwAILLLDAAgsssMACCyzwPx+zKMKCBRNk1AAAAABJRU5ErkJggg==">
</body>
</html>"#,
        in_scope: true,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: None,
    }
}

fn default_line_height() -> Fixture {
    Fixture {
        name: "default-line-height",
        description: "Heading, paragraph, and table with no line-height declarations",
        html: r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; animation: none !important; transition: none !important; }
  html { width: 1280px; height: 720px; background: #ffffff; color: #111111; }
  body { width: 1280px; height: 720px; overflow: hidden; padding: 40px; background: #ffffff; color: #111111; font-family: "DejaVu Sans", sans-serif; }
  .heading { width: 900px; font-size: 28px; font-weight: 700; color: #111111; margin-bottom: 18px; }
  .paragraph { width: 900px; font-size: 16px; font-weight: 400; color: #334155; margin-bottom: 24px; }
  .table { width: 900px; border-collapse: collapse; }
  .table th, .table td { border: 1px solid #94a3b8; padding: 8px 12px; text-align: left; font-size: 15px; font-weight: 400; color: #111111; }
  .table th { background: #e2e8f0; font-weight: 700; }
</style>
</head>
<body>
  <div class="heading">Default Line Height</div>
  <div class="paragraph">This paragraph deliberately has no line-height declaration. Chromium resolves the normal value from font metrics, while the Strata reader root currently injects a fixed multiplier that changes line box heights and the following table geometry.</div>
  <table class="table">
    <tbody>
      <tr><th>Metric</th><th>Value</th></tr>
      <tr><td>Normal line height</td><td>Font metrics derived</td></tr>
      <tr><td>Injected line height</td><td>Fixed multiplier</td></tr>
    </tbody>
  </table>
</body>
</html>"#,
        in_scope: false,
        ssim_threshold: DEFAULT_SSIM_THRESHOLD,
        perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
        scroll_y: 0.0,
        known_gap: Some("The Strata reader root injects line-height 1.5 while Chromium uses normal font-metric line height, shifting table rows."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_names_are_unique_and_cover_the_required_topics() {
        let fixtures = suite_fixtures();
        assert_eq!(fixtures.len(), 12);
        let mut names: Vec<&str> = fixtures.iter().map(|f| f.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 12);

        for fixture in &fixtures {
            assert!(fixture.ssim_threshold > 0.0);
            assert!(fixture.perceptual_threshold > 0.0);
            assert!(fixture.html.contains("DejaVu Sans"));
            assert!(fixture.html.contains("animation: none"));
            assert!(fixture.html.contains("transition: none"));
            assert!(!fixture.html.contains("http://"));
            assert!(!fixture.html.contains("https://"));
            assert!(
                fixture.html.contains("html, body {")
                    || (fixture.html.contains("html {") && fixture.html.contains("body {")),
                "{} must declare html and body rules",
                fixture.name
            );
            assert!(
                fixture.html.matches("background:").count() >= 2,
                "{} must set explicit backgrounds on html and body",
                fixture.name
            );
            if !fixture.in_scope {
                assert!(fixture.known_gap.is_some());
            }
        }

        let scroll = find_fixture("scroll_fixed_header").expect("scroll fixture");
        assert!(scroll.scroll_y > 0.0);
        assert!(scroll.html.contains("position: fixed"));
        let forms = find_fixture("form_controls").expect("form fixture");
        assert!(forms.html.contains("<input"));
        assert!(forms.html.contains("<button"));
        assert!(forms.html.contains("<select"));
        assert!(forms.html.contains("type=\"checkbox\""));
    }
}
