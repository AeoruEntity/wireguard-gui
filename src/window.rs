use crate::tray::{self, TrayAction, TrayHandle};
use crate::types::*;
use crate::wireguard::WireGuard;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

const WG_CONFIG_TEMPLATE: &str = "[Interface]
Address =
PrivateKey =
ListenPort = 51820
DNS =

[Peer]
PublicKey =
PresharedKey =
Endpoint =
AllowedIPs =";

struct AppState {
    wg: WireGuard,
    conn_state: ConnState,
    current_profile: Option<String>,
    public_ip: Option<String>,
    tray_handle: Option<TrayHandle>,
    loading_overlay: Option<gtk::Box>,
    loading_label: Option<gtk::Label>,
    toast_overlay: Option<adw::ToastOverlay>,
    vpn_details_box: Option<gtk::Box>,
    vpn_ip_label: Option<gtk::Label>,
    endpoint_label: Option<gtk::Label>,
    dns_label: Option<gtk::Label>,
    allowed_ips_label: Option<gtk::Label>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let css = gtk::CssProvider::new();
    css.load_from_data(include_str!("../data/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    gtk::Window::set_default_icon_name("com.aeoru.nvr");

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Aeoru VPN")
        .default_width(440)
        .default_height(660)
        .resizable(false)
        .icon_name("com.aeoru.nvr")
        .build();

    let wg = WireGuard::new();
    let current = wg.get_current();
    let is_up = current
        .as_ref()
        .map(|n| wg.is_interface_up(n))
        .unwrap_or(false);
    let conn_state = if is_up {
        ConnState::Connected
    } else {
        ConnState::Disconnected
    };
    let current_profile = if is_up { current } else { None };

    // Spawn system tray
    let (tray_handle, tray_rx) = if let Some((handle, rx)) = tray::spawn_tray(is_up) {
        (Some(handle), Some(rx))
    } else {
        (None, None)
    };

    let state = Rc::new(RefCell::new(AppState {
        wg,
        conn_state,
        current_profile,
        public_ip: None,
        tray_handle,
        loading_overlay: None,
        loading_label: None,
        toast_overlay: None,
        vpn_details_box: None,
        vpn_ip_label: None,
        endpoint_label: None,
        dns_label: None,
        allowed_ips_label: None,
    }));

    // Hide window on close instead of quitting (minimize to tray)
    {
        let win = window.clone();
        let has_tray = state.borrow().tray_handle.is_some();
        window.connect_close_request(move |_| {
            if has_tray {
                win.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    // Poll tray actions (Open / Quit)
    if let Some(tray_rx) = tray_rx {
        let win = window.clone();
        let app_clone = app.clone();
        glib::idle_add_local(move || match tray_rx.try_recv() {
            Ok(TrayAction::Open) => {
                win.set_visible(true);
                win.present();
                glib::ControlFlow::Continue
            }
            Ok(TrayAction::Quit) => {
                app_clone.quit();
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    }

    // === Main layout with overlays ===
    let root_overlay = gtk::Overlay::new();

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();
    let header_title_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header_title_box.set_halign(gtk::Align::Center);
    let header_logo_bytes = glib::Bytes::from_static(include_bytes!("../data/aeoru-nvr-logo.png"));
    let header_texture =
        gtk::gdk::Texture::from_bytes(&header_logo_bytes).expect("Failed to load header logo");
    let header_logo = gtk::Image::from_paintable(Some(&header_texture));
    header_logo.set_pixel_size(28);
    header_title_box.append(&header_logo);
    header.set_title_widget(Some(&header_title_box));

    let import_btn = gtk::Button::with_label("Import");
    import_btn.add_css_class("flat");
    header.pack_start(&import_btn);

    let export_btn = gtk::Button::with_label("Export");
    export_btn.add_css_class("flat");
    header.pack_end(&export_btn);

    main_box.append(&header);

    // Toast overlay wraps the content area
    let toast_overlay = adw::ToastOverlay::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_top(8);
    content.set_margin_bottom(16);

    // Brand
    let brand = build_brand();
    content.append(&brand);

    // Connection panel
    let (conn_panel, status_dot, status_label, profile_label, ip_label, toggle_btn) =
        build_connection_panel(&state);

    // VPN details (shown when connected)
    let vpn_details_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    vpn_details_box.set_visible(is_up);

    let (vpn_ip_row, vpn_ip_label) = build_detail_row("VPN IP");
    let (endpoint_row, endpoint_label) = build_detail_row("Endpoint");
    let (dns_row, dns_label) = build_detail_row("DNS");
    let (aips_row, allowed_ips_label) = build_detail_row("Allowed IPs");
    vpn_details_box.append(&vpn_ip_row);
    vpn_details_box.append(&endpoint_row);
    vpn_details_box.append(&dns_row);
    vpn_details_box.append(&aips_row);

    // Insert details into conn_panel before the toggle button
    conn_panel.insert_child_after(&vpn_details_box, Some(&ip_label.parent().unwrap()));

    // Populate details if already connected
    if is_up {
        if let Some(ref name) = state.borrow().current_profile {
            if let Some(details) = state.borrow().wg.get_profile_details(name) {
                vpn_ip_label.set_label(if details.address.is_empty() { "\u{2014}" } else { &details.address });
                endpoint_label.set_label(if details.endpoint.is_empty() { "\u{2014}" } else { &details.endpoint });
                dns_label.set_label(if details.dns.is_empty() { "\u{2014}" } else { &details.dns });
                allowed_ips_label.set_label(if details.allowed_ips.is_empty() { "\u{2014}" } else { &details.allowed_ips });
            }
        }
    }

    content.append(&conn_panel);

    // Profile list
    let (profile_section, profile_listbox, add_btn) = build_profile_list();
    content.append(&profile_section);

    // Footer
    let footer = gtk::Label::new(Some("v0.1.0"));
    footer.add_css_class("muted");
    footer.set_margin_top(12);
    content.append(&footer);

    toast_overlay.set_child(Some(&content));
    main_box.append(&toast_overlay);
    root_overlay.set_child(Some(&main_box));

    // === Loading overlay ===
    let (loading_overlay, loading_label) = build_loading_overlay();
    root_overlay.add_overlay(&loading_overlay);

    // === Splash screen overlay ===
    let splash = build_splash_overlay();
    root_overlay.add_overlay(&splash);

    window.set_content(Some(&root_overlay));

    // Store UI widgets in state for easy access
    {
        let mut s = state.borrow_mut();
        s.loading_overlay = Some(loading_overlay.clone());
        s.loading_label = Some(loading_label.clone());
        s.toast_overlay = Some(toast_overlay.clone());
        s.vpn_details_box = Some(vpn_details_box.clone());
        s.vpn_ip_label = Some(vpn_ip_label.clone());
        s.endpoint_label = Some(endpoint_label.clone());
        s.dns_label = Some(dns_label.clone());
        s.allowed_ips_label = Some(allowed_ips_label.clone());
    }

    // Hide splash after 1 second
    {
        let s = splash.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1000), move || {
            s.set_visible(false);
        });
    }

    // -- Fetch IP on startup --
    {
        let (tx, rx) = mpsc::channel::<Option<String>>();
        std::thread::spawn(move || {
            let ip = WireGuard::fetch_public_ip();
            let _ = tx.send(ip);
        });
        let il = ip_label.clone();
        let sr = state.clone();
        glib::idle_add_local(move || match rx.try_recv() {
            Ok(ip) => {
                il.set_label(ip.as_deref().unwrap_or("unavailable"));
                sr.borrow_mut().public_ip = ip;
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    }

    // -- Disconnect button --
    {
        let sr = state.clone();
        let sd = status_dot.clone();
        let sl = status_label.clone();
        let pl = profile_label.clone();
        let il = ip_label.clone();
        let tb = toggle_btn.clone();
        let lb = profile_listbox.clone();
        toggle_btn.connect_clicked(move |_| {
            if sr.borrow().conn_state.is_connected() {
                let name = sr
                    .borrow()
                    .current_profile
                    .clone()
                    .unwrap_or_default();
                {
                    let s = sr.borrow();
                    if let (Some(lo), Some(ll)) = (&s.loading_overlay, &s.loading_label) {
                        ll.set_label(&format!("Disconnecting from {}...", name));
                        lo.set_visible(true);
                    }
                }
                do_disconnect(&sr, &sd, &sl, &pl, &il, &tb, &lb);
            }
        });
    }

    // -- Add profile button --
    {
        let sr = state.clone();
        let lb = profile_listbox.clone();
        let sd = status_dot.clone();
        let sl = status_label.clone();
        let pl = profile_label.clone();
        let il = ip_label.clone();
        let tb = toggle_btn.clone();
        let to = toast_overlay.clone();
        add_btn.connect_clicked(move |_| {
            show_add_dialog(&sr, &lb, &sd, &sl, &pl, &il, &tb, &to);
        });
    }

    // -- Import --
    {
        let sr = state.clone();
        let lb = profile_listbox.clone();
        let sd = status_dot.clone();
        let sl = status_label.clone();
        let pl = profile_label.clone();
        let il = ip_label.clone();
        let tb = toggle_btn.clone();
        let win = window.clone();
        let to = toast_overlay.clone();
        import_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Import WireGuard Profiles")
                .build();
            let filter = gtk::FileFilter::new();
            filter.add_pattern("*.conf");
            filter.set_name(Some("WireGuard configs"));
            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));

            let sr = sr.clone();
            let lb = lb.clone();
            let sd = sd.clone();
            let sl = sl.clone();
            let pl = pl.clone();
            let il = il.clone();
            let tb = tb.clone();
            let to = to.clone();
            dialog.open_multiple(
                Some(&win),
                gtk::gio::Cancellable::NONE,
                move |result: Result<gtk::gio::ListModel, glib::Error>| {
                    if let Ok(files) = result {
                        let mut paths = Vec::new();
                        for i in 0..files.n_items() {
                            if let Some(file) =
                                files.item(i).and_downcast::<gtk::gio::File>()
                            {
                                if let Some(p) = file.path() {
                                    paths.push(p);
                                }
                            }
                        }
                        if !paths.is_empty() {
                            let (ok, failed) = sr.borrow().wg.import_profiles(&paths);
                            if !ok.is_empty() {
                                refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
                                let msg = format!(
                                    "Imported {} profile(s): {}",
                                    ok.len(),
                                    ok.join(", ")
                                );
                                to.add_toast(adw::Toast::new(&msg));
                            }
                            if !failed.is_empty() {
                                let names: Vec<_> =
                                    failed.iter().map(|(n, e)| format!("{n}: {e}")).collect();
                                let msg = format!("Failed: {}", names.join(", "));
                                to.add_toast(adw::Toast::new(&msg));
                            }
                        }
                    }
                },
            );
        });
    }

    // -- Export --
    {
        let sr = state.clone();
        let win = window.clone();
        let to = toast_overlay.clone();
        export_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Export Profiles To...")
                .build();
            let sr = sr.clone();
            let to = to.clone();
            dialog.select_folder(
                Some(&win),
                gtk::gio::Cancellable::NONE,
                move |result: Result<gtk::gio::File, glib::Error>| {
                    if let Ok(folder) = result {
                        if let Some(path) = folder.path() {
                            let (ok, failed) = sr.borrow().wg.export_profiles(&path);
                            if !ok.is_empty() {
                                let msg = format!(
                                    "Exported {} profile(s): {}",
                                    ok.len(),
                                    ok.join(", ")
                                );
                                to.add_toast(adw::Toast::new(&msg));
                            }
                            if !failed.is_empty() {
                                let names: Vec<_> =
                                    failed.iter().map(|(n, e)| format!("{n}: {e}")).collect();
                                let msg = format!("Export failed: {}", names.join(", "));
                                to.add_toast(adw::Toast::new(&msg));
                            }
                            if ok.is_empty() && failed.is_empty() {
                                to.add_toast(adw::Toast::new("No profiles to export"));
                            }
                        }
                    }
                },
            );
        });
    }

    // Populate profile list
    refresh_profile_list(
        &state,
        &profile_listbox,
        &status_dot,
        &status_label,
        &profile_label,
        &ip_label,
        &toggle_btn,
    );

    window
}

fn build_brand() -> gtk::Box {
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    brand.set_margin_bottom(16);
    brand.set_halign(gtk::Align::Center);

    let logo_bytes = glib::Bytes::from_static(include_bytes!("../data/aeoru-logo.png"));
    let texture = gtk::gdk::Texture::from_bytes(&logo_bytes).expect("Failed to load logo");
    let logo_img = gtk::Image::from_paintable(Some(&texture));
    logo_img.set_pixel_size(48);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some("Aeoru VPN"));
    title.add_css_class("brand-title");
    let subtitle = gtk::Label::new(Some("WireGuard VPN Client"));
    subtitle.add_css_class("muted");
    text_box.append(&title);
    text_box.append(&subtitle);

    brand.append(&logo_img);
    brand.append(&text_box);
    brand
}

fn build_detail_row(key: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let k = gtk::Label::new(Some(key));
    k.add_css_class("conn-key");
    k.set_hexpand(true);
    k.set_halign(gtk::Align::Start);
    let v = gtk::Label::new(Some("\u{2014}"));
    v.add_css_class("conn-value");
    v.set_ellipsize(gtk::pango::EllipsizeMode::End);
    v.set_max_width_chars(28);
    row.append(&k);
    row.append(&v);
    (row, v)
}

fn build_loading_overlay() -> (gtk::Box, gtk::Label) {
    let overlay = gtk::Box::new(gtk::Orientation::Vertical, 0);
    overlay.add_css_class("loading-overlay");
    overlay.set_halign(gtk::Align::Fill);
    overlay.set_valign(gtk::Align::Fill);
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_visible(false);

    let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
    center.set_halign(gtk::Align::Center);
    center.set_valign(gtk::Align::Center);
    center.add_css_class("loading-box");

    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_width_request(48);
    spinner.set_height_request(48);
    center.append(&spinner);

    let label = gtk::Label::new(Some("Connecting..."));
    label.add_css_class("loading-text");
    center.append(&label);

    overlay.append(&center);
    (overlay, label)
}

fn build_splash_overlay() -> gtk::Box {
    let splash = gtk::Box::new(gtk::Orientation::Vertical, 0);
    splash.add_css_class("splash-overlay");
    splash.set_halign(gtk::Align::Fill);
    splash.set_valign(gtk::Align::Fill);
    splash.set_hexpand(true);
    splash.set_vexpand(true);

    let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
    center.set_halign(gtk::Align::Center);
    center.set_valign(gtk::Align::Center);

    let logo_bytes = glib::Bytes::from_static(include_bytes!("../data/aeoru-nvr-icon.png"));
    let texture = gtk::gdk::Texture::from_bytes(&logo_bytes).expect("Failed to load splash logo");
    let logo = gtk::Image::from_paintable(Some(&texture));
    logo.set_pixel_size(128);
    logo.add_css_class("pulse");
    center.append(&logo);

    let title = gtk::Label::new(Some("Aeoru VPN"));
    title.add_css_class("splash-title");
    center.append(&title);

    let subtitle = gtk::Label::new(Some("WireGuard VPN Client"));
    subtitle.add_css_class("splash-subtitle");
    center.append(&subtitle);

    splash.append(&center);
    splash
}

fn build_connection_panel(
    state: &Rc<RefCell<AppState>>,
) -> (
    gtk::Box,
    gtk::Label,
    gtk::Label,
    gtk::Label,
    gtk::Label,
    gtk::Button,
) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 8);
    panel.add_css_class("conn-panel");
    panel.set_margin_bottom(16);

    let s = state.borrow();

    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let status_dot = gtk::Label::new(Some("\u{25cf}"));
    status_dot.add_css_class(if s.conn_state.is_connected() {
        "status-online"
    } else {
        "status-offline"
    });
    let status_label = gtk::Label::new(Some(s.conn_state.label()));
    status_label.add_css_class("status-text");
    status_row.append(&status_dot);
    status_row.append(&status_label);
    panel.append(&status_row);

    let profile_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let pkey = gtk::Label::new(Some("Profile"));
    pkey.add_css_class("conn-key");
    pkey.set_hexpand(true);
    pkey.set_halign(gtk::Align::Start);
    let profile_label = gtk::Label::new(Some(
        s.current_profile.as_deref().unwrap_or("\u{2014}"),
    ));
    profile_label.add_css_class("conn-value");
    profile_row.append(&pkey);
    profile_row.append(&profile_label);
    panel.append(&profile_row);

    let ip_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let ikey = gtk::Label::new(Some("Public IP"));
    ikey.add_css_class("conn-key");
    ikey.set_hexpand(true);
    ikey.set_halign(gtk::Align::Start);
    let ip_label = gtk::Label::new(Some("fetching..."));
    ip_label.add_css_class("conn-value");
    ip_row.append(&ikey);
    ip_row.append(&ip_label);
    panel.append(&ip_row);

    let toggle_btn = gtk::Button::with_label("Disconnect");
    toggle_btn.add_css_class("btn-danger");
    toggle_btn.set_margin_top(8);
    toggle_btn.set_visible(s.conn_state.is_connected());
    panel.append(&toggle_btn);

    drop(s);
    (
        panel,
        status_dot,
        status_label,
        profile_label,
        ip_label,
        toggle_btn,
    )
}

fn build_profile_list() -> (gtk::Box, gtk::ListBox, gtk::Button) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_vexpand(true);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Profiles"));
    title.add_css_class("section-title");
    title.set_hexpand(true);
    title.set_halign(gtk::Align::Start);
    let add_btn = gtk::Button::with_label("+ Add");
    add_btn.add_css_class("btn-primary");
    add_btn.add_css_class("btn-sm");
    header.append(&title);
    header.append(&add_btn);
    section.append(&header);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search profiles..."));
    search.add_css_class("form-entry");
    section.append(&search);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_min_content_height(200);

