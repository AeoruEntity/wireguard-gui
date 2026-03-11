use crate::types::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

pub struct WireGuard {
    config_dir: PathBuf,
    profiles_dir: PathBuf,
}

impl WireGuard {
    pub fn new() -> Self {
        let home = std::env::var("HOME").expect("HOME not set");
        let config_dir = PathBuf::from(&home).join(".config/aeoru-vpn");
        let profiles_dir = config_dir.join("profiles");

        // Migrate from old aeoru-nvr config dir if it exists
        let old_dir = PathBuf::from(&home).join(".config/aeoru-nvr");
        if old_dir.exists() && !config_dir.exists() {
            let _ = fs::rename(&old_dir, &config_dir);
        }

        fs::create_dir_all(&profiles_dir).ok();
        Self {
            config_dir,
            profiles_dir,
        }
    }

    pub fn get_current(&self) -> Option<String> {
        fs::read_to_string(self.config_dir.join("current"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn set_current(&self, name: &str) {
        fs::write(self.config_dir.join("current"), name).ok();
    }

    fn clear_current(&self) {
        fs::remove_file(self.config_dir.join("current")).ok();
    }

    pub fn is_interface_up(&self, name: &str) -> bool {
        Command::new("ip")
            .args(["-br", "link", "show", "dev", name])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    pub fn list_profiles(&self) -> Vec<Profile> {
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "conf").unwrap_or(false) {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let content = fs::read_to_string(&path).unwrap_or_default();
                    profiles.push(Profile { name, content });
                }
            }
        }
        profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        profiles
    }

    pub fn create_profile(&self, name: &str, content: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Profile name cannot be empty".into());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err("Name must be alphanumeric (dashes/underscores allowed)".into());
        }
        let path = self.profiles_dir.join(format!("{name}.conf"));
        if path.exists() {
            return Err(format!("Profile '{name}' already exists"));
        }
        fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn update_profile(&self, name: &str, content: &str) -> Result<(), String> {
        let path = self.profiles_dir.join(format!("{name}.conf"));
        if !path.exists() {
            return Err(format!("Profile '{name}' not found"));
        }
        fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn delete_profile(&self, name: &str) -> Result<(), String> {
        // Disconnect if this profile is active
        if self.get_current().as_deref() == Some(name) {
            self.exec_wg(name)?;
            self.clear_current();
        }
        let path = self.profiles_dir.join(format!("{name}.conf"));
        fs::remove_file(&path).map_err(|e| e.to_string())
    }

    pub fn connect(&self, name: &str) -> Result<(), String> {
        // Disconnect current if different
        if let Some(current) = self.get_current() {
            if current != name && self.is_interface_up(&current) {
                self.exec_wg(&current)?;
            }
        }

        self.exec_wg(name)?;
        self.set_current(name);
        Ok(())
    }

    pub fn disconnect(&self) -> Result<(), String> {
        if let Some(current) = self.get_current() {
            if self.is_interface_up(&current) {
                self.exec_wg(&current)?;
            }
            self.clear_current();
        }
        Ok(())
    }

    fn exec_wg(&self, profile: &str) -> Result<(), String> {
        let config_dir = self.config_dir.to_string_lossy();
        let script_content = format!(
            r#"#!/bin/bash
cp -f "{config_dir}/profiles/{profile}.conf" "/etc/wireguard/{profile}.conf"
if ip a | grep -q {profile}; then
    wg-quick down {profile}
else
    wg-quick up {profile}
fi
"#
        );

        let script_path = "/tmp/aeoru-vpn-wg.sh";
        fs::write(script_path, &script_content).map_err(|e| e.to_string())?;
        fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        let status = Command::new("pkexec")
            .arg(script_path)
            .status()
            .map_err(|e| format!("Failed to run pkexec: {e}"))?;

        fs::remove_file(script_path).ok();

        if status.success() {
            Ok(())
        } else {
            Err("WireGuard command failed (cancelled or error)".into())
        }
    }

    pub fn import_profiles(&self, paths: &[PathBuf]) -> (Vec<String>, Vec<(String, String)>) {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for path in paths {
            let file_name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            // Sanitize name
            let name: String = file_name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect();

            if name.is_empty() {
                failed.push((file_name, "Invalid filename".into()));
                continue;
            }

            match fs::read_to_string(path) {
                Ok(content) => {
                    let target = self.profiles_dir.join(format!("{name}.conf"));
                    if target.exists() {
                        failed.push((name, "Profile already exists".into()));
                    } else {
                        match fs::write(&target, &content) {
                            Ok(_) => success.push(name),
                            Err(e) => failed.push((name, e.to_string())),
                        }
                    }
                }
                Err(e) => failed.push((file_name, e.to_string())),
            }
        }

        (success, failed)
    }

    pub fn export_profiles(&self, target_dir: &PathBuf) -> (Vec<String>, Vec<(String, String)>) {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for profile in self.list_profiles() {
            let src = self.profiles_dir.join(format!("{}.conf", profile.name));
            let dst = target_dir.join(format!("{}.conf", profile.name));
            match fs::copy(&src, &dst) {
                Ok(_) => success.push(profile.name),
                Err(e) => failed.push((profile.name, e.to_string())),
            }
        }

        (success, failed)
    }

    pub fn get_profile_details(&self, name: &str) -> Option<VpnDetails> {
        let path = self.profiles_dir.join(format!("{name}.conf"));
        let content = std::fs::read_to_string(&path).ok()?;
        let mut address = String::new();
        let mut endpoint = String::new();
        let mut dns = String::new();
        let mut allowed_ips = String::new();

        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("Address") {
                address = val.trim_start_matches(|c: char| c == ' ' || c == '=').trim().to_string();
            } else if let Some(val) = line.strip_prefix("Endpoint") {
                endpoint = val.trim_start_matches(|c: char| c == ' ' || c == '=').trim().to_string();
            } else if let Some(val) = line.strip_prefix("DNS") {
                dns = val.trim_start_matches(|c: char| c == ' ' || c == '=').trim().to_string();
            } else if let Some(val) = line.strip_prefix("AllowedIPs") {
                allowed_ips = val.trim_start_matches(|c: char| c == ' ' || c == '=').trim().to_string();
            }
        }

        Some(VpnDetails { address, endpoint, dns, allowed_ips })
    }

    pub fn fetch_public_ip() -> Option<String> {
        ureq::get("https://httpbin.org/ip")
            .call()
            .ok()
            .and_then(|r| r.into_json::<IpResponse>().ok())
            .map(|r| r.origin)
    }
}
