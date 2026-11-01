//! Pre-built, sandboxed conversational agent widgets for generative UI.
//! Includes Action Approvals, Dynamic Forms, Telemetry Cards, and Multi-Step Progress Trackers.

use strata_dom::{NodeBlueprint, Tag};

/// Status of a human-in-the-loop action approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalStatus {
    #[default]
    Pending,
    Approved,
    Rejected,
}

/// Action approval card for sensitive tool calls or autonomous mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalWidget {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action_name: String,
    pub parameters: Vec<(String, String)>,
    pub status: ApprovalStatus,
}

impl ApprovalWidget {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        action_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            action_name: action_name.into(),
            parameters: Vec::new(),
            status: ApprovalStatus::Pending,
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.push((key.into(), value.into()));
        self
    }

    pub fn approve(&mut self) {
        self.status = ApprovalStatus::Approved;
    }

    pub fn reject(&mut self) {
        self.status = ApprovalStatus::Rejected;
    }

    /// Renders a reactive NodeBlueprint representation of the approval card.
    pub fn render_blueprint(&self) -> NodeBlueprint {
        let mut children = Vec::new();

        // Title and badge header
        let badge_class = match self.status {
            ApprovalStatus::Pending => "bg-amber-950/80 text-amber-400 border border-amber-800/60",
            ApprovalStatus::Approved => "bg-emerald-950/80 text-emerald-400 border border-emerald-800/60",
            ApprovalStatus::Rejected => "bg-rose-950/80 text-rose-400 border border-rose-800/60",
        };
        let status_label = match self.status {
            ApprovalStatus::Pending => "Pending Review",
            ApprovalStatus::Approved => "Approved",
            ApprovalStatus::Rejected => "Rejected",
        };

        let header = NodeBlueprint::element(
            Tag::Div,
            vec!["flex items-center justify-between pb-3 border-b border-slate-800".to_string()],
            Vec::new(),
            vec![
                NodeBlueprint::element(
                    Tag::H3,
                    vec!["text-sm font-semibold text-slate-100".to_string()],
                    Vec::new(),
                    vec![NodeBlueprint::text(self.title.clone())],
                ),
                NodeBlueprint::element(
                    Tag::Span,
                    vec![format!("text-xs px-2.5 py-0.5 rounded-full font-mono {}", badge_class)],
                    Vec::new(),
                    vec![NodeBlueprint::text(status_label.to_string())],
                ),
            ],
        );
        children.push(header);

        // Description
        if !self.description.is_empty() {
            children.push(NodeBlueprint::element(
                Tag::P,
                vec!["text-xs text-slate-400 mt-2".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(self.description.clone())],
            ));
        }

        // Action Box
        let mut action_children = vec![
            NodeBlueprint::element(
                Tag::Span,
                vec!["text-xs text-slate-500 font-mono block mb-1".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text("ACTION TO EXECUTE:")],
            ),
            NodeBlueprint::element(
                Tag::Span,
                vec!["text-xs font-mono font-medium text-purple-400 block".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(format!("{}()", self.action_name))],
            ),
        ];

        for (k, v) in &self.parameters {
            action_children.push(NodeBlueprint::element(
                Tag::Div,
                vec!["text-xs font-mono text-slate-300 pl-3 mt-1".to_string()],
                Vec::new(),
                vec![
                    NodeBlueprint::element(
                        Tag::Span,
                        vec!["text-slate-500".to_string()],
                        Vec::new(),
                        vec![NodeBlueprint::text(format!("{}: ", k))],
                    ),
                    NodeBlueprint::element(
                        Tag::Span,
                        vec!["text-emerald-300".to_string()],
                        Vec::new(),
                        vec![NodeBlueprint::text(v.clone())],
                    ),
                ],
            ));
        }

        children.push(NodeBlueprint::element(
            Tag::Div,
            vec!["bg-slate-950/80 rounded-md p-3 my-3 border border-slate-800/80".to_string()],
            Vec::new(),
            action_children,
        ));

        // Action Buttons (only visible when Pending)
        if self.status == ApprovalStatus::Pending {
            let buttons = NodeBlueprint::element(
                Tag::Div,
                vec!["flex gap-2 justify-end pt-2".to_string()],
                Vec::new(),
                vec![
                    NodeBlueprint::element(
                        Tag::Button,
                        vec!["px-3 py-1.5 text-xs font-medium rounded-md bg-rose-950/40 text-rose-300 hover:bg-rose-900/60 border border-rose-800/50".to_string()],
                        Vec::new(),
                        vec![NodeBlueprint::text("Reject")],
                    ),
                    NodeBlueprint::element(
                        Tag::Button,
                        vec!["px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-600 text-white hover:bg-emerald-500 shadow-sm".to_string()],
                        Vec::new(),
                        vec![NodeBlueprint::text("Approve Action")],
                    ),
                ],
            );
            children.push(buttons);
        }

        NodeBlueprint::element(
            Tag::Div,
            vec!["bg-slate-900 border border-slate-800 rounded-lg p-4 shadow-lg max-w-md w-full".to_string()],
            Vec::new(),
            children,
        )
    }
}

