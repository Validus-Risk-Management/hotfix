//! Configuration for FIX sessions.
//!
//! Refer to [Config] for the supported configuration options.
//! [Config] objects can be constructed manually or by creating a `toml`
//! config file. See the
//! [example project's config file](https://github.com/Validus-Risk-Management/hotfix/blob/main/examples/simple-new-order/config/test-config.toml)
//! for more detail.
use chrono::{NaiveTime, Weekday};
use chrono_tz::Tz;
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

/// TLS encryption details with configurable trust store.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "trust_store", rename_all = "snake_case")]
pub enum TlsConfig {
    /// Use a custom CA certificate file (PEM format).
    File {
        /// Path to the CA certificate file.
        ca_certificate_path: String,
    },
    /// Use the operating system's native certificate store.
    Native,
    /// Use Mozilla's bundled root certificates (via webpki-roots).
    Webpki,
}

/// Session schedule configuration
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ScheduleConfig {
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub start_day: Option<Weekday>,
    pub end_day: Option<Weekday>,
    #[serde(default)]
    pub weekdays: Vec<Weekday>,
    pub timezone: Option<Tz>,
}

fn default_reconnect_interval() -> u64 {
    30
}

fn default_logon_timeout() -> u64 {
    10
}

fn default_logout_timeout() -> u64 {
    2
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

    /// The time we wait in seconds for Logon responses before timing out.
    #[serde(default = "default_logon_timeout")]
    pub logon_timeout: u64,

    /// The time we wait in seconds for Logon responses before timing out.
    #[serde(default = "default_logout_timeout")]
    pub logout_timeout: u64,

    /// The interval we should attempt to reconnect at in seconds.
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: u64,

    /// Specifies whether we should reset the state of the message store on logon.
    #[serde(default)]
    pub reset_on_logon: bool,

    /// The schedule configuration for the session
    pub schedule: Option<ScheduleConfig>,
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, TlsConfig};
    use chrono::{NaiveTime, Weekday};

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
trust_store = "file"
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
        let expected_tls_config = TlsConfig::File {
            ca_certificate_path: "my_cert.crt".to_string(),
        };
        assert_eq!(session_config.tls_config, Some(expected_tls_config));
        assert_eq!(session_config.reconnect_interval, 30);
        assert_eq!(session_config.logon_timeout, 10);
    }

    #[test]
    fn test_tls_config_native() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
connection_port = 443
connection_host = "127.0.0.1"
heartbeat_interval = 30
trust_store = "native"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.tls_config, Some(TlsConfig::Native));
    }

    #[test]
    fn test_tls_config_webpki() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
connection_port = 443
connection_host = "127.0.0.1"
heartbeat_interval = 30
trust_store = "webpki"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.tls_config, Some(TlsConfig::Webpki));
    }

    #[test]
    fn test_no_tls_config() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
connection_port = 9880
connection_host = "127.0.0.1"
heartbeat_interval = 30
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.tls_config, None);
    }

    #[test]
    fn test_schedule_config_weekdays() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
heartbeat_interval = 30

connection_port = 443
connection_host = "127.0.0.1"

