//! Configuration for FIX sessions.
//!
//! Refer to [Config] for the supported configuration options.
//! [Config] objects can be constructed manually or by creating a `toml`
//! config file. See the
//! [example project's config file](https://github.com/Validus-Risk-Management/hotfix/blob/main/examples/simple-new-order/config/test-config.toml)
//! for more detail.
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// The configuration for multiple sessions.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub sessions: Vec<SessionConfig>,
}

impl Config {
    /// Load a [Config] from a `toml` file.
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Self {
        let config_str = fs::read_to_string(path).expect("to be able to load config");
        toml::from_str::<Self>(&config_str).expect("to be able to parse config")
    }
}

/// TLS encryption details.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TlsConfig {
    /// The path to the CA certificate.
    pub ca_certificate_path: String,
}

fn default_reconnect_interval() -> u64 {
    30
}

/// The configuration of a single FIX session.
#[derive(Clone, Debug, Deserialize)]
pub struct SessionConfig {
    /// The begin string specifying the FIX version.
    pub begin_string: String,
    /// The sender's comp ID.
    pub sender_comp_id: String,
    /// The target's comp ID.
    pub target_comp_id: String,
    /// The path to the data dictionary to use.
    pub data_dictionary_path: Option<String>,
    /// The host to connect to.
    ///
    /// This can be any representation of a host that can be interpreted
    /// as a host object.
    pub connection_host: String,
    /// The port to use to connect.
    pub connection_port: u16,
    /// The TLS configuration for the session, if TLS is used.
    #[serde(flatten)]
    pub tls_config: Option<TlsConfig>,
    /// The heartbeat interval to agree on with the peer in seconds.
    pub heartbeat_interval: u64,
    #[serde(default = "default_reconnect_interval")]
    /// The interval we should attempt to reconnect at in seconds.
    pub reconnect_interval: u64,
    /// Specifies whether we should reset the state of the message store on logon.
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

        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.begin_string, "FIX.4.4");
        assert_eq!(session_config.sender_comp_id, "send-comp-id");
        assert_eq!(session_config.target_comp_id, "target-comp-id");
        assert_eq!(
            session_config.data_dictionary_path,
            Some("./spec/FIX44.xml".to_string())
        );
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
