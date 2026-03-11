use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

#[derive(Debug)]
pub enum TrayAction {
    Open,
    Quit,
}

const TRAY_SCRIPT: &str = r#"
import gi, sys, os, signal
gi.require_version('Gtk', '3.0')
gi.require_version('AyatanaAppIndicator3', '0.1')
from gi.repository import Gtk, AyatanaAppIndicator3, GLib

connected_icon = sys.argv[1]
disconnected_icon = sys.argv[2]

indicator = AyatanaAppIndicator3.Indicator.new(
    "aeoru-vpn",
    disconnected_icon,
    AyatanaAppIndicator3.IndicatorCategory.APPLICATION_STATUS
)
indicator.set_status(AyatanaAppIndicator3.IndicatorStatus.ACTIVE)

def on_open(_):
    print("OPEN", flush=True)

def on_quit(_):
    print("QUIT", flush=True)
    Gtk.main_quit()

menu = Gtk.Menu()

item_title = Gtk.MenuItem(label="Aeoru VPN")
item_title.set_sensitive(False)
menu.append(item_title)

menu.append(Gtk.SeparatorMenuItem())

item_open = Gtk.MenuItem(label="Open")
item_open.connect("activate", on_open)
menu.append(item_open)

item_quit = Gtk.MenuItem(label="Quit")
item_quit.connect("activate", on_quit)
menu.append(item_quit)

menu.show_all()
indicator.set_menu(menu)

def read_stdin(channel, condition):
    try:
        line = sys.stdin.readline().strip()
        if line == "CONNECTED":
            indicator.set_icon(connected_icon)
        elif line == "DISCONNECTED":
            indicator.set_icon(disconnected_icon)
        elif line == "STOP":
            Gtk.main_quit()
            return False
        elif not line:
            Gtk.main_quit()
            return False
    except:
        Gtk.main_quit()
        return False
    return True

channel = GLib.IOChannel.unix_new(sys.stdin.fileno())
GLib.io_add_watch(channel, GLib.PRIORITY_DEFAULT, GLib.IOCondition.IN, read_stdin)

signal.signal(signal.SIGTERM, lambda *a: Gtk.main_quit())
Gtk.main()
"#;

pub struct TrayHandle {
    child: Child,
}

impl TrayHandle {
    pub fn set_connected(&mut self, connected: bool) {
        if let Some(stdin) = &mut self.child.stdin {
            let cmd = if connected { "CONNECTED\n" } else { "DISCONNECTED\n" };
            let _ = stdin.write_all(cmd.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(stdin) = &mut self.child.stdin {
            let _ = stdin.write_all(b"STOP\n");
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn spawn_tray(connected: bool) -> Option<(TrayHandle, mpsc::Receiver<TrayAction>)> {
    // Write icon files to temp dir
    let tmp = std::env::temp_dir().join("aeoru-vpn-tray");
    std::fs::create_dir_all(&tmp).ok()?;

    let conn_icon = tmp.join("connected.png");
    let disc_icon = tmp.join("disconnected.png");
    std::fs::write(&conn_icon, include_bytes!("../data/tray-connected.png")).ok()?;
    std::fs::write(&disc_icon, include_bytes!("../data/tray-disconnected.png")).ok()?;

    // Write script
    let script_path = tmp.join("tray.py");
    std::fs::write(&script_path, TRAY_SCRIPT).ok()?;

    let initial_icon = if connected { &conn_icon } else { &disc_icon };
    let _ = initial_icon; // we pass both, script picks disconnected by default

    let mut child = Command::new("python3")
        .arg(&script_path)
        .arg(conn_icon.to_str()?)
        .arg(disc_icon.to_str()?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            match line.trim() {
                "OPEN" => { let _ = tx.send(TrayAction::Open); }
                "QUIT" => { let _ = tx.send(TrayAction::Quit); break; }
                _ => {}
            }
        }
    });

    let mut handle = TrayHandle { child };
    if connected {
        handle.set_connected(true);
    }

    Some((handle, rx))
}
