// Strata In-Browser Playground Core Evaluator & Live State Bridge

const TEMPLATES = {
    counter: `rsx! {
    <div class="flex flex-col items-center justify-center h-full gap-6 p-8 bg-[#08090a] text-white">
        <div class="flex items-center gap-2">
            <span class="w-3 h-3 rounded-full bg-[#10b981] animate-ping"></span>
            <span class="text-xs font-mono text-[#a1a1aa]">REACTIVE SIGNAL DEMO</span>
        </div>

        <h1 class="text-3xl font-bold tracking-tight text-white">
            {"Strata Reactive Canvas"}
        </h1>

        <div class="flex items-center gap-4 p-4 rounded-xl bg-[#0f1012] border border-[rgba(255,255,255,0.08)]">
            <button id="decBtn" class="px-4 py-2 rounded bg-[#15171a] hover:bg-[#1c1e22] text-[#f7f8f8] border border-[rgba(255,255,255,0.1)] font-bold transition-colors">
                {"- DECREMENT"}
            </button>
            <span id="counterValue" class="text-2xl font-bold px-6 text-[#38bdf8] font-mono">
                {"42"}
            </span>
            <button id="incBtn" class="px-4 py-2 rounded bg-[#38bdf8]/20 hover:bg-[#38bdf8]/30 text-[#38bdf8] border border-[#38bdf8]/40 font-bold transition-colors">
                {"+ INCREMENT"}
            </button>
        </div>

        <p class="text-xs text-[#71717a] font-mono">
            {"Sub-millisecond updates · Zero V8 garbage collection · Pure Rust"}
        </p>
    </div>
}`,

    saas: `rsx! {
    <div class="flex flex-col h-full bg-[#08090a] text-[#f7f8f8] p-6 gap-6 font-mono">
        <div class="flex items-center justify-between border-b border-[rgba(255,255,255,0.07)] pb-4">
            <div>
                <h2 class="text-lg font-bold text-white">{"AI Infrastructure & Cluster Metrics"}</h2>
                <p class="text-xs text-[#a1a1aa]">{"Live GPU inference latency across global regions"}</p>
            </div>
            <span class="px-2.5 py-1 rounded bg-[#10b981]/20 text-[#10b981] border border-[#10b981]/40 text-xs font-bold">
                {"128 NODES ACTIVE"}
            </span>
        </div>

        <div class="grid grid-cols-3 gap-4">
            <div class="p-4 rounded-lg bg-[#0f1012] border border-[rgba(255,255,255,0.07)] flex flex-col gap-1">
                <span class="text-[10px] text-[#71717a]">{"COLD START"}</span>
                <span class="text-xl font-bold text-[#38bdf8]">{"16 µs"}</span>
                <span class="text-[10px] text-[#10b981]">{"500x faster than Electron"}</span>
            </div>
            <div class="p-4 rounded-lg bg-[#0f1012] border border-[rgba(255,255,255,0.07)] flex flex-col gap-1">
                <span class="text-[10px] text-[#71717a]">{"MEMORY FOOTPRINT"}</span>
                <span class="text-xl font-bold text-[#10b981]">{"2.7 MB"}</span>
                <span class="text-[10px] text-[#a1a1aa]">{"CPU Cache Aligned IR"}</span>
            </div>
            <div class="p-4 rounded-lg bg-[#0f1012] border border-[rgba(255,255,255,0.07)] flex flex-col gap-1">
                <span class="text-[10px] text-[#71717a]">{"PRESENTATION RATE"}</span>
                <span class="text-xl font-bold text-[#f59e0b]">{"120 FPS"}</span>
                <span class="text-[10px] text-[#10b981]">{"Hardware VSync Lock"}</span>
            </div>
        </div>
    </div>
}`,

    horizon: `rsx! {
    <div class="flex flex-col h-full bg-[#08090a] text-[#f7f8f8] p-6 gap-4 font-mono select-none">
        <div class="flex items-center justify-between border-b border-[rgba(255,255,255,0.07)] pb-3">
            <div class="flex items-center gap-3">
                <span class="w-2.5 h-2.5 rounded-sm bg-[#5e6ad2]"></span>
                <span class="font-bold text-sm tracking-wider">{"HORIZON C2 // MINI HUD"}</span>
            </div>
            <span class="px-2 py-0.5 rounded text-[10px] font-bold bg-[#10b981]/20 text-[#10b981] border border-[#10b981]/40">
                {"AUTO_WAYPOINT · ARMED"}
            </span>
        </div>

        <div class="grid grid-cols-2 gap-4 flex-1">
            <div class="p-4 rounded-lg bg-[#0f1012] border border-[rgba(255,255,255,0.07)] flex flex-col justify-between">
                <span class="text-xs text-[#71717a] font-bold">{"6-DOF ATTITUDE DIRECTOR"}</span>
                <div class="flex flex-col items-center justify-center my-4">
                    <span class="text-xs text-[#38bdf8]">{"PITCH: +2.4°  ·  ROLL: -1.2°"}</span>
                    <span class="text-xl font-bold text-white mt-1">{"YAW: 184.5°"}</span>
                </div>
                <div class="text-[10px] text-[#a1a1aa] flex justify-between">
                    <span>{"IAS: 35.8 KT"}</span>
                    <span class="text-[#10b981]">{"ALT: 142.8 M"}</span>
                </div>
            </div>

            <div class="p-4 rounded-lg bg-[#0f1012] border border-[rgba(255,255,255,0.07)] flex flex-col justify-between">
                <span class="text-xs text-[#71717a] font-bold">{"SORTIE WAYPOINT PROGRESS"}</span>
                <div class="flex flex-col gap-2 my-2 text-xs">
                    <div class="flex justify-between text-[#10b981]">
                        <span>{"WP-01 (ALPHA)"}</span>
                        <span>{"√ REACHED"}</span>
                    </div>
                    <div class="flex justify-between text-[#f59e0b] font-bold">
                        <span>{"WP-02 (BRAVO)"}</span>
                        <span>{"› EN ROUTE"}</span>
                    </div>
                    <div class="flex justify-between text-[#71717a]">
                        <span>{"WP-03 (CHARLIE)"}</span>
                        <span>{"PENDING"}</span>
                    </div>
                </div>
                <span class="text-[10px] text-[#38bdf8]">{"BITCASK WAL BLACKBOX SYNCED"}</span>
            </div>
        </div>
    </div>
}`,

    waveform: `rsx! {
    <div class="flex flex-col h-full bg-[#08090a] text-[#f7f8f8] p-4 font-mono select-none">
        <div class="h-8 flex items-center justify-between border-b border-[rgba(255,255,255,0.07)] pb-2 text-xs">
            <span class="text-[#38bdf8] font-bold">{"IEEE MULTI-CHANNEL LOGIC ANALYZER"}</span>
            <span class="text-[#71717a]">{"10.0 ms/div · 100 MSPS"}</span>
        </div>

        <div class="flex-1 flex flex-col divide-y divide-[rgba(255,255,255,0.05)] my-2">
            <div class="py-2 flex items-center gap-4">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-[#f59e0b]/20 text-[#f59e0b]">{"0 D"}</span>
                <span class="text-xs w-28 text-[#f7f8f8]">{"pwm.esc0"}</span>
                <span class="text-[#f59e0b] text-xs font-mono">{"___|▔▔▔|___|▔▔▔|___|▔▔▔|___|▔▔▔|___|▔▔▔|___|▔▔▔|___"}</span>
            </div>
            <div class="py-2 flex items-center gap-4">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-[#10b981]/20 text-[#10b981]">{"1 D"}</span>
                <span class="text-xs w-28 text-[#f7f8f8]">{"pwm.esc1"}</span>
                <span class="text-[#10b981] text-xs font-mono">{"___|▔▔▔▔|___|▔▔▔▔|___|▔▔▔▔|___|▔▔▔▔|___|▔▔▔▔|___|"}</span>
            </div>
            <div class="py-2 flex items-center gap-4">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-[#d946ef]/20 text-[#d946ef]">{"B0"}</span>
                <span class="text-xs w-28 text-[#f7f8f8]">{"can.uavcan"}</span>
                <div class="flex gap-1 text-[10px] text-[#d946ef]">
                    <span class="px-1.5 py-0.5 rounded bg-[#d946ef]/10 border border-[#d946ef]/30">{"< 0x10FF_0140 >"}</span>
                    <span class="px-1.5 py-0.5 rounded bg-[#d946ef]/10 border border-[#d946ef]/30">{"< 0x0024_A312 >"}</span>
                </div>
            </div>
        </div>

        <div class="h-6 flex items-center justify-between text-[10px] text-[#71717a] border-t border-[rgba(255,255,255,0.07)] pt-2">
            <span>{"CURSOR: ΔT = 2.50 ms (400.00 Hz)"}</span>
            <span class="text-[#10b981]">{"DMA FIFO: 0 OVERRUNS"}</span>
        </div>
    </div>
}`
};

