//! Progressive Parity Fixtures: from atomic primitives to complex telemetry dashboards.

use strata::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityLevel {
    /// Level 1: Pure Box Model - Solid Rectangles & Borders
    Level1BoxModel,
    /// Level 2: Rounded Rectangles - Corner Curvature
    Level2RoundedCorner,
    /// Level 3: Typography & Text Color
    Level3Typography,
    /// Level 4: Flexbox Row Alignment & Pill Badge
    Level4FlexRow,
    /// Level 5: Full Multi-Card Dashboard
    Level5FullDashboard,
    /// Level 6: 2D CSS Grid with Gaps, MinMax Tracks & Multi-Span Cards
    Level6CssGrid2D,
    /// Level 7: High-Complexity Multi-Section Cloud Telemetry Console
    Level7TelemetryConsole,
}

impl ParityLevel {
    pub fn from_u32(level: u32) -> Self {
        match level {
            1 => Self::Level1BoxModel,
            2 => Self::Level2RoundedCorner,
            3 => Self::Level3Typography,
            4 => Self::Level4FlexRow,
            5 => Self::Level5FullDashboard,
            6 => Self::Level6CssGrid2D,
            _ => Self::Level7TelemetryConsole,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Level1BoxModel => "Level 1: Pure Box Model (Solid Rectangles & Borders)",
            Self::Level2RoundedCorner => "Level 2: Rounded Rectangles (Corner Curvature Geometry)",
            Self::Level3Typography => "Level 3: Typography & Subpixel Baseline Placement",
            Self::Level4FlexRow => "Level 4: Flexbox 1D Row Alignment & Space-Between",
            Self::Level5FullDashboard => "Level 5: Full Multi-Card Infrastructure Dashboard",
            Self::Level6CssGrid2D => "Level 6: 2D CSS Grid (Gaps, MinMax Tracks & Multi-Span)",
            Self::Level7TelemetryConsole => {
                "Level 7: High-Complexity Multi-Section Telemetry Console"
            }
        }
    }
}

