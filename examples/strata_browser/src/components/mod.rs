//! UI component module hierarchy for the Strata Native Browser.

pub mod app;
pub mod devtools;
pub mod google_new_tab;
pub mod status_bar;
pub mod tab_bar;
pub mod toolbar;
pub mod viewport;

pub use app::BrowserApp;
pub use devtools::DevToolsPanel;
pub use google_new_tab::GoogleNewTab;
pub use status_bar::StatusBar;
pub use tab_bar::TabBar;
pub use toolbar::Toolbar;
pub use viewport::Viewport;