    let listbox = gtk::ListBox::new();
    listbox.set_selection_mode(gtk::SelectionMode::None);
    listbox.add_css_class("profile-list");
    scroll.set_child(Some(&listbox));
    section.append(&scroll);

    {
        let lb = listbox.clone();
        search.connect_search_changed(move |entry| {
            let query = entry.text().to_string().to_lowercase();
            let mut idx = 0;
            while let Some(row) = lb.row_at_index(idx) {
                if query.is_empty() {
                    row.set_visible(true);
                } else {
                    row.set_visible(
                        row.widget_name()
                            .to_string()
                            .to_lowercase()
                            .contains(&query),
                    );
                }
                idx += 1;
            }
        });
    }

    (section, listbox, add_btn)
}

fn refresh_profile_list(
    state: &Rc<RefCell<AppState>>,
    listbox: &gtk::ListBox,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
) {
    while let Some(row) = listbox.row_at_index(0) {
        listbox.remove(&row);
    }

    let profiles = state.borrow().wg.list_profiles();
    let current = state.borrow().current_profile.clone();

    for profile in &profiles {
        let is_active = current.as_deref() == Some(&profile.name);
        let row = build_profile_row(profile, is_active, state, listbox, sd, sl, pl, il, tb);
        listbox.append(&row);
    }
}

