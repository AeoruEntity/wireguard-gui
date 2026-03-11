mod tray;
mod types;
mod window;
mod wireguard;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("com.aeoru.nvr")
        .build();

    app.connect_activate(|app| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        window::build(app).present();
    });

    app.run()
}