let currentCount = 42;

function renderRsxToDom(rsx) {
    const container = document.getElementById('previewContainer');
    container.innerHTML = '';

    // Strip rsx! { ... }
    let raw = rsx.trim();
    if (raw.startsWith('rsx!')) {
        const firstBrace = raw.indexOf('{');
        const lastBrace = raw.lastIndexOf('}');
        if (firstBrace !== -1 && lastBrace !== -1) {
            raw = raw.slice(firstBrace + 1, lastBrace).trim();
        }
    }

    // Replace {"..."} with text
    raw = raw.replace(/\{"([^"]*)"\}/g, '$1');

    container.innerHTML = raw;

    // Attach dynamic reactive interactions
    const incBtn = document.getElementById('incBtn');
    const decBtn = document.getElementById('decBtn');
    const counterVal = document.getElementById('counterValue');

    if (incBtn && counterVal) {
        incBtn.onclick = () => {
            currentCount += 1;
            counterVal.textContent = currentCount;
        };
    }
    if (decBtn && counterVal) {
        decBtn.onclick = () => {
            currentCount -= 1;
            counterVal.textContent = currentCount;
        };
    }
}

document.addEventListener('DOMContentLoaded', () => {
    const editor = document.getElementById('codeEditor');
    const select = document.getElementById('templateSelect');
    const runBtn = document.getElementById('runBtn');
    const exportBtn = document.getElementById('exportBtn');

    // Load initial counter template
    editor.value = TEMPLATES.counter;
    renderRsxToDom(editor.value);

    select.onchange = (e) => {
        const val = e.target.value;
        if (TEMPLATES[val]) {
            editor.value = TEMPLATES[val];
            renderRsxToDom(editor.value);
        }
    };

    runBtn.onclick = () => {
        renderRsxToDom(editor.value);
    };

    editor.onkeydown = (e) => {
        if (e.ctrlKey && e.key === 'Enter') {
            renderRsxToDom(editor.value);
        }
    };

    exportBtn.onclick = () => {
        alert("Strata CLI Command:\\n\\nstrata new my-strata-app --template " + select.value);
    };
});