fn build_profile_row(
    profile: &crate::types::Profile,
    is_active: bool,
    state: &Rc<RefCell<AppState>>,
    listbox: &gtk::ListBox,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("profile-row");
    row.set_widget_name(&profile.name);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);

    let dot = gtk::Label::new(Some("\u{25cf}"));
    dot.add_css_class(if is_active {
        "dot-online"
    } else {
        "dot-offline"
    });

    let name_label = gtk::Label::new(Some(&profile.name));
    name_label.add_css_class("profile-name");
    name_label.set_hexpand(true);
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

    hbox.append(&dot);
    hbox.append(&name_label);

    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    if !is_active {
        let connect_btn = gtk::Button::with_label("Connect");
        connect_btn.add_css_class("btn-primary");
        connect_btn.add_css_class("btn-sm");
        let name = profile.name.clone();
        let sr = state.clone();
        let lb = listbox.clone();
        let sd = sd.clone();
        let sl = sl.clone();
        let pl = pl.clone();
        let il = il.clone();
        let tb = tb.clone();
        connect_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("...");
            do_connect(&name, &sr, &sd, &sl, &pl, &il, &tb, &lb);
        });
        btn_box.append(&connect_btn);
    }

    let edit_btn = gtk::Button::with_label("\u{270e}");
    edit_btn.add_css_class("btn-icon");
    edit_btn.set_tooltip_text(Some("Edit"));
    {
        let p = profile.clone();
        let sr = state.clone();
        let lb = listbox.clone();
        let sd = sd.clone();
        let sl = sl.clone();
        let pl = pl.clone();
        let il = il.clone();
        let tb = tb.clone();
        edit_btn.connect_clicked(move |_| {
            show_edit_dialog(&p, &sr, &lb, &sd, &sl, &pl, &il, &tb);
        });
    }

    let del_btn = gtk::Button::with_label("\u{1f5d1}");
    del_btn.add_css_class("btn-icon");
    del_btn.set_tooltip_text(Some("Delete"));
    {
        let name = profile.name.clone();
        let sr = state.clone();
        let lb = listbox.clone();
        let sd = sd.clone();
        let sl = sl.clone();
        let pl = pl.clone();
        let il = il.clone();
        let tb = tb.clone();
        del_btn.connect_clicked(move |_| {
            show_delete_dialog(&name, &sr, &lb, &sd, &sl, &pl, &il, &tb);
        });
    }

    btn_box.append(&edit_btn);
    btn_box.append(&del_btn);
    hbox.append(&btn_box);
    row.set_child(Some(&hbox));
    row
}