/// Dynamic form field types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldType {
    Text,
    Number,
    Select(Vec<String>),
    Checkbox,
}

/// An individual field within a dynamic form widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub name: String,
    pub label: String,
    pub field_type: FormFieldType,
    pub required: bool,
    pub value: String,
}

impl FormField {
    pub fn new(name: impl Into<String>, label: impl Into<String>, field_type: FormFieldType) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            field_type,
            required: false,
            value: String::new(),
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_default(mut self, val: impl Into<String>) -> Self {
        self.value = val.into();
        self
    }
}

/// Dynamic conversational data collection form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormWidget {
    pub id: String,
    pub title: String,
    pub fields: Vec<FormField>,
    pub submitted: bool,
}

impl FormWidget {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            fields: Vec::new(),
            submitted: false,
        }
    }

    pub fn add_field(&mut self, field: FormField) {
        self.fields.push(field);
    }

    pub fn set_value(&mut self, name: &str, value: &str) {
        if let Some(f) = self.fields.iter_mut().find(|f| f.name == name) {
            f.value = value.to_string();
        }
    }

    pub fn render_blueprint(&self) -> NodeBlueprint {
        let mut field_nodes = Vec::new();

        field_nodes.push(NodeBlueprint::element(
            Tag::H3,
            vec!["text-sm font-semibold text-slate-100 mb-3".to_string()],
            Vec::new(),
            vec![NodeBlueprint::text(self.title.clone())],
        ));

        for f in &self.fields {
            let label = NodeBlueprint::element(
                Tag::Label,
                vec!["block text-xs font-medium text-slate-300 mb-1".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(f.label.clone())],
            );

            let input = match &f.field_type {
                FormFieldType::Text | FormFieldType::Number => NodeBlueprint::element(
                    Tag::Input,
                    vec!["w-full bg-slate-950 border border-slate-800 rounded px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500".to_string()],
                    Vec::new(),
                    Vec::new(),
                ),
                FormFieldType::Select(options) => {
                    let opt_nodes = options
                        .iter()
                        .map(|opt| {
                            NodeBlueprint::element(
                                Tag::Option,
                                Vec::new(),
                                Vec::new(),
                                vec![NodeBlueprint::text(opt.clone())],
                            )
                        })
                        .collect();
                    NodeBlueprint::element(
                        Tag::Select,
                        vec!["w-full bg-slate-950 border border-slate-800 rounded px-2.5 py-1.5 text-xs text-slate-200".to_string()],
                        Vec::new(),
                        opt_nodes,
                    )
                }
                FormFieldType::Checkbox => NodeBlueprint::element(
                    Tag::Input,
                    vec!["rounded bg-slate-950 border-slate-800 text-indigo-600".to_string()],
                    Vec::new(),
                    Vec::new(),
                ),
            };

            field_nodes.push(NodeBlueprint::element(
                Tag::Div,
                vec!["mb-3".to_string()],
                Vec::new(),
                vec![label, input],
            ));
        }

        field_nodes.push(NodeBlueprint::element(
            Tag::Button,
            vec!["w-full mt-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium py-2 rounded shadow-sm".to_string()],
            Vec::new(),
            vec![NodeBlueprint::text(if self.submitted { "Submitted" } else { "Submit Form" })],
        ));

        NodeBlueprint::element(
            Tag::Form,
            vec!["bg-slate-900 border border-slate-800 rounded-lg p-4 max-w-sm w-full".to_string()],
            Vec::new(),
            field_nodes,
        )
    }
}

/// Operational status for real-time telemetry metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricStatus {
    #[default]
    Normal,
    Optimal,
    Warning,
    Critical,
}

/// Enterprise telemetry card displaying key metric, units, and trend.
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryCard {
    pub id: String,
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
    pub trend_pct: Option<f32>,
    pub status: MetricStatus,
}

