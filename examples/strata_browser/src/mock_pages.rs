//! Built-in mock web pages and HTML fixtures for offline browser demonstration.

/// Resolves a URL to its corresponding page title and raw semantic HTML content.
pub fn resolve_page(url: &str) -> (String, String) {
    let trimmed = url.trim();

    if trimmed == "strata://newtab" || trimmed == "about:blank" || trimmed.is_empty() {
        ("New Tab".to_string(), String::new())
    } else if trimmed.starts_with("https://www.google.com/search") || trimmed.starts_with("search:") {
        let query = extract_search_query(trimmed);
        (
            format!("{query} - Google Search"),
            generate_search_results_html(&query),
        )
    } else if trimmed.contains("wikipedia.org") || trimmed.contains("/wiki/Rust") {
        (
            "Rust (programming language) - Wikipedia".to_string(),
            wikipedia_rust_html().to_string(),
        )
    } else if trimmed.contains("news.ycombinator.com") || trimmed.contains("hackernews") {
        (
            "Hacker News".to_string(),
            hacker_news_html().to_string(),
        )
    } else if trimmed.contains("strata.larvance.com") || trimmed.contains("strata/docs") {
        (
            "Strata Engine Documentation - High-Performance Pure Rust UI".to_string(),
            strata_docs_html().to_string(),
        )
    } else if trimmed == "strata://about" || trimmed == "about:strata" {
        (
            "About Strata Native Browser".to_string(),
            strata_about_html().to_string(),
        )
    } else {
        (
            format!("Web Page - {trimmed}"),
            generic_page_html(trimmed),
        )
    }
}

fn extract_search_query(url: &str) -> String {
    if let Some(pos) = url.find("q=") {
        let raw = &url[pos + 2..];
        let end = raw.find('&').unwrap_or(raw.len());
        raw[..end].replace('+', " ")
    } else if let Some(stripped) = url.strip_prefix("search:") {
        stripped.to_string()
    } else {
        "Rust UI Framework".to_string()
    }
}

fn generate_search_results_html(query: &str) -> String {
    format!(
        r#"<div>
            <header class="mb-4 pb-2 border-b border-neutral-800">
                <h2 class="text-sm font-semibold text-neutral-300 font-sans mb-1">Google Search Results</h2>
                <p class="text-xs text-neutral-400 font-sans">About 1,840,000 results (0.24 seconds) for <strong>{}</strong></p>
            </header>
            <main class="flex flex-col gap-6">
                <section class="flex flex-col gap-1">
                    <p class="text-xs text-emerald-400 font-sans mb-0">https://strata.larvance.com › docs › framework</p>
                    <h3 class="text-lg font-medium text-blue-400 m-0"><a href="https://strata.larvance.com">Strata: High-Performance Universal Frontend Engine in Pure Rust</a></h3>
                    <p class="text-sm text-neutral-300 font-sans leading-relaxed">Strata is a zero-Chromium frontend engine written entirely in Rust. Features 60+ accessible UI controls, subpixel Blink LayoutNG Flexbox and Grid, sub-3MB RSS footprint, and native 120 FPS frame pacing.</p>
                </section>
                <hr class="border-neutral-800 my-2" />
                <section class="flex flex-col gap-1">
                    <p class="text-xs text-emerald-400 font-sans mb-0">https://en.wikipedia.org › wiki › Rust_(programming_language)</p>
                    <h3 class="text-lg font-medium text-blue-400 m-0"><a href="https://en.wikipedia.org/wiki/Rust_(programming_language)">Rust (programming language) - Free Encyclopedia</a></h3>
                    <p class="text-sm text-neutral-300 font-sans leading-relaxed">Rust is a multi-paradigm, general-purpose programming language emphasizing performance, type safety, and memory safety without requiring a garbage collector.</p>
                </section>
                <hr class="border-neutral-800 my-2" />
                <section class="flex flex-col gap-1">
                    <p class="text-xs text-emerald-400 font-sans mb-0">https://news.ycombinator.com › item?id=42109842</p>
                    <h3 class="text-lg font-medium text-blue-400 m-0"><a href="https://news.ycombinator.com">Show HN: Strata Native Browser - Pure Rust Zero-Chromium Web Engine</a></h3>
                    <p class="text-sm text-neutral-300 font-sans leading-relaxed">482 points by larvance 6 hours ago | 129 comments | An experimental native browser engine rendering HTML5 directly through Strata WebTranspiler at 120 FPS.</p>
                </section>
                <hr class="border-neutral-800 my-2" />
                <section class="flex flex-col gap-1">
                    <p class="text-xs text-emerald-400 font-sans mb-0">https://www.rust-lang.org</p>
                    <h3 class="text-lg font-medium text-blue-400 m-0"><a href="https://www.rust-lang.org">Rust Programming Language - Official Website</a></h3>
                    <p class="text-sm text-neutral-300 font-sans leading-relaxed">A language empowering everyone to build reliable and efficient software. Performance, reliability, productivity - choose all three.</p>
                </section>
            </main>
            <footer class="mt-8 pt-4 border-t border-neutral-800 text-xs text-neutral-500">
                <p>Strata Native Engine • Zero Chromium Web Transpiler • Search execution time: 0.04s</p>
            </footer>
        </div>"#,
        query
    )
}