fn do_connect(
    name: &str,
    state: &Rc<RefCell<AppState>>,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
    lb: &gtk::ListBox,
) {
    let name = name.to_string();

    // Show loading overlay
    {
        let s = state.borrow();
        if let (Some(lo), Some(ll)) = (&s.loading_overlay, &s.loading_label) {
            ll.set_label(&format!("Connecting to {}...", name));
            lo.set_visible(true);
        }
    }

    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let name_clone = name.clone();
    std::thread::spawn(move || {
        let wg = WireGuard::new();
        let result = wg.connect(&name_clone);
        let _ = tx.send(result);
    });

    let sr = state.clone();
    let sd = sd.clone();
    let sl = sl.clone();
    let pl = pl.clone();
    let il = il.clone();
    let tb = tb.clone();
    let lb = lb.clone();
    let name2 = name.clone();
    glib::idle_add_local(move || match rx.try_recv() {
        Ok(Ok(())) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            sr.borrow_mut().conn_state = ConnState::Connected;
            sr.borrow_mut().current_profile = Some(name2.clone());
            if let Some(handle) = &mut sr.borrow_mut().tray_handle {
                handle.set_connected(true);
            }
            update_conn_ui(&sd, &sl, &pl, &il, &tb, true, Some(&name2));
            show_vpn_details(&sr, &name2);
            refresh_ip(&il, &sr);
            refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
            if let Some(to) = &sr.borrow().toast_overlay {
                to.add_toast(adw::Toast::new(&format!("Connected to {}", name2)));
            }
            glib::ControlFlow::Break
        }
        Ok(Err(e)) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            hide_vpn_details(&sr);
            refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
            if let Some(to) = &sr.borrow().toast_overlay {
                to.add_toast(adw::Toast::new(&format!("Connection failed: {}", e)));
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(_) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            glib::ControlFlow::Break
        }
    });
}

