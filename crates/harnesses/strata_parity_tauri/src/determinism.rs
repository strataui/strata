//! Determinism injection shared by the Chromium and Tauri capture paths.
//!
//! Both browser engines receive the same injected style and script. The style
//! disables animations, transitions, caret painting, and smooth scrolling. The
//! script freezes the wall clock, the monotonic clock, and animation frame
//! timestamps, then applies a fixed document scroll offset.

/// Fixed epoch used for `Date` and `performance.now` in injected captures.
pub const FIXED_EPOCH_MS: u64 = 1_704_067_200_000;

const DETERMINISM_STYLE: &str = r#"<style id="strata-parity-determinism">
*, *::before, *::after {
  animation: none !important;
  transition: none !important;
  caret-color: transparent !important;
  scroll-behavior: auto !important;
}
html { scroll-behavior: auto !important; }
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0s !important;
    animation-delay: 0s !important;
    transition-duration: 0s !important;
    transition-delay: 0s !important;
    scroll-behavior: auto !important;
  }
}
</style>"#;

fn determinism_script(scroll_y: f32) -> String {
    format!(
        r#"<script id="strata-parity-freeze">
(function () {{
  var FIXED_EPOCH = {epoch};
  var NativeDate = Date;
  function FixedDate() {{
    if (arguments.length === 0) {{ return new NativeDate(FIXED_EPOCH); }}
    return Reflect.construct(NativeDate, arguments);
  }}
  FixedDate.prototype = NativeDate.prototype;
  FixedDate.now = function () {{ return FIXED_EPOCH; }};
  FixedDate.parse = NativeDate.parse;
  FixedDate.UTC = NativeDate.UTC;
  window.Date = FixedDate;

  try {{
    Object.defineProperty(window.performance, 'now', {{
      configurable: true,
      value: function () {{ return 0; }}
    }});
  }} catch (ignored) {{
    window.performance.now = function () {{ return 0; }};
  }}

  var nativeRaf = window.requestAnimationFrame
    ? window.requestAnimationFrame.bind(window)
    : function (callback) {{ return window.setTimeout(function () {{ callback(0); }}, 0); }};
  window.requestAnimationFrame = function (callback) {{
    return nativeRaf(function () {{ callback(0); }});
  }};

  document.documentElement.style.scrollBehavior = 'auto';
  var targetScroll = {scroll};

  function applyScroll() {{
    window.scrollTo(0, targetScroll);
    if (document.documentElement) {{ document.documentElement.scrollTop = targetScroll; }}
    if (document.body) {{ document.body.scrollTop = targetScroll; }}
    var scrollContainer = document.getElementById('parity-scroll-root');
    if (scrollContainer) {{ scrollContainer.scrollTop = targetScroll; }}
  }}

  window.addEventListener('DOMContentLoaded', applyScroll);
  window.addEventListener('load', applyScroll);
  applyScroll();
  window.setTimeout(applyScroll, 0);
  window.setTimeout(applyScroll, 120);
}})();
</script>"#,
        epoch = FIXED_EPOCH_MS,
        scroll = scroll_y
    )
}

fn insert_before(html: &str, marker: &str, payload: &str) -> String {
    if let Some(position) = html.rfind(marker) {
        let mut out = String::with_capacity(html.len() + payload.len());
        out.push_str(&html[..position]);
        out.push_str(payload);
        out.push_str(&html[position..]);
        out
    } else {
        let mut out = String::with_capacity(html.len() + payload.len());
        out.push_str(payload);
        out.push_str(html);
        out
    }
}

/// Returns the fixture document with the determinism style and script injected.
pub fn injected_html(html: &str, scroll_y: f32) -> String {
    let with_style = insert_before(html, "</head>", DETERMINISM_STYLE);
    let script = determinism_script(scroll_y);
    if with_style.contains("</body>") {
        insert_before(&with_style, "</body>", &script)
    } else {
        let mut out = with_style;
        out.push_str(&script);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_adds_style_script_and_fixed_scroll() {
        let html = "<!DOCTYPE html><html><head></head><body><p>fixture</p></body></html>";
        let injected = injected_html(html, 600.0);
        assert!(injected.contains("strata-parity-determinism"));
        assert!(injected.contains("strata-parity-freeze"));
        assert!(injected.contains("FIXED_EPOCH"));
        assert!(injected.contains("targetScroll = 600"));
        assert!(injected.contains("caret-color: transparent"));
        assert!(injected.contains("prefers-reduced-motion"));
        assert!(
            injected.find("strata-parity-determinism").unwrap() < injected.find("</head>").unwrap()
        );
        assert!(injected.find("strata-parity-freeze").unwrap() < injected.find("</body>").unwrap());
    }

    #[test]
    fn injection_without_head_or_body_prefixes_and_appends() {
        let injected = injected_html("<p>bare</p>", 0.0);
        assert!(injected.starts_with("<style id=\"strata-parity-determinism\">"));
        assert!(injected.ends_with("</script>"));
    }
}
