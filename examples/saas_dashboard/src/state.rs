//! Shared state for the Strata SaaS Dashboard example.

use strata::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Deployment {
    pub id: &'static str,
    pub name: &'static str,
    pub model: &'static str,
    pub region: &'static str,
    pub status: &'static str,
    pub replicas: usize,
    pub latency_p99: f32,
}

#[derive(Clone)]
pub struct DashboardState {
    pub search_query: Signal<String>,
    pub status_filter: Signal<String>,
    pub is_deploy_modal_open: Signal<bool>,
    pub deployments: Signal<Vec<Deployment>>,
    pub gpu_utilization: Signal<f32>,
    pub requests_per_sec: Signal<usize>,
    pub p99_latency: Signal<f32>,
    pub monthly_burn: Signal<f32>,
}

impl DashboardState {
    pub fn new() -> Self {
        let initial_deployments = vec![
            Deployment {
                id: "dep-01",
                name: "mistral-large-prod-us-east",
                model: "Mistral Large 2 (123B)",
                region: "us-east-1",
                status: "RUNNING",
                replicas: 16,
                latency_p99: 14.2,
            },
            Deployment {
                id: "dep-02",
                name: "llama3-70b-instruct-eu",
                model: "Llama 3.3 70B Instruct",
                region: "eu-central-1",
                status: "RUNNING",
                replicas: 8,
                latency_p99: 18.5,
            },
            Deployment {
                id: "dep-03",
                name: "deepseek-coder-v2-dev",
                model: "DeepSeek Coder V2 (236B)",
                region: "us-west-2",
                status: "PROVISIONING",
                replicas: 4,
                latency_p99: 22.1,
            },
            Deployment {
                id: "dep-04",
                name: "qwen-2.5-coder-batch",
                model: "Qwen 2.5 Coder 32B",
                region: "ap-northeast-1",
                status: "DEGRADED",
                replicas: 2,
                latency_p99: 38.9,
            },
        ];

        Self {
            search_query: use_signal(String::new()),
            status_filter: use_signal(String::from("ALL")),
            is_deploy_modal_open: use_signal(false),
            deployments: use_signal(initial_deployments),
            gpu_utilization: use_signal(91.4),
            requests_per_sec: use_signal(148200),
            p99_latency: use_signal(16.8),
            monthly_burn: use_signal(24_980.5),
        }
    }
}