fn do_disconnect(
    state: &Rc<RefCell<AppState>>,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
    lb: &gtk::ListBox,
) {
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    std::thread::spawn(move || {
        let wg = WireGuard::new();
        let result = wg.disconnect();
        let _ = tx.send(result);
    });

    let sr = state.clone();
    let sd = sd.clone();
    let sl = sl.clone();
    let pl = pl.clone();
    let il = il.clone();
    let tb = tb.clone();
    let lb = lb.clone();
    glib::idle_add_local(move || match rx.try_recv() {
        Ok(Ok(())) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            sr.borrow_mut().conn_state = ConnState::Disconnected;
            sr.borrow_mut().current_profile = None;
            if let Some(handle) = &mut sr.borrow_mut().tray_handle {
                handle.set_connected(false);
            }
            update_conn_ui(&sd, &sl, &pl, &il, &tb, false, None);
            hide_vpn_details(&sr);
            refresh_ip(&il, &sr);
            refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
            if let Some(to) = &sr.borrow().toast_overlay {
                to.add_toast(adw::Toast::new("Disconnected"));
            }
            glib::ControlFlow::Break
        }
        Ok(Err(e)) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            if let Some(to) = &sr.borrow().toast_overlay {
                to.add_toast(adw::Toast::new(&format!("Error: {e}")));
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(_) => {
            if let Some(lo) = &sr.borrow().loading_overlay {
                lo.set_visible(false);
            }
            glib::ControlFlow::Break
        }
    });
}

