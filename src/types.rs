use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum ConnState {
    Connected,
    Disconnected,
}

impl ConnState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone)]
pub struct VpnDetails {
    pub address: String,
    pub endpoint: String,
    pub dns: String,
    pub allowed_ips: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpResponse {
    pub origin: String,
}