[sessions.schedule]
start_time = "00:00:00"
end_time = "23:55:00"
weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);
        let session = config.sessions.first().unwrap();

        assert!(session.schedule.is_some());
        let schedule = session.schedule.as_ref().unwrap();

        assert_eq!(schedule.start_time, NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(schedule.end_time, NaiveTime::from_hms_opt(23, 55, 0));
        assert_eq!(
            schedule.weekdays,
            vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri
            ]
        );
        assert_eq!(schedule.start_day, None);
        assert_eq!(schedule.end_day, None);
    }

    #[test]
    fn test_schedule_config_weeklong_session() {
        let config_contents = r#"
[[sessions]]
begin_string = "FIX.4.4"
sender_comp_id = "send-comp-id"
target_comp_id = "target-comp-id"
heartbeat_interval = 30

connection_port = 443
connection_host = "127.0.0.1"

[sessions.schedule]
start_time = "00:00:00"
end_time = "23:55:00"
start_day = "Monday"
end_day = "Friday"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);
        let session = config.sessions.first().unwrap();

        assert!(session.schedule.is_some());
        let schedule = session.schedule.as_ref().unwrap();

        assert_eq!(schedule.start_time, NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(schedule.end_time, NaiveTime::from_hms_opt(23, 55, 0));
        assert_eq!(schedule.start_day, Some(Weekday::Mon));
        assert_eq!(schedule.end_day, Some(Weekday::Fri));
    }

    #[test]
    fn test_schedule_config_with_new_york_timezone() {
        use chrono_tz::Tz;

        let config_contents = r#"
    [[sessions]]
    begin_string = "FIX.4.4"
    sender_comp_id = "send-comp-id"
    target_comp_id = "target-comp-id"
    heartbeat_interval = 30

    connection_port = 443
    connection_host = "127.0.0.1"

    [sessions.schedule]
    start_time = "08:00:00"
    end_time = "16:30:00"
    weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
    timezone = "America/New_York"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);
        let session = config.sessions.first().unwrap();

        assert!(session.schedule.is_some());
        let schedule = session.schedule.as_ref().unwrap();

        assert_eq!(schedule.start_time, NaiveTime::from_hms_opt(8, 0, 0));
        assert_eq!(schedule.end_time, NaiveTime::from_hms_opt(16, 30, 0));
        assert_eq!(schedule.timezone, Some(Tz::America__New_York));
    }

    #[test]
    fn test_schedule_config_with_utc_timezone() {
        use chrono_tz::Tz;

        let config_contents = r#"
    [[sessions]]
    begin_string = "FIX.4.4"
    sender_comp_id = "send-comp-id"
    target_comp_id = "target-comp-id"
    heartbeat_interval = 30

    connection_port = 443
    connection_host = "127.0.0.1"

    [sessions.schedule]
    start_time = "00:00:00"
    end_time = "23:55:00"
    weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
    timezone = "UTC"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        let session = config.sessions.first().unwrap();
        let schedule = session.schedule.as_ref().unwrap();

        assert_eq!(schedule.start_time, NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(schedule.end_time, NaiveTime::from_hms_opt(23, 55, 0));
        assert_eq!(schedule.timezone, Some(Tz::UTC));
    }

    #[test]
    fn test_schedule_config_with_london_timezone() {
        use chrono_tz::Tz;

        let config_contents = r#"
    [[sessions]]
    begin_string = "FIX.4.4"
    sender_comp_id = "send-comp-id"
    target_comp_id = "target-comp-id"
    heartbeat_interval = 30

    connection_port = 443
    connection_host = "127.0.0.1"

    [sessions.schedule]
    start_time = "09:30:00"
    end_time = "17:00:00"
    weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
    timezone = "Europe/London"
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        let session = config.sessions.first().unwrap();
        let schedule = session.schedule.as_ref().unwrap();

        assert_eq!(schedule.start_time, NaiveTime::from_hms_opt(9, 30, 0));
        assert_eq!(schedule.end_time, NaiveTime::from_hms_opt(17, 0, 0));
        assert_eq!(schedule.timezone, Some(Tz::Europe__London));
    }

    #[test]
    fn test_logon_timeout_config() {
        let config_contents = r#"
    [[sessions]]
    begin_string = "FIX.4.4"
    sender_comp_id = "send-comp-id"
    target_comp_id = "target-comp-id"
    data_dictionary_path = "./spec/FIX44.xml"

    connection_port = 443
    connection_host = "127.0.0.1"
    trust_store = "file"
    ca_certificate_path = "my_cert.crt"
    heartbeat_interval = 30
    logon_timeout = 20
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);

        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.logon_timeout, 20);
    }

    #[test]
    fn test_reconnect_interval_config() {
        let config_contents = r#"
    [[sessions]]
    begin_string = "FIX.4.4"
    sender_comp_id = "send-comp-id"
    target_comp_id = "target-comp-id"
    data_dictionary_path = "./spec/FIX44.xml"

    connection_port = 443
    connection_host = "127.0.0.1"
    trust_store = "file"
    ca_certificate_path = "my_cert.crt"
    heartbeat_interval = 30
    reconnect_interval = 15
        "#;

        let config: Config = toml::from_str(config_contents).unwrap();
        assert_eq!(config.sessions.len(), 1);

        let session_config = config.sessions.first().unwrap();
        assert_eq!(session_config.reconnect_interval, 15);
    }
}
