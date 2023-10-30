use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub sessions: Vec<SessionConfig>,
}

impl Config {
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Self {
        let config_str = fs::read_to_string(path).expect("to be able to load config");
        toml::from_str::<Self>(&config_str).expect("to be able to parse config")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TlsConfig {
    pub ca_certificate_path: String,
}

fn default_reconnect_interval() -> u64 {
    30
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionConfig {
    pub begin_string: String,
    pub sender_comp_id: String,
    pub target_comp_id: String,
    pub data_dictionary_path: String,
    pub connection_host: String,
    pub connection_port: u16,
    #[serde(flatten)]
    pub tls_config: Option<TlsConfig>,
    pub heartbeat_interval: u64, // in seconds
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: u64, // in seconds
    pub reset_on_logon: bool,
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, TlsConfig};

    #[test]
    fn test_simple_config() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
data_dictionary_path = "./spec/FIX44.xml"

connection_port = 443
connection_host = "127.0.0.1"
ca_certificate_path = "my_cert.crt"
heartbeat_interval = 30
reset_on_logon = false
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);

        let session_config = config.sessions.get(0).unwrap();
        assert_eq!(session_config.begin_string, "FIX.4.4");
        assert_eq!(session_config.sender_comp_id, "send-comp-id");
        assert_eq!(session_config.target_comp_id, "target-comp-id");
        assert_eq!(session_config.data_dictionary_path, "./spec/FIX44.xml");
        assert_eq!(session_config.connection_port, 443);
        assert_eq!(session_config.connection_host, "127.0.0.1");
        assert_eq!(session_config.heartbeat_interval, 30);
        let expected_tls_config = TlsConfig {
            ca_certificate_path: "my_cert.crt".to_string(),
        };
        assert_eq!(session_config.tls_config, Some(expected_tls_config));
        assert_eq!(session_config.reconnect_interval, 30);
    }
}
