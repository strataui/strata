//! Standard accessibility roles mapped to OS screen-reader primitives.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum A11yRole {
    Unknown,
    Window,
    RootWebArea,
    Button,
    Checkbox,
    Radio,
    RadioButton,
    Slider,
    Heading,
    Dialog,
    Alert,
    AlertDialog,
    TextInput,
    List,
    ListItem,
    Grid,
    Cell,
    Row,
    Image,
    Link,
    Navigation,
    Banner,
    Main,
    Complementary,
    ContentInfo,
    Form,
    Toolbar,
    Tab,
    TabList,
    TabPanel,
    Switch,
    Status,
    ProgressBar,
    Menu,
    MenuItem,
    Tree,
    TreeItem,
}

impl A11yRole {
    /// Maps an HTML/RSX tag name or ARIA role string to an A11yRole.
    pub fn from_tag_or_role(tag: &str, aria_role: Option<&str>) -> Self {
        if let Some(role) = aria_role {
            match role.to_ascii_lowercase().as_str() {
                "button" => return A11yRole::Button,
                "checkbox" => return A11yRole::Checkbox,
                "radio" => return A11yRole::Radio,
                "slider" => return A11yRole::Slider,
                "heading" => return A11yRole::Heading,
                "dialog" => return A11yRole::Dialog,
                "alert" => return A11yRole::Alert,
                "alertdialog" => return A11yRole::AlertDialog,
                "textbox" => return A11yRole::TextInput,
                "list" => return A11yRole::List,
                "listitem" => return A11yRole::ListItem,
                "grid" => return A11yRole::Grid,
                "cell" => return A11yRole::Cell,
                "row" => return A11yRole::Row,
                "img" => return A11yRole::Image,
                "link" => return A11yRole::Link,
                "navigation" => return A11yRole::Navigation,
                "banner" => return A11yRole::Banner,
                "main" => return A11yRole::Main,
                "complementary" => return A11yRole::Complementary,
                "contentinfo" => return A11yRole::ContentInfo,
                "form" => return A11yRole::Form,
                "toolbar" => return A11yRole::Toolbar,
                "tab" => return A11yRole::Tab,
                "tablist" => return A11yRole::TabList,
                "tabpanel" => return A11yRole::TabPanel,
                "switch" => return A11yRole::Switch,
                "status" => return A11yRole::Status,
                "progressbar" => return A11yRole::ProgressBar,
                "menu" => return A11yRole::Menu,
                "menuitem" => return A11yRole::MenuItem,
                "tree" => return A11yRole::Tree,
                "treeitem" => return A11yRole::TreeItem,
                _ => {}
            }
        }

        match tag.to_ascii_lowercase().as_str() {
            "button" => A11yRole::Button,
            "input" => A11yRole::TextInput,
            "textarea" => A11yRole::TextInput,
            "a" => A11yRole::Link,
            "img" => A11yRole::Image,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => A11yRole::Heading,
            "ul" | "ol" => A11yRole::List,
            "li" => A11yRole::ListItem,
            "nav" => A11yRole::Navigation,
            "header" => A11yRole::Banner,
            "main" => A11yRole::Main,
            "footer" => A11yRole::ContentInfo,
            "aside" => A11yRole::Complementary,
            "form" => A11yRole::Form,
            "table" => A11yRole::Grid,
            "tr" => A11yRole::Row,
            "td" | "th" => A11yRole::Cell,
            _ => A11yRole::Unknown,
        }
    }

    /// Whether this role is an interactive control that receives keyboard focus or user gestures.
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            A11yRole::Button
                | A11yRole::Checkbox
                | A11yRole::Radio
                | A11yRole::RadioButton
                | A11yRole::Slider
                | A11yRole::TextInput
                | A11yRole::Link
                | A11yRole::Switch
                | A11yRole::MenuItem
                | A11yRole::Tab
        )
    }
}
