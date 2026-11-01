//! Virtual OS Home Launcher springboard and Recents multitasking carousel.

/// Metadata for installed Strata applications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMetadata {
    pub bundle_id: String,
    pub name: String,
    pub icon_name: String,
    pub category: String,
}

impl AppMetadata {
    pub fn new(bundle_id: &str, name: &str, icon_name: &str, category: &str) -> Self {
        Self {
            bundle_id: bundle_id.to_string(),
            name: name.to_string(),
            icon_name: icon_name.to_string(),
            category: category.to_string(),
        }
    }
}

/// Home Screen Springboard state.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeScreenState {
    pub grid_apps: Vec<AppMetadata>,
    pub dock_apps: Vec<AppMetadata>,
    pub search_query: String,
    pub is_app_drawer_open: bool,
}

impl Default for HomeScreenState {
    fn default() -> Self {
        Self::new()
    }
}

impl HomeScreenState {
    pub fn new() -> Self {
        let default_apps = vec![
            AppMetadata::new("com.strata.saas", "Cloud SaaS", "server", "Productivity"),
            AppMetadata::new("com.strata.storefront", "Apex Store", "shopping-cart", "Shopping"),
            AppMetadata::new("com.strata.studio", "Creative Studio", "palette", "Graphics"),
            AppMetadata::new("com.strata.settings", "Settings", "settings", "System"),
            AppMetadata::new("com.strata.camera", "Camera", "camera", "Media"),
            AppMetadata::new("com.strata.browser", "Web Engine", "globe", "Utility"),
        ];

        let dock = vec![
            AppMetadata::new("com.strata.phone", "Phone", "phone", "System"),
            AppMetadata::new("com.strata.messages", "Messages", "message-square", "System"),
            AppMetadata::new("com.strata.storefront", "Apex Store", "shopping-cart", "Shopping"),
            AppMetadata::new("com.strata.saas", "Cloud SaaS", "server", "Productivity"),
        ];

        Self {
            grid_apps: default_apps,
            dock_apps: dock,
            search_query: String::new(),
            is_app_drawer_open: false,
        }
    }

    /// Filters apps according to search query.
    pub fn filtered_apps(&self) -> Vec<&AppMetadata> {
        if self.search_query.trim().is_empty() {
            self.grid_apps.iter().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.grid_apps
                .iter()
                .filter(|a| a.name.to_lowercase().contains(&q) || a.bundle_id.to_lowercase().contains(&q))
                .collect()
        }
    }
}

/// 3D Recents Multitasking Carousel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecentsCarouselState {
    pub is_active: bool,
    pub tasks: Vec<AppMetadata>,
    pub focused_index: usize,
}

impl RecentsCarouselState {
    pub fn new() -> Self {
        Self {
            is_active: false,
            tasks: Vec::new(),
            focused_index: 0,
        }
    }

    /// Opens the recents multitasking carousel.
    pub fn open(&mut self) {
        self.is_active = true;
        if self.focused_index >= self.tasks.len() && !self.tasks.is_empty() {
            self.focused_index = self.tasks.len() - 1;
        }
    }

    /// Closes the recents multitasking carousel.
    pub fn close(&mut self) {
        self.is_active = false;
    }

    /// Dismisses (swipes up) an application from recents.
    pub fn dismiss_task(&mut self, index: usize) -> Option<AppMetadata> {
        if index < self.tasks.len() {
            let removed = self.tasks.remove(index);
            if self.focused_index >= self.tasks.len() && !self.tasks.is_empty() {
                self.focused_index = self.tasks.len() - 1;
            }
            Some(removed)
        } else {
            None
        }
    }

    /// Adds an app to the top of recents stack.
    pub fn push_task(&mut self, app: AppMetadata) {
        self.tasks.retain(|t| t.bundle_id != app.bundle_id);
        self.tasks.insert(0, app);
        self.focused_index = 0;
    }
}