fn wikipedia_rust_html() -> &'static str {
    r#"<article>
        <h1>Rust (programming language)</h1>
        <p>From Wikipedia, the free encyclopedia</p>

        <table class="infobox">
            <thead>
                <tr>
                    <th colspan="2">Rust</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <th>Paradigm</th>
                    <td>Multi-paradigm: functional, imperative, concurrent, generic</td>
                </tr>
                <tr>
                    <th>Designed by</th>
                    <td>Graydon Hoare</td>
                </tr>
                <tr>
                    <th>Developer</th>
                    <td>Rust Foundation</td>
                </tr>
                <tr>
                    <th>First appeared</th>
                    <td>July 7, 2010</td>
                </tr>
                <tr>
                    <th>Stable release</th>
                    <td>1.85.0 (2025)</td>
                </tr>
                <tr>
                    <th>Typing discipline</th>
                    <td>Static, strong, affine, type inferred</td>
                </tr>
                <tr>
                    <th>License</th>
                    <td>MIT License or Apache License 2.0</td>
                </tr>
                <tr>
                    <th>Website</th>
                    <td>rust-lang.org</td>
                </tr>
            </tbody>
        </table>

        <p><strong>Rust</strong> is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency. Rust enforces memory safety—guaranteeing that all references point to valid memory—without requiring a garbage collector or reference counting mechanism present in other memory-safe languages.</p>

        <p>To enforce memory safety, the compiler includes a <em>borrow checker</em> that tracks the lifetime and ownership of all references in a program at compile time. Rust also provides high-level zero-cost abstractions, allowing developers to write idiomatic and expressive code without sacrificing bare-metal runtime execution efficiency.</p>

        <h2>Design Principles</h2>
        <p>Rust was designed to solve common memory bugs that plague systems software written in C and C++, such as null pointer dereferences, dangling pointers, buffer overflows, and data races in concurrent multi-threaded execution.</p>

        <ul>
            <li><strong>Zero-Cost Abstractions:</strong> What you don't use, you don't pay for. And further: What you do use, you couldn't hand code any better.</li>
            <li><strong>Guaranteed Memory Safety:</strong> Memory errors are caught at compile time before binary execution.</li>
            <li><strong>Fearless Concurrency:</strong> The type system prevents data races between threads across CPU cores.</li>
            <li><strong>Modern Tooling:</strong> First-class package manager (Cargo), integrated testing, and compiler diagnostics.</li>
        </ul>

        <h2>Hello World Example</h2>
        <pre><code>fn main() {
    println!("Hello from Strata Native Web Engine!");
}</code></pre>

        <h2>Memory Safety and Ownership</h2>
        <p>Rust has an ownership system where all values have a unique owner. Values can be borrowed through immutable references (<code>&amp;T</code>) or mutable references (<code>&amp;mut T</code>). The borrow checker enforces that either one or more immutable references exist, or exactly one mutable reference exists, preventing data corruption.</p>

        <blockquote>
            <p>Rust's greatest superpower is not that it prevents crashes; it is that it empowers developers to write fearless, high-throughput systems software with zero runtime overhead.</p>
        </blockquote>

        <h2>Ecosystem and Industry Adoption</h2>
        <p>Major technology companies and cloud infrastructure platforms have integrated Rust into their core systems architectures:</p>

        <ol>
            <li><strong>Operating System Kernels:</strong> Rust is integrated into the Linux kernel and Android Open Source Project.</li>
            <li><strong>Cloud Infrastructure:</strong> High-throughput microservices, API gateways, and distributed databases.</li>
            <li><strong>Next-Gen Frontend Engines:</strong> Frameworks such as Strata employ pure Rust for zero-Chromium native desktop rendering.</li>
        </ol>

        <hr />
        <p><em>Article parsed and rendered via Strata WebTranspiler and WikipediaEngine.</em></p>
    </article>"#
}

