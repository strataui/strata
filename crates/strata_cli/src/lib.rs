//! Strata CLI: Developer Toolchain, Project Scaffolding, HMR Dev Engine, Multi-Target Builder, and Packager.

pub mod args;
pub mod builder;
pub mod dev_server;
pub mod packager;
pub mod parity;
pub mod prompt;
pub mod scaffold;
pub mod wizard;

pub use args::{BuildTarget, CliArgs, CliCommand, PackageFormat, TemplateKind};
pub use builder::{BuildReport, BuildRunner};
pub use dev_server::{DevServer, DevServerConfig, HmrSession};
pub use packager::{PackageReport, Packager};
pub use parity::{ParityHarness, ParityReport, ParityTestCase};
pub use prompt::{
    ConfirmPrompt, Key, PromptTheme, RawModeGuard, SelectItem, SelectPrompt, Terminal, TextPrompt,
};
pub use scaffold::{ProjectScaffolder, ScaffoldReport};
pub use wizard::{ProjectWizard, WizardConfig};

/// CLI Toolchain prelude.
pub mod prelude {
    pub use super::args::{BuildTarget, CliArgs, CliCommand, PackageFormat, TemplateKind};
    pub use super::builder::{BuildReport, BuildRunner};
    pub use super::dev_server::{DevServer, DevServerConfig, HmrSession};
    pub use super::packager::{PackageReport, Packager};
    pub use super::parity::{ParityHarness, ParityReport, ParityTestCase};
    pub use super::prompt::{
        ConfirmPrompt, Key, PromptTheme, RawModeGuard, SelectItem, SelectPrompt, Terminal,
        TextPrompt,
    };
    pub use super::scaffold::{ProjectScaffolder, ScaffoldReport};
    pub use super::wizard::{ProjectWizard, WizardConfig};
}
