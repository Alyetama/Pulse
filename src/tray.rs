//! Menu-bar status item and its right-click context menu.

use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

// Stable menu-item ids used when matching `MenuEvent`s.
pub const ID_OPEN: &str = "pulse.open";
pub const ID_PREFS: &str = "pulse.prefs";
pub const ID_LOGIN: &str = "pulse.login";
pub const ID_QUIT: &str = "pulse.quit";

/// Owns the tray icon and menu. These must stay alive for the whole run, and
/// must be created on the main thread (done from the eframe app creator).
pub struct Tray {
    pub icon: TrayIcon,
    // Kept alive so the menu and its live handles remain valid.
    _menu: Menu,
    login_item: CheckMenuItem,
}

impl Tray {
    pub fn new(launch_at_login: bool) -> Result<Self, String> {
        let open = MenuItem::with_id(ID_OPEN, "Open Pulse", true, None);
        let prefs = MenuItem::with_id(ID_PREFS, "Preferences…", true, None);
        let login = CheckMenuItem::with_id(ID_LOGIN, "Launch at Login", true, launch_at_login, None);
        let quit = MenuItem::with_id(ID_QUIT, "Quit Pulse", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &open,
            &prefs,
            &PredefinedMenuItem::separator(),
            &login,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| e.to_string())?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("Pulse — system monitor")
            .with_icon(load_icon()?)
            .with_icon_as_template(true)
            // Left click toggles the panel (we handle the event); right click
            // opens this context menu.
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            icon,
            _menu: menu,
            login_item: login,
        })
    }

    /// Set the glanceable number shown beside the icon (or clear it).
    pub fn set_title(&self, text: Option<&str>) {
        self.icon.set_title(text);
    }

    /// Keep the menu checkbox in sync with the real launch-at-login state.
    pub fn set_login_checked(&self, checked: bool) {
        self.login_item.set_checked(checked);
    }
}

fn load_icon() -> Result<Icon, String> {
    let bytes = include_bytes!("../assets/tray-template.png");
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("decode tray icon: {e}"))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).map_err(|e| format!("build tray icon: {e}"))
}
