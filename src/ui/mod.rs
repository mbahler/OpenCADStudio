/// Single source of truth for UI row height (px).
/// Change this to scale the ribbon, layer manager rows, and property panel rows uniformly.
pub const ROW_H: f32 = 26.0;

pub mod about;
pub mod app_menu;
pub mod color_select;
pub mod command_line;
pub mod icons;
pub mod layers;
pub mod layout_manager;
pub mod modal;
pub mod open_progress;
pub mod overlay;
pub mod page_setup;
pub mod plugin_manager;
pub mod popup;
pub mod properties;
pub mod ribbon;
pub mod side_toolbar;
pub mod shortcuts;
pub mod statusbar;
pub mod statusbar_config;
pub mod statusbar_menu;
pub mod style;
pub mod text_util;
pub mod update_notice;

pub use app_menu::AppMenu;
pub use command_line::CommandLine;
pub use layers::LayerPanel;
pub use properties::PropertiesPanel;
pub use ribbon::Ribbon;
pub use statusbar::StatusBar;