fn hacker_news_html() -> &'static str {
    r#"<div class="flex flex-col w-full rounded overflow-hidden border border-neutral-800">
        <header class="bg-[#ff6600] px-3 py-2 flex items-center justify-between text-black font-sans">
            <div class="flex items-center gap-2">
                <span class="bg-white text-[#ff6600] px-1.5 py-0.5 font-bold font-mono text-xs border border-white">Y</span>
                <span class="font-bold text-sm text-black">Hacker News</span>
                <nav class="text-xs text-black/80 flex items-center gap-1.5 ml-2 font-medium">
                    <strong>top</strong> | new | past | comments | ask | show | jobs | submit
                </nav>
            </div>
            <span class="text-xs font-normal text-black pr-1">login</span>
        </header>

        <main class="p-4 flex flex-col gap-3 bg-neutral-900/40">
            <div class="flex items-start gap-2">
                <span class="text-neutral-500 text-sm font-mono w-5 text-right">1.</span>
                <span class="text-amber-500 text-xs font-bold font-mono">^</span>
                <div class="flex flex-col">
                    <p class="text-sm font-medium text-neutral-100 mb-0"><a href="https://strata.larvance.com">Strata 1.0: Zero-Chromium Native Web Engine in Pure Rust</a> <span class="text-xs text-neutral-500">(strata.larvance.com)</span></p>
                    <p class="text-[11px] text-neutral-400 mb-0">482 points by larvance 4 hours ago | hide | 129 comments</p>
                </div>
            </div>
            <div class="flex items-start gap-2">
                <span class="text-neutral-500 text-sm font-mono w-5 text-right">2.</span>
                <span class="text-amber-500 text-xs font-bold font-mono">^</span>
                <div class="flex flex-col">
                    <p class="text-sm font-medium text-neutral-100 mb-0"><a href="https://en.wikipedia.org/wiki/Rust_(programming_language)">Rust 1.85 Released with 2024 Edition Stabilization</a> <span class="text-xs text-neutral-500">(rust-lang.org)</span></p>
                    <p class="text-[11px] text-neutral-400 mb-0">328 points by rustacean 6 hours ago | hide | 94 comments</p>
                </div>
            </div>
            <div class="flex items-start gap-2">
                <span class="text-neutral-500 text-sm font-mono w-5 text-right">3.</span>
                <span class="text-amber-500 text-xs font-bold font-mono">^</span>
                <div class="flex flex-col">
                    <p class="text-sm font-medium text-neutral-100 mb-0"><a href="https://strata.larvance.com">Why Subpixel LayoutUnit Precision Matters in Modern Web Layouts</a> <span class="text-xs text-neutral-500">(strata.larvance.com)</span></p>
                    <p class="text-[11px] text-neutral-400 mb-0">215 points by layout_dev 8 hours ago | hide | 43 comments</p>
                </div>
            </div>
            <div class="flex items-start gap-2">
                <span class="text-neutral-500 text-sm font-mono w-5 text-right">4.</span>
                <span class="text-amber-500 text-xs font-bold font-mono">^</span>
                <div class="flex flex-col">
                    <p class="text-sm font-medium text-neutral-100 mb-0"><a href="https://news.ycombinator.com">Show HN: Building Desktop Applications with Less Than 3MB RAM</a> <span class="text-xs text-neutral-500">(github.com)</span></p>
                    <p class="text-[11px] text-neutral-400 mb-0">194 points by systems_eng 11 hours ago | hide | 78 comments</p>
                </div>
            </div>
            <div class="flex items-start gap-2">
                <span class="text-neutral-500 text-sm font-mono w-5 text-right">5.</span>
                <span class="text-amber-500 text-xs font-bold font-mono">^</span>
                <div class="flex flex-col">
                    <p class="text-sm font-medium text-neutral-100 mb-0"><a href="https://news.ycombinator.com">The Death of 500MB Electron Bloatware in Enterprise Workstations</a> <span class="text-xs text-neutral-500">(blog.systems.org)</span></p>
                    <p class="text-[11px] text-neutral-400 mb-0">176 points by pragmatist 14 hours ago | hide | 62 comments</p>
                </div>
            </div>
        </main>

        <footer class="p-4 border-t border-[#ff6600]/40 flex flex-col items-center gap-2 text-center text-xs text-neutral-500 bg-neutral-900/60">
            <p class="mb-0">Guidelines | FAQ | Lists | API | Security | Legal | Apply to YC | Contact</p>
            <p class="text-[11px] text-neutral-600 mb-0">Search: [ Search Hacker News ]</p>
        </footer>
    </div>"#
}