fn refresh_ip(il: &gtk::Label, sr: &Rc<RefCell<AppState>>) {
    il.set_label("fetching...");
    let (tx, rx) = mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let ip = WireGuard::fetch_public_ip();
        let _ = tx.send(ip);
    });
    let il = il.clone();
    let sr = sr.clone();
    glib::idle_add_local(move || match rx.try_recv() {
        Ok(ip) => {
            il.set_label(ip.as_deref().unwrap_or("unavailable"));
            sr.borrow_mut().public_ip = ip;
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(_) => glib::ControlFlow::Break,
    });
}

fn update_conn_ui(
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    _il: &gtk::Label,
    tb: &gtk::Button,
    connected: bool,
    profile_name: Option<&str>,
) {
    sd.remove_css_class("status-online");
    sd.remove_css_class("status-offline");
    if connected {
        sd.add_css_class("status-online");
        sl.set_label("Connected");
        pl.set_label(profile_name.unwrap_or("\u{2014}"));
        tb.set_visible(true);
    } else {
        sd.add_css_class("status-offline");
        sl.set_label("Disconnected");
        pl.set_label("\u{2014}");
        tb.set_visible(false);
    }
}

fn show_vpn_details(state: &Rc<RefCell<AppState>>, profile_name: &str) {
    let s = state.borrow();
    if let Some(details) = s.wg.get_profile_details(profile_name) {
        if let Some(l) = &s.vpn_ip_label {
            l.set_label(if details.address.is_empty() { "\u{2014}" } else { &details.address });
        }
        if let Some(l) = &s.endpoint_label {
            l.set_label(if details.endpoint.is_empty() { "\u{2014}" } else { &details.endpoint });
        }
        if let Some(l) = &s.dns_label {
            l.set_label(if details.dns.is_empty() { "\u{2014}" } else { &details.dns });
        }
        if let Some(l) = &s.allowed_ips_label {
            l.set_label(if details.allowed_ips.is_empty() { "\u{2014}" } else { &details.allowed_ips });
        }
    }
    if let Some(b) = &s.vpn_details_box {
        b.set_visible(true);
    }
}

