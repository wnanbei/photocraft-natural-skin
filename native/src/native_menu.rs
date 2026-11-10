//! Translate our macOS menu titles in place. Actions, key equivalents and native
//! responder validation are preserved; OS-supplied Services remain OS-owned.
use natural_skin_companion::i18n::Language;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSMenu};
use objc2_foundation::NSString;

pub fn translate(language: Language) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(main_thread);
    if let Some(menu) = app.mainMenu() {
        translate_menu(&menu, language, 0);
    }
}
fn translate_menu(menu: &NSMenu, language: Language, depth: u8) {
    if depth > 2 {
        return;
    }
    for item in menu.itemArray() {
        let title = item.title().to_string();
        let source = match title.as_str() {
            "About natural-skin" => "About Natural Skin",
            "Hide natural-skin" => "Hide Natural Skin",
            "Quit natural-skin" => "Quit Natural Skin",
            _ => title.as_str(),
        };
        for key in [
            "About Natural Skin",
            "Hide Natural Skin",
            "Quit Natural Skin",
            "Hide Others",
            "Show All",
            "Quit and Keep Windows",
            "Services",
            "Edit",
            "Cut",
            "Copy",
            "Paste",
            "Select All",
        ] {
            if source == key || source == Language::Chinese.text(key) {
                let title = NSString::from_str(&language.text(key));
                item.setTitle(&title);
                // Keep submenu titles synchronized, too (including accessibility).
                if let Some(submenu) = item.submenu() {
                    submenu.setTitle(&title);
                }
                break;
            }
        }
        if source != "Services"
            && source != Language::Chinese.text("Services")
            && let Some(submenu) = item.submenu()
        {
            translate_menu(&submenu, language, depth + 1);
        }
    }
}
