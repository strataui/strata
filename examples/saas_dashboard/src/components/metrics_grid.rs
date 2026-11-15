//! Real-time KPI Metrics Cards for Strata Cloud SaaS Dashboard.

use crate::state::DashboardState;
use strata::prelude::*;

#[component]
pub fn MetricsGrid(state: DashboardState) -> View {
    rsx! {
        <div class="grid grid-cols-4 gap-4">
            <Stat
                label="GPU CLUSTER UTILIZATION"
                value="91.4%"
                trend={Some(4.2)}
            />
            <Stat
                label="INFERENCE THROUGHPUT"
                value="148,200 req/s"
                trend={Some(12.8)}
            />
            <Stat
                label="P99 LATENCY"
                value="16.8ms"
                trend={Some(-1.4)}
            />
            <Stat
                label="ESTIMATED MONTHLY BURN"
                value="$24,980.50"
                trend={None}
            />
        </div>
    }
}