fn hide_vpn_details(state: &Rc<RefCell<AppState>>) {
    let s = state.borrow();
    if let Some(b) = &s.vpn_details_box {
        b.set_visible(false);
    }
}

fn show_add_dialog(
    state: &Rc<RefCell<AppState>>,
    listbox: &gtk::ListBox,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
    toast_overlay: &adw::ToastOverlay,
) {
    let dialog = adw::Window::builder()
        .title("Add Profile")
        .default_width(400)
        .default_height(400)
        .modal(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());

    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_start(20);
    form.set_margin_end(20);
    form.set_margin_top(12);
    form.set_margin_bottom(20);

    let nl = gtk::Label::new(Some("Profile Name"));
    nl.add_css_class("form-label");
    nl.set_halign(gtk::Align::Start);
    let ne = gtk::Entry::new();
    ne.set_placeholder_text(Some("e.g. my-vpn"));
    ne.add_css_class("form-entry");

    let cl = gtk::Label::new(Some("WireGuard Config"));
    cl.add_css_class("form-label");
    cl.set_halign(gtk::Align::Start);
    let cs = gtk::ScrolledWindow::new();
    cs.set_min_content_height(180);
    cs.set_vexpand(true);
    let cv = gtk::TextView::new();
    cv.set_monospace(true);
    cv.add_css_class("form-entry");
    cv.set_wrap_mode(gtk::WrapMode::WordChar);
    // Pre-fill with WireGuard config template
    cv.buffer().set_text(WG_CONFIG_TEMPLATE);
    cs.set_child(Some(&cv));

    let el = gtk::Label::new(None);
    el.add_css_class("error-text");
    el.set_visible(false);
    el.set_halign(gtk::Align::Start);

    let bb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bb.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("btn-ghost");
    let save = gtk::Button::with_label("Create");
    save.add_css_class("btn-primary");
    bb.append(&cancel);
    bb.append(&save);

    form.append(&nl);
    form.append(&ne);
    form.append(&cl);
    form.append(&cs);
    form.append(&el);
    form.append(&bb);
    content.append(&form);
    dialog.set_content(Some(&content));

    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());

    let d = dialog.clone();
    let sr = state.clone();
    let lb = listbox.clone();
    let sd = sd.clone();
    let sl = sl.clone();
    let pl = pl.clone();
    let il = il.clone();
    let tb = tb.clone();
    let to = toast_overlay.clone();
    save.connect_clicked(move |_| {
        let name = ne.text().to_string();
        let buf = cv.buffer();
        let cfg = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        match sr.borrow().wg.create_profile(&name, &cfg) {
            Ok(()) => {
                refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
                to.add_toast(adw::Toast::new(&format!("Profile '{name}' created")));
                d.close();
            }
            Err(e) => {
                el.set_label(&e);
                el.set_visible(true);
            }
        }
    });

    dialog.present();
}

