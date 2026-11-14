//! One module per page. Each `build` returns the page's scrolled widget and
//! registers its rows for search.

pub mod about;
pub mod animation;
pub mod appearance;
pub mod defaultapps;
pub mod displays;
pub mod startmenu;
pub mod system;
pub mod taskbar;
pub mod wallpaper;
pub mod windows;

use gtk4 as gtk;

use crate::catalog::Page;
use crate::ui::App;

pub fn build(app: &App, p: Page) -> gtk::Widget {
    match p {
        Page::Appearance => appearance::build(app),
        Page::Wallpaper => wallpaper::build(app),
        Page::Animation => animation::build(app),
        Page::Taskbar => taskbar::build(app),
        Page::StartMenu => startmenu::build(app),
        Page::Windows => windows::build(app),
        Page::Displays => displays::build(app),
        Page::Sound => system::sound(app),
        Page::Network => system::network(app),
        Page::Power => system::power(app),
        Page::DefaultApps => defaultapps::build(app),
        Page::About => about::build(app),
    }
}
