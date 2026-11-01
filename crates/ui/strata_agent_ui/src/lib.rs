//! Streaming, sandboxed generative UI protocol for LLM and autonomous agents in Project Strata.

pub mod action;
pub mod delta;
pub mod history;
pub mod sandbox;
pub mod schema;
pub mod stream;
pub mod widgets;

pub use action::{AgentActionBus, ToolActionCallback};
pub use delta::AgentPatch;
pub use history::{RevisionSnapshot, TimeTravelHistory};
pub use sandbox::{AgentUiSandbox, SandboxedNode};
pub use schema::{is_attribute_safe, is_tag_allowed, AgentSchemaValidator, ALLOWED_TAGS, PROHIBITED_TAGS};
pub use stream::StreamingUiParser;
pub use widgets::{
    ApprovalStatus, ApprovalWidget, ExecutionStep, FormField, FormFieldType, FormWidget,
    MetricStatus, StepProgressWidget, StepStatus, TelemetryCard,
};

pub mod prelude {
    pub use crate::action::{AgentActionBus, ToolActionCallback};
    pub use crate::delta::AgentPatch;
    pub use crate::history::{RevisionSnapshot, TimeTravelHistory};
    pub use crate::sandbox::{AgentUiSandbox, SandboxedNode};
    pub use crate::schema::{is_attribute_safe, is_tag_allowed, AgentSchemaValidator};
    pub use crate::stream::StreamingUiParser;
    pub use crate::widgets::{
        ApprovalStatus, ApprovalWidget, ExecutionStep, FormField, FormFieldType, FormWidget,
        MetricStatus, StepProgressWidget, StepStatus, TelemetryCard,
    };
}
