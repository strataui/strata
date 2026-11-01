//! Streaming, sandboxed generative UI protocol for LLM and autonomous agents in Project Strata.

pub mod action;
pub mod delta;
pub mod history;
pub mod sandbox;
pub mod schema;

pub use action::{AgentActionBus, ToolActionCallback};
pub use delta::AgentPatch;
pub use history::{RevisionSnapshot, TimeTravelHistory};
pub use sandbox::{AgentUiSandbox, SandboxedNode};
pub use schema::{is_attribute_safe, is_tag_allowed, AgentSchemaValidator, ALLOWED_TAGS, PROHIBITED_TAGS};