fn show_edit_dialog(
    profile: &crate::types::Profile,
    state: &Rc<RefCell<AppState>>,
    listbox: &gtk::ListBox,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
) {
    let dialog = adw::Window::builder()
        .title("Edit Profile")
        .default_width(400)
        .default_height(400)
        .modal(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());

    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_start(20);
    form.set_margin_end(20);
    form.set_margin_top(12);
    form.set_margin_bottom(20);

    let nl = gtk::Label::new(Some(&format!("Profile: {}", profile.name)));
    nl.add_css_class("section-title");
    nl.set_halign(gtk::Align::Start);

    let cl = gtk::Label::new(Some("WireGuard Config"));
    cl.add_css_class("form-label");
    cl.set_halign(gtk::Align::Start);
    let cs = gtk::ScrolledWindow::new();
    cs.set_min_content_height(220);
    cs.set_vexpand(true);
    let cv = gtk::TextView::new();
    cv.set_monospace(true);
    cv.add_css_class("form-entry");
    cv.set_wrap_mode(gtk::WrapMode::WordChar);
    cv.buffer().set_text(&profile.content);
    cs.set_child(Some(&cv));

    let bb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bb.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("btn-ghost");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("btn-primary");
    bb.append(&cancel);
    bb.append(&save);

    form.append(&nl);
    form.append(&cl);
    form.append(&cs);
    form.append(&bb);
    content.append(&form);
    dialog.set_content(Some(&content));

    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());

    let d = dialog.clone();
    let name = profile.name.clone();
    let sr = state.clone();
    let lb = listbox.clone();
    let sd = sd.clone();
    let sl = sl.clone();
    let pl = pl.clone();
    let il = il.clone();
    let tb = tb.clone();
    save.connect_clicked(move |_| {
        let buf = cv.buffer();
        let cfg = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        if sr.borrow().wg.update_profile(&name, &cfg).is_ok() {
            let is_active = sr.borrow().current_profile.as_deref() == Some(name.as_str());
            if is_active {
                do_connect(&name, &sr, &sd, &sl, &pl, &il, &tb, &lb);
            }
            refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
            d.close();
        }
    });

    dialog.present();
}

fn show_delete_dialog(
    name: &str,
    state: &Rc<RefCell<AppState>>,
    listbox: &gtk::ListBox,
    sd: &gtk::Label,
    sl: &gtk::Label,
    pl: &gtk::Label,
    il: &gtk::Label,
    tb: &gtk::Button,
) {
    let dialog = adw::Window::builder()
        .title("Delete Profile")
        .default_width(360)
        .default_height(180)
        .modal(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_start(20);
    body.set_margin_end(20);
    body.set_margin_top(12);
    body.set_margin_bottom(20);

    let msg = gtk::Label::new(Some(&format!(
        "Are you sure you want to delete \"{name}\"?\nThis action cannot be undone."
    )));
    msg.set_wrap(true);
    msg.set_halign(gtk::Align::Start);

    let bb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bb.set_halign(gtk::Align::End);
    bb.set_margin_top(8);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("btn-ghost");
    let del = gtk::Button::with_label("Delete");
    del.add_css_class("btn-danger");
    bb.append(&cancel);
    bb.append(&del);

    body.append(&msg);
    body.append(&bb);
    content.append(&body);
    dialog.set_content(Some(&content));

    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());

    let d = dialog.clone();
    let name = name.to_string();
    let sr = state.clone();
    let lb = listbox.clone();
    let sd = sd.clone();
    let sl = sl.clone();
    let pl = pl.clone();
    let il = il.clone();
    let tb = tb.clone();
    del.connect_clicked(move |_| {
        let was_active = sr.borrow().current_profile.as_deref() == Some(name.as_str());
        if sr.borrow().wg.delete_profile(&name).is_ok() {
            if was_active {
                sr.borrow_mut().conn_state = ConnState::Disconnected;
                sr.borrow_mut().current_profile = None;
                update_conn_ui(&sd, &sl, &pl, &il, &tb, false, None);
            }
            refresh_profile_list(&sr, &lb, &sd, &sl, &pl, &il, &tb);
        }
        d.close();
    });

    dialog.present();
}