impl TelemetryCard {
    pub fn new(id: impl Into<String>, label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            unit: None,
            trend_pct: None,
            status: MetricStatus::Normal,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_trend(mut self, trend: f32) -> Self {
        self.trend_pct = Some(trend);
        self
    }

    pub fn with_status(mut self, status: MetricStatus) -> Self {
        self.status = status;
        self
    }

    pub fn render_blueprint(&self) -> NodeBlueprint {
        let status_color = match self.status {
            MetricStatus::Optimal => "text-emerald-400",
            MetricStatus::Normal => "text-sky-400",
            MetricStatus::Warning => "text-amber-400",
            MetricStatus::Critical => "text-rose-400",
        };

        let mut value_row = vec![
            NodeBlueprint::element(
                Tag::Span,
                vec![format!("text-2xl font-bold font-mono {}", status_color)],
                Vec::new(),
                vec![NodeBlueprint::text(self.value.clone())],
            ),
        ];

        if let Some(ref u) = self.unit {
            value_row.push(NodeBlueprint::element(
                Tag::Span,
                vec!["text-xs text-slate-500 font-mono pl-1.5".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(u.clone())],
            ));
        }

        let mut rows = vec![
            NodeBlueprint::element(
                Tag::P,
                vec!["text-xs text-slate-400 uppercase tracking-wide font-medium".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(self.label.clone())],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec!["flex items-baseline mt-1".to_string()],
                Vec::new(),
                value_row,
            ),
        ];

        if let Some(trend) = self.trend_pct {
            let (trend_str, trend_color) = if trend >= 0.0 {
                (format!("+{:.1}%", trend), "text-emerald-400")
            } else {
                (format!("{:.1}%", trend), "text-rose-400")
            };

            rows.push(NodeBlueprint::element(
                Tag::P,
                vec![format!("text-xs font-mono mt-2 {}", trend_color)],
                Vec::new(),
                vec![NodeBlueprint::text(format!("{} vs last cycle", trend_str))],
            ));
        }

        NodeBlueprint::element(
            Tag::Div,
            vec!["bg-slate-900 border border-slate-800 rounded-lg p-4 flex flex-col justify-between shadow-sm min-w-[180px]".to_string()],
            Vec::new(),
            rows,
        )
    }
}

/// Execution status of an individual agent plan step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
}

/// An individual step in an autonomous agent execution plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionStep {
    pub step_number: usize,
    pub name: String,
    pub status: StepStatus,
    pub detail: Option<String>,
}

/// Multi-step execution tracker widget with status badges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepProgressWidget {
    pub id: String,
    pub title: String,
    pub steps: Vec<ExecutionStep>,
}

impl StepProgressWidget {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, name: impl Into<String>) -> usize {
        let step_number = self.steps.len() + 1;
        self.steps.push(ExecutionStep {
            step_number,
            name: name.into(),
            status: StepStatus::Pending,
            detail: None,
        });
        step_number
    }

    pub fn set_step_status(&mut self, step_number: usize, status: StepStatus) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.status = status;
        }
    }

    pub fn set_step_detail(&mut self, step_number: usize, detail: impl Into<String>) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.detail = Some(detail.into());
        }
    }

    pub fn render_blueprint(&self) -> NodeBlueprint {
        let mut children = vec![
            NodeBlueprint::element(
                Tag::H3,
                vec!["text-sm font-semibold text-slate-100 mb-3".to_string()],
                Vec::new(),
                vec![NodeBlueprint::text(self.title.clone())],
            ),
        ];

        for step in &self.steps {
            let (indicator_class, text_class) = match step.status {
                StepStatus::Pending => ("bg-slate-800 text-slate-500", "text-slate-500"),
                StepStatus::Running => ("bg-indigo-600 text-white animate-pulse", "text-indigo-300 font-medium"),
                StepStatus::Completed => ("bg-emerald-600 text-white", "text-slate-200"),
                StepStatus::Failed => ("bg-rose-600 text-white", "text-rose-300 font-medium"),
            };

            let row = NodeBlueprint::element(
                Tag::Div,
                vec!["flex items-center gap-3 py-2 border-b border-slate-800/40 last:border-0".to_string()],
                Vec::new(),
                vec![
                    NodeBlueprint::element(
                        Tag::Span,
                        vec![format!("w-6 h-6 rounded-full flex items-center justify-center text-xs font-mono {}", indicator_class)],
                        Vec::new(),
                        vec![NodeBlueprint::text(step.step_number.to_string())],
                    ),
                    NodeBlueprint::element(
                        Tag::Span,
                        vec![format!("text-xs {}", text_class)],
                        Vec::new(),
                        vec![NodeBlueprint::text(step.name.clone())],
                    ),
                ],
            );

            children.push(row);
        }

        NodeBlueprint::element(
            Tag::Div,
            vec!["bg-slate-900 border border-slate-800 rounded-lg p-4 max-w-md w-full shadow-md".to_string()],
            Vec::new(),
            children,
        )
    }
}