/// Returns the HTML/CSS document matching the specified parity level.
pub fn get_fixture_html(level: ParityLevel) -> String {
    match level {
        ParityLevel::Level1BoxModel => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; overflow: hidden; padding: 48px; }
  .box { width: 480px; height: 240px; background: #18181b; border: 2px solid #27272a; }
</style>
</head>
<body>
  <div class="box"></div>
</body>
</html>"#.to_string(),

        ParityLevel::Level2RoundedCorner => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; overflow: hidden; padding: 48px; }
  .box { width: 480px; height: 240px; background: #18181b; border: 2px solid #27272a; border-radius: 16px; }
</style>
</head>
<body>
  <div class="box"></div>
</body>
</html>"#.to_string(),

        ParityLevel::Level3Typography => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; overflow: hidden; padding: 48px; color: #ffffff; font-family: sans-serif; }
  .title { font-size: 30px; font-weight: bold; color: #ffffff; line-height: 1.2; }
  .subtitle { font-size: 14px; font-weight: 500; color: #a1a1aa; margin-top: 8px; line-height: 1.2; }
</style>
</head>
<body>
  <h1 class="title">Cloud Infrastructure Health</h1>
  <p class="subtitle">Real-time compute cluster telemetry and edge routing diagnostics</p>
</body>
</html>"#.to_string(),

        ParityLevel::Level4FlexRow => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; color: #ffffff; font-family: sans-serif; overflow: hidden; padding: 32px; }
  .header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 16px; border-bottom: 1px solid #27272a; }
  .title { font-size: 24px; line-height: 32px; font-weight: bold; color: #ffffff; }
  .badge { padding: 4px 12px; background: #052e16; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; border-radius: 9999px; border: 1px solid #166534; }
</style>
</head>
<body>
  <header class="header">
    <h1 class="title">Cloud Infrastructure Health</h1>
    <span class="badge">All Systems Operational</span>
  </header>
</body>
</html>"#.to_string(),

        ParityLevel::Level5FullDashboard => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; color: #ffffff; font-family: sans-serif; overflow: hidden; padding: 32px; }
  .header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 16px; border-bottom: 1px solid #27272a; margin-bottom: 32px; }
  .title { font-size: 24px; line-height: 32px; font-weight: bold; color: #ffffff; }
  .badge { padding: 4px 12px; background: #052e16; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; border-radius: 9999px; border: 1px solid #166534; }
  .grid { display: flex; gap: 24px; }
  .card { flex: 1; padding: 24px; background: #18181b; border: 1px solid #27272a; border-radius: 12px; display: flex; flex-direction: column; }
  .card-title { font-size: 14px; line-height: 20px; font-weight: 500; color: #a1a1aa; }
  .card-value { font-size: 30px; line-height: 36px; font-weight: bold; color: #ffffff; margin-top: 8px; }
  .card-delta { font-size: 12px; line-height: 16px; font-weight: 600; color: #4ade80; margin-top: 4px; }
</style>
</head>
<body>
  <header class="header">
    <h1 class="title">Cloud Infrastructure Health</h1>
    <span class="badge">All Systems Operational</span>
  </header>
  <div class="grid">
    <div class="card">
      <span class="card-title">Active Compute Nodes</span>
      <div class="card-value">1,024</div>
      <span class="card-delta">+12% from last hour</span>
    </div>
    <div class="card">
      <span class="card-title">Average Latency</span>
      <div class="card-value">1.42 ms</div>
      <span class="card-delta">-0.2ms improvement</span>
    </div>
    <div class="card">
      <span class="card-title">Bandwidth Ingress</span>
      <div class="card-value">48.2 Gbps</div>
      <span class="card-delta">+5.4 Gbps burst</span>
    </div>
  </div>
</body>
</html>"#.to_string(),

        ParityLevel::Level6CssGrid2D => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; color: #ffffff; font-family: sans-serif; overflow: hidden; padding: 32px; }
  .header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 16px; border-bottom: 1px solid #27272a; margin-bottom: 32px; }
  .title { font-size: 24px; line-height: 32px; font-weight: bold; color: #ffffff; }
  .badge { padding: 4px 12px; background: #052e16; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; border-radius: 9999px; border: 1px solid #166534; }
  .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 24px; }
  .card { padding: 24px; background: #18181b; border: 1px solid #27272a; border-radius: 12px; display: flex; flex-direction: column; }
  .card-span-2 { grid-column: span 2 / span 2; }
  .card-title { font-size: 14px; line-height: 20px; font-weight: 500; color: #a1a1aa; }
  .card-value { font-size: 30px; line-height: 36px; font-weight: bold; color: #ffffff; margin-top: 8px; }
  .card-delta { font-size: 12px; line-height: 16px; font-weight: 600; color: #4ade80; margin-top: 4px; }
</style>
</head>
<body>
  <header class="header">
    <h1 class="title">Cloud Infrastructure Health</h1>
    <span class="badge">All Systems Operational</span>
  </header>
  <div class="grid">
    <div class="card card-span-2">
      <span class="card-title">Active Cluster Telemetry</span>
      <div class="card-value">128 Edge Regions Online</div>
      <span class="card-delta">Consensus quorum 100% stable across all availability zones</span>
    </div>
    <div class="card">
      <span class="card-title">Network Throughput</span>
      <div class="card-value">94.8 Gbps</div>
      <span class="card-delta">+14.2 Gbps peak</span>
    </div>
    <div class="card">
      <span class="card-title">Memory Saturation</span>
      <div class="card-value">42.1%</div>
      <span class="card-delta">Optimal threshold</span>
    </div>
    <div class="card">
      <span class="card-title">Request Rate</span>
      <div class="card-value">1.84M req/s</div>
      <span class="card-delta">+8.4% throughput</span>
    </div>
    <div class="card">
      <span class="card-title">P99 Response Time</span>
      <div class="card-value">2.18 ms</div>
      <span class="card-delta">-0.4ms latency</span>
    </div>
  </div>
</body>
</html>"#.to_string(),

        ParityLevel::Level7TelemetryConsole => r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { width: 100vw; height: 100vh; background: #09090b; color: #ffffff; font-family: sans-serif; overflow: hidden; display: flex; }
  .sidebar { width: 256px; height: 100vh; border-right: 1px solid #27272a; padding: 24px; display: flex; flex-direction: column; justify-content: space-between; flex-shrink: 0; }
  .brand-title { font-size: 16px; line-height: 24px; font-weight: bold; color: #ffffff; }
  .brand-subtitle { font-size: 12px; line-height: 16px; color: #71717a; margin-top: 4px; }
  .nav-list { display: flex; flex-direction: column; gap: 8px; margin-top: 32px; }
  .nav-item { padding: 8px 12px; border-radius: 8px; font-size: 14px; line-height: 20px; font-weight: 500; color: #a1a1aa; }
  .nav-item-active { background: #27272a; color: #ffffff; font-weight: 600; }
  .sidebar-footer { padding: 12px; background: #18181b; border: 1px solid #27272a; border-radius: 8px; font-size: 12px; line-height: 16px; color: #a1a1aa; }
  .footer-status { color: #4ade80; font-weight: 600; margin-top: 4px; font-size: 12px; line-height: 16px; }

  .main { flex: 1; height: 100vh; padding: 32px; display: flex; flex-direction: column; overflow: hidden; }
  .header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 16px; border-bottom: 1px solid #27272a; margin-bottom: 24px; }
  .title { font-size: 24px; line-height: 32px; font-weight: bold; color: #ffffff; }
  .badge { padding: 4px 12px; background: #052e16; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; border-radius: 9999px; border: 1px solid #166534; }

  .metrics-grid { display: flex; gap: 24px; margin-bottom: 24px; }
  .metric-card { flex: 1; padding: 20px; background: #18181b; border: 1px solid #27272a; border-radius: 12px; display: flex; flex-direction: column; }
  .metric-label { font-size: 14px; line-height: 20px; font-weight: 500; color: #a1a1aa; }
  .metric-value { font-size: 30px; line-height: 36px; font-weight: bold; color: #ffffff; margin-top: 8px; }
  .metric-sub { font-size: 12px; line-height: 16px; font-weight: 600; color: #4ade80; margin-top: 4px; }

  .table-card { background: #18181b; border: 1px solid #27272a; border-radius: 12px; display: flex; flex-direction: column; }
  .table-header { padding: 16px 20px; border-bottom: 1px solid #27272a; font-size: 14px; line-height: 20px; font-weight: 600; color: #ffffff; }
  .table-row { display: flex; justify-content: space-between; align-items: center; padding: 12px 20px; border-bottom: 1px solid #27272a; }
  .table-row-last { border-bottom: none; }
  .row-node { font-size: 14px; line-height: 20px; font-weight: 500; color: #ffffff; }
  .row-region { font-size: 12px; line-height: 16px; color: #71717a; }
  .row-rate { font-size: 12px; line-height: 16px; font-weight: 600; color: #a1a1aa; }
  .row-badge { padding: 2px 8px; background: #052e16; color: #4ade80; font-size: 12px; line-height: 16px; font-weight: 600; border-radius: 9999px; border: 1px solid #166534; }
</style>
</head>
<body>
  <aside class="sidebar">
    <div>
      <div class="brand-title">STRATA CLOUD</div>
      <div class="brand-subtitle">Telemetry & Edge Gateway</div>
      <nav class="nav-list">
        <div class="nav-item nav-item-active">Cluster Overview</div>
        <div class="nav-item">Node Instances</div>
        <div class="nav-item">Edge Routing</div>
        <div class="nav-item">Security Policies</div>
      </nav>
    </div>
    <div class="sidebar-footer">
      <div>Cluster SLA Status</div>
      <div class="footer-status">99.99% Uptime Verified</div>
    </div>
  </aside>
  <main class="main">
    <header class="header">
      <h1 class="title">Cloud Infrastructure Health</h1>
      <span class="badge">All Systems Operational</span>
    </header>
    <div class="metrics-grid">
      <div class="metric-card">
        <span class="metric-label">Active Compute Nodes</span>
        <div class="metric-value">1,024</div>
        <span class="metric-sub">+12% from last hour</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">Average Latency</span>
        <div class="metric-value">1.42 ms</div>
        <span class="metric-sub">-0.2ms improvement</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">Bandwidth Ingress</span>
        <div class="metric-value">48.2 Gbps</div>
        <span class="metric-sub">+5.4 Gbps burst</span>
      </div>
    </div>
    <div class="table-card">
      <div class="table-header">Active Ingress Gateways</div>
      <div class="table-row">
        <span class="row-node">gw-us-east-1a</span>
        <span class="row-region">US East (N. Virginia)</span>
        <span class="row-rate">840k req/s</span>
        <span class="row-badge">Healthy</span>
      </div>
      <div class="table-row">
        <span class="row-node">gw-eu-central-1b</span>
        <span class="row-region">EU Central (Frankfurt)</span>
        <span class="row-rate">620k req/s</span>
        <span class="row-badge">Healthy</span>
      </div>
      <div class="table-row">
        <span class="row-node">gw-ap-northeast-1a</span>
        <span class="row-region">Asia Pacific (Tokyo)</span>
        <span class="row-rate">410k req/s</span>
        <span class="row-badge">Healthy</span>
      </div>
      <div class="table-row table-row-last">
        <span class="row-node">gw-sa-east-1c</span>
        <span class="row-region">South America (São Paulo)</span>
        <span class="row-rate">190k req/s</span>
        <span class="row-badge">Healthy</span>
      </div>
    </div>
  </main>
</body>
</html>"#.to_string(),
    }
}

/// Returns the Strata RSX component matching the specified parity level.
pub fn get_fixture_view(level: ParityLevel) -> View {
    match level {
        ParityLevel::Level1BoxModel => {
            rsx! {
                <div class="w-full h-full bg-zinc-950 p-12">
                    <div class="w-[480px] h-[240px] bg-zinc-900 border-2 border-zinc-800"></div>
                </div>
            }
        }
        ParityLevel::Level2RoundedCorner => {
            rsx! {
                <div class="w-full h-full bg-zinc-950 p-12">
                    <div class="w-[480px] h-[240px] bg-zinc-900 border-2 border-zinc-800 rounded-2xl"></div>
                </div>
            }
        }
        ParityLevel::Level3Typography => {
            rsx! {
                <div class="w-full h-full bg-zinc-950 p-12 text-white">
                    <h1 class="text-3xl font-bold text-white">{"Cloud Infrastructure Health"}</h1>
                    <p class="text-sm font-medium text-zinc-400 mt-2">{"Real-time compute cluster telemetry and edge routing diagnostics"}</p>
                </div>
            }
        }
        ParityLevel::Level4FlexRow => {
            rsx! {
                <div class="w-full h-full bg-zinc-950 p-8 text-white">
                    <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
                        <h1 class="text-2xl font-bold text-white">{"Cloud Infrastructure Health"}</h1>
                        <span class="px-3 py-1 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                            {"All Systems Operational"}
                        </span>
                    </header>
                </div>
            }
        }
        ParityLevel::Level5FullDashboard => {
            rsx! {
                <div class="flex flex-col w-full h-full p-8 bg-zinc-950 text-white">
                    <header class="flex justify-between items-center pb-4 mb-8 border-b border-zinc-800">
                        <h1 class="text-2xl font-bold text-white">{"Cloud Infrastructure Health"}</h1>
                        <span class="px-3 py-1 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                            {"All Systems Operational"}
                        </span>
                    </header>
                    <div class="flex gap-6">
                        <div class="flex-1 p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Active Compute Nodes"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"1,024"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"+12% from last hour"}</span>
                        </div>
                        <div class="flex-1 p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Average Latency"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"1.42 ms"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"-0.2ms improvement"}</span>
                        </div>
                        <div class="flex-1 p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Bandwidth Ingress"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"48.2 Gbps"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"+5.4 Gbps burst"}</span>
                        </div>
                    </div>
                </div>
            }
        }
        ParityLevel::Level6CssGrid2D => {
            rsx! {
                <div class="flex flex-col w-full h-full p-8 bg-zinc-950 text-white">
                    <header class="flex justify-between items-center pb-4 mb-8 border-b border-zinc-800">
                        <h1 class="text-2xl font-bold text-white">{"Cloud Infrastructure Health"}</h1>
                        <span class="px-3 py-1 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                            {"All Systems Operational"}
                        </span>
                    </header>
                    <div class="grid grid-cols-3 gap-6">
                        <div class="col-span-2 p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Active Cluster Telemetry"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"128 Edge Regions Online"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"Consensus quorum 100% stable across all availability zones"}</span>
                        </div>
                        <div class="p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Network Throughput"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"94.8 Gbps"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"+14.2 Gbps peak"}</span>
                        </div>
                        <div class="p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Memory Saturation"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"42.1%"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"Optimal threshold"}</span>
                        </div>
                        <div class="p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"Request Rate"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"1.84M req/s"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"+8.4% throughput"}</span>
                        </div>
                        <div class="p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <span class="text-sm font-medium text-zinc-400">{"P99 Response Time"}</span>
                            <div class="text-3xl font-bold text-white mt-2">{"2.18 ms"}</div>
                            <span class="text-xs font-semibold text-emerald-400 mt-1">{"-0.4ms latency"}</span>
                        </div>
                    </div>
                </div>
            }
        }
        ParityLevel::Level7TelemetryConsole => {
            rsx! {
                <div class="flex flex-row w-full h-full bg-zinc-950 text-white">
                    <aside class="w-64 h-full border-r border-zinc-800 p-6 flex flex-col justify-between shrink-0">
                        <div class="flex flex-col">
                            <div class="text-base font-bold text-white">{"STRATA CLOUD"}</div>
                            <div class="text-xs text-zinc-500 mt-1">{"Telemetry & Edge Gateway"}</div>
                            <nav class="flex flex-col gap-2 mt-8">
                                <div class="px-3 py-2 rounded-lg text-sm font-semibold bg-zinc-800 text-white">
                                    {"Cluster Overview"}
                                </div>
                                <div class="px-3 py-2 rounded-lg text-sm font-medium text-zinc-400">
                                    {"Node Instances"}
                                </div>
                                <div class="px-3 py-2 rounded-lg text-sm font-medium text-zinc-400">
                                    {"Edge Routing"}
                                </div>
                                <div class="px-3 py-2 rounded-lg text-sm font-medium text-zinc-400">
                                    {"Security Policies"}
                                </div>
                            </nav>
                        </div>
                        <div class="p-3 bg-zinc-900 border border-zinc-800 rounded-lg text-xs text-zinc-400">
                            <div>{"Cluster SLA Status"}</div>
                            <div class="text-xs text-emerald-400 font-semibold mt-1">{"99.99% Uptime Verified"}</div>
                        </div>
                    </aside>
                    <main class="flex-1 h-full p-8 flex flex-col">
                        <header class="flex justify-between items-center pb-4 mb-6 border-b border-zinc-800">
                            <h1 class="text-2xl font-bold text-white">{"Cloud Infrastructure Health"}</h1>
                            <span class="px-3 py-1 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                                {"All Systems Operational"}
                            </span>
                        </header>
                        <div class="flex gap-6 mb-6">
                            <div class="flex-1 p-5 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                                <span class="text-sm font-medium text-zinc-400">{"Active Compute Nodes"}</span>
                                <div class="text-3xl font-bold text-white mt-2">{"1,024"}</div>
                                <span class="text-xs font-semibold text-emerald-400 mt-1">{"+12% from last hour"}</span>
                            </div>
                            <div class="flex-1 p-5 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                                <span class="text-sm font-medium text-zinc-400">{"Average Latency"}</span>
                                <div class="text-3xl font-bold text-white mt-2">{"1.42 ms"}</div>
                                <span class="text-xs font-semibold text-emerald-400 mt-1">{"-0.2ms improvement"}</span>
                            </div>
                            <div class="flex-1 p-5 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                                <span class="text-sm font-medium text-zinc-400">{"Bandwidth Ingress"}</span>
                                <div class="text-3xl font-bold text-white mt-2">{"48.2 Gbps"}</div>
                                <span class="text-xs font-semibold text-emerald-400 mt-1">{"+5.4 Gbps burst"}</span>
                            </div>
                        </div>
                        <div class="bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col">
                            <div class="px-5 py-4 border-b border-zinc-800 text-sm font-semibold text-white">
                                {"Active Ingress Gateways"}
                            </div>
                            <div class="flex justify-between items-center px-5 py-3 border-b border-zinc-800">
                                <span class="text-sm font-medium text-white">{"gw-us-east-1a"}</span>
                                <span class="text-xs text-zinc-500">{"US East (N. Virginia)"}</span>
                                <span class="text-xs font-semibold text-zinc-400">{"840k req/s"}</span>
                                <span class="px-2 py-0.5 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                                    {"Healthy"}
                                </span>
                            </div>
                            <div class="flex justify-between items-center px-5 py-3 border-b border-zinc-800">
                                <span class="text-sm font-medium text-white">{"gw-eu-central-1b"}</span>
                                <span class="text-xs text-zinc-500">{"EU Central (Frankfurt)"}</span>
                                <span class="text-xs font-semibold text-zinc-400">{"620k req/s"}</span>
                                <span class="px-2 py-0.5 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                                    {"Healthy"}
                                </span>
                            </div>
                            <div class="flex justify-between items-center px-5 py-3 border-b border-zinc-800">
                                <span class="text-sm font-medium text-white">{"gw-ap-northeast-1a"}</span>
                                <span class="text-xs text-zinc-500">{"Asia Pacific (Tokyo)"}</span>
                                <span class="text-xs font-semibold text-zinc-400">{"410k req/s"}</span>
                                <span class="px-2 py-0.5 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                                    {"Healthy"}
                                </span>
                            </div>
                            <div class="flex justify-between items-center px-5 py-3">
                                <span class="text-sm font-medium text-white">{"gw-sa-east-1c"}</span>
                                <span class="text-xs text-zinc-500">{"South America (São Paulo)"}</span>
                                <span class="text-xs font-semibold text-zinc-400">{"190k req/s"}</span>
                                <span class="px-2 py-0.5 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">
                                    {"Healthy"}
                                </span>
                            </div>
                        </div>
                    </main>
                </div>
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level6_layout_geometry_parity() {
        let view = get_fixture_view(ParityLevel::Level6CssGrid2D);
        let mut root = strata::AppRuntime::build_layout_tree(&view.root);
        root.compute_layout(1920.0, 993.0);

        assert_eq!(root.geometry.border_box.width, 1920.0);
        let grid = &root.children[1];
        assert_eq!(grid.geometry.content_box.width, 1856.0);

        // Card 1: col-span-2
        let card1 = &grid.children[0];
        let card2 = &grid.children[1];
        assert_eq!(card1.geometry.border_box.x, 32.0);
        assert!((card1.geometry.border_box.width - 1229.34).abs() < 0.1);
        assert_eq!(
            card2.geometry.border_box.x,
            32.0 + card1.geometry.border_box.width + 24.0
        );
        assert!(
            (card2.geometry.border_box.x + card2.geometry.border_box.width - 1888.0).abs() < 0.1
        );

        // Row 2
        let card3 = &grid.children[2];
        let card5 = &grid.children[4];
        assert_eq!(card3.geometry.border_box.x, 32.0);
        assert!(
            (card5.geometry.border_box.x + card5.geometry.border_box.width - 1888.0).abs() < 0.1
        );
    }

    #[test]
    fn test_level7_layout_geometry_parity() {
        let view = get_fixture_view(ParityLevel::Level7TelemetryConsole);
        let mut root = strata::AppRuntime::build_layout_tree(&view.root);
        root.compute_layout(1920.0, 993.0);

        assert_eq!(root.geometry.border_box.width, 1920.0);
        let aside = &root.children[0];
        assert_eq!(aside.geometry.border_box.width, 256.0);

        let main = &root.children[1];
        assert_eq!(main.geometry.border_box.x, 256.0);
        assert_eq!(main.geometry.border_box.width, 1664.0);
        assert_eq!(main.geometry.content_box.width, 1600.0);

        let header = &main.children[0];
        assert_eq!(header.geometry.border_box.width, 1600.0);

        let metrics_grid = &main.children[1];
        assert_eq!(metrics_grid.geometry.border_box.width, 1600.0);
        let card1 = &metrics_grid.children[0];
        let card3 = &metrics_grid.children[2];
        assert!((card1.geometry.border_box.width - 517.33).abs() < 0.1);
        assert!(
            (card3.geometry.border_box.x + card3.geometry.border_box.width - 1888.0).abs() < 0.1
        );

        let table_card = &main.children[2];
        assert_eq!(table_card.geometry.border_box.width, 1600.0);
    }
    #[test]
    fn test_strata_raw_benchmarks() {
        let timing = crate::raw_bench::measure_strata_level(ParityLevel::Level1BoxModel, 5);
        assert!(timing.total_us > 0);
    }
}