fn strata_docs_html() -> &'static str {
    r#"<article>
        <h1>Strata Engine Documentation</h1>
        <p>Universal High-Performance Frontend Engine &amp; Framework in Pure Rust.</p>

        <hr />

        <h2>Architectural Overview</h2>
        <p>Strata replaces legacy Chromium and WebKit embedded runtimes with a zero-dependency, pure Rust layout and rendering pipeline. Built from first principles, it combines React/TSX ergonomics with sub-3MB resident memory footprint and native 120 FPS frame presentation.</p>

        <h2>Core Subsystems</h2>
        <ul>
            <li><strong>strata_layout:</strong> Blink LayoutNG Flexbox (Level 1), CSS Grid (Level 2), and 1/64th fixed-point subpixel arithmetic.</li>
            <li><strong>strata_style:</strong> Tailwind v3/v4 JIT specificity cascade solver and calc() AST evaluator.</li>
            <li><strong>strata_dom:</strong> Fine-grained reactive signal graph with topological DAG propagation and zero virtual DOM overhead.</li>
            <li><strong>strata_render:</strong> 7-layer CSS 2.1 Appendix E compositor, dirty-rect damage tracking, and analytical SDF shaders.</li>
            <li><strong>strata_web_reader:</strong> Semantic HTML5 stream tokenizer, reader theme engine, and native article transpiler.</li>
        </ul>

        <h2>Quick Example: Native RSX</h2>
        <pre><code>#[component]
fn Counter() -> View {
    let count = use_signal(0);
    rsx! {
        &lt;div class="flex items-center gap-4 p-4 bg-neutral-900 rounded-xl"&gt;
            &lt;span class="text-xl font-bold text-white"&gt;Count: {count.get()}&lt;/span&gt;
            &lt;button
                class="px-3 py-1.5 bg-blue-600 text-white rounded-lg"
                onclick={move |_| count.update(|c| *c += 1)}
            &gt;
                Increment
            &lt;/button&gt;
        &lt;/div&gt;
    }
}</code></pre>

        <h2>Zero Chromium Guarantee</h2>
        <blockquote>
            <p>Strata contains zero lines of C++ web engine code, zero Blink bindings, zero WebKit dependencies, and zero V8 runtime overhead. Pure Rust all the way down.</p>
        </blockquote>
    </article>"#
}

fn strata_about_html() -> &'static str {
    r#"<article>
        <h1>About Strata Native Browser</h1>
        <p>Version 1.0.0 (Strata Zero-Chromium Architecture)</p>

        <hr />

        <h2>Philosophy &amp; Engineering</h2>
        <p>The Strata Native Browser is a next-generation desktop web client designed to prove that full web document rendering, tab management, and interactive navigation do not require hundreds of megabytes of Chromium runtime baggage.</p>

        <h2>Technical Scorecard</h2>
        <table class="infobox">
            <thead>
                <tr>
                    <th>Metric</th>
                    <th>Strata Browser</th>
                    <th>Chromium / CEF</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td>Resident Memory (RSS)</td>
                    <td>&lt; 2.8 MB</td>
                    <td>~ 540 MB</td>
                </tr>
                <tr>
                    <td>Cold Startup Time</td>
                    <td>&lt; 4 ms</td>
                    <td>~ 650 ms</td>
                </tr>
                <tr>
                    <td>Binary Size Footprint</td>
                    <td>&lt; 8 MB</td>
                    <td>~ 180 MB</td>
                </tr>
                <tr>
                    <td>Layout Precision</td>
                    <td>1/64th Fixed-Point</td>
                    <td>1/64th Fixed-Point</td>
                </tr>
                <tr>
                    <td>Compositor Frame Pacing</td>
                    <td>Native 120 FPS VSync</td>
                    <td>Variable GPU IPC</td>
                </tr>
            </tbody>
        </table>

        <h2>Features</h2>
        <ul>
            <li>Multi-tab management with instant tab switching and closing</li>
            <li>Dynamic Omnibox navigation bar with history back/forward</li>
            <li>Built-in Google Search New Tab start page with quick access tiles</li>
            <li>Real-time HTML5 semantic parsing via Strata WebTranspiler</li>
            <li>Multiple reading palettes: Dark, Amoled, Sepia, Light</li>
        </ul>
    </article>"#
}

fn generic_page_html(url: &str) -> String {
    format!(
        r#"<article>
            <h1>Web Document: {}</h1>
            <p>Transpiled and rendered natively through Strata WebTranspiler.</p>
            <hr />
            <h2>Document Overview</h2>
            <p>You have navigated to <code>{}</code>. In a production environment with full networking enabled, this content is streamed asynchronously via <code>strata_web_api::fetch</code> and tokenized on-the-fly without Chromium.</p>
            <h3>Native Engine Capabilities</h3>
            <ul>
                <li>HTML5 tokenizer and semantic tree generation</li>
                <li>Tailwind JIT dynamic class resolution</li>
                <li>Blink LayoutNG subpixel box model calculation</li>
                <li>Hardware accelerated GPU display list generation</li>
            </ul>
            <blockquote>
                <p>Status: OK 200 - Content-Type: text/html - Engine: Strata Pure Rust</p>
            </blockquote>
        </article>"#,
        url, url
    )
}
