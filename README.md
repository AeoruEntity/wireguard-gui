# Aeoru VPN

A native Linux desktop application for managing WireGuard VPN connections, built with **Rust**, **GTK4**, and **libadwaita**.

![Aeoru VPN](data/aeoru-nvr-dashboard.png)

## Features

- **Profile Management** - Create, edit, delete, import, and export WireGuard profiles
- **One-Click Connect/Disconnect** - Connect to any saved profile with a single click
- **Connection Details** - View VPN IP, endpoint, DNS, allowed IPs, and public IP when connected
- **System Tray** - Minimize to tray with connected/disconnected status icons, Open/Quit menu
- **Loading Overlay** - Visual feedback during connect/disconnect operations
- **Error Toasts** - Clear error messages when connections fail (e.g., wrong config)
- **Splash Screen** - Branded splash screen on startup
- **Config Template** - Pre-filled WireGuard config template when creating new profiles
- **Import/Export** - Bulk import `.conf` files or export all profiles to a directory
- **Dark Theme** - Native dark theme via libadwaita

## Screenshots

| Disconnected | Connected |
|---|---|
| Status dot turns red, no VPN details shown | Status dot turns green, full VPN details visible |

## Installation

### From .deb Package (Ubuntu/Debian)

Download the latest `.deb` from [Releases](https://github.com/AeoruEntity/wireguard-gui/releases) or build it yourself:

```bash
sudo dpkg -i aeoru-vpn_0.1.0_amd64.deb
```

### Build from Source

#### Prerequisites

- **Rust** (1.70+) - [Install](https://rustup.rs/)
- **GTK4 development libraries** (4.10+)
- **libadwaita development libraries** (1.4+)
- **WireGuard tools**
- **Python 3** (for system tray)
- **AyatanaAppIndicator3** GObject Introspection bindings (for system tray)

On Ubuntu/Debian:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev wireguard-tools python3 gir1.2-ayatanaappindicator3-0.1
```

#### Build & Run

```bash
git clone https://github.com/AeoruEntity/wireguard-gui.git
cd wireguard-gui
cargo build --release
./target/release/aeoru-nvr
```

## Building the .deb Package

A build script is included that handles everything:

```bash
./build-deb.sh
```

This will:

1. Build the release binary with `cargo build --release`
2. Assemble the `.deb` directory structure (binary, desktop file, icons)
3. Resize the app icon to all standard sizes (requires ImageMagick)
4. Package it with `dpkg-deb`

The resulting `.deb` is output to the project root.

#### Build Dependencies (in addition to the above)

```bash
sudo apt install dpkg-dev imagemagick
```

#### Install / Uninstall

```bash
# Install
sudo dpkg -i aeoru-vpn_0.1.0_amd64.deb

# Uninstall
sudo dpkg -r aeoru-vpn
```

## Project Structure

```
.
├── src/
│   ├── main.rs         # App entry point, GTK application setup
│   ├── window.rs       # Main window UI (connection panel, profiles, overlays)
│   ├── wireguard.rs    # WireGuard operations (connect, disconnect, profiles)
│   ├── tray.rs         # System tray via Python/AyatanaAppIndicator3 subprocess
│   └── types.rs        # Data types (Profile, ConnState, VpnDetails)
├── data/
│   ├── style.css               # Application stylesheet
│   ├── com.aeoru.nvr.desktop   # Desktop entry file
│   ├── aeoru-logo.png          # Aeoru octopus logo
│   ├── aeoru-nvr-icon.png      # Square app icon (used for taskbar & deb)
│   ├── aeoru-nvr-logo.png      # Combined logo for header bar
│   ├── aeoru-nvr-dashboard.png # Dashboard logo with title
│   ├── tray-connected.png      # 22x22 tray icon (connected state)
│   └── tray-disconnected.png   # 22x22 tray icon (disconnected state)
├── pkg/
│   └── DEBIAN/
│       ├── control     # Package metadata and dependencies
│       ├── postinst    # Post-install hook (icon cache, desktop db)
│       └── postrm      # Post-remove hook (cleanup)
├── build-deb.sh        # Script to build the .deb package
├── Cargo.toml          # Rust dependencies
└── README.md
```

## How It Works

- **VPN management** uses `wg-quick up/down` via `pkexec` for privilege escalation - no daemon required
- **System tray** runs a Python subprocess using GTK3's `AyatanaAppIndicator3` (the native GNOME tray API), communicating with the Rust app via stdin/stdout pipes
- **Profiles** are stored as `.conf` files in `~/.config/aeoru-vpn/profiles/`
- **All assets** (icons, CSS, logos) are embedded in the binary at compile time via `include_bytes!` / `include_str!`

## Dependencies

| Crate | Purpose |
|---|---|
| `gtk4` (0.8) | GTK4 bindings with `v4_10` features |
| `libadwaita` (0.6) | Adwaita widgets with `v1_4` features |
| `serde` + `serde_json` | Profile serialization |
| `ureq` | HTTP client for public IP lookup |

## Runtime Dependencies

| Package | Purpose |
|---|---|
| `libgtk-4-1` (>= 4.10) | GTK4 runtime |
| `libadwaita-1-0` (>= 1.4) | Adwaita runtime |
| `wireguard-tools` | `wg-quick` for VPN management |
| `python3` | System tray subprocess |
| `gir1.2-ayatanaappindicator3-0.1` | Tray icon on GNOME/Ubuntu |

## License

MIT
