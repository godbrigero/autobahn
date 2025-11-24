use std::fs;

use crate::util::Address;

#[derive(serde::Deserialize, Clone)]
pub enum LogLevel {
  Debug,
  Info,
  Warn,
  Error,
  Off,
}

impl Default for LogLevel {
  fn default() -> Self {
    LogLevel::Off
  }
}

impl LogLevel {
  pub fn to_str(&self) -> &str {
    match self {
      LogLevel::Debug => "debug",
      LogLevel::Info => "info",
      LogLevel::Warn => "warn",
      LogLevel::Error => "error",
      LogLevel::Off => "off",
    }
  }
}

impl PartialEq for LogLevel {
  fn eq(&self, other: &Self) -> bool {
    self.to_str() == other.to_str()
  }
}

impl std::fmt::Debug for LogLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.to_str())
  }
}

#[derive(serde::Deserialize, Clone)]
pub struct Config {
  pub log_level: Option<LogLevel>,
  pub self_addr: Address,
  pub others: Option<Vec<Address>>,
}

impl Config {
  pub fn from_args(args: Vec<String>) -> Self {
    let config_path = if args.len() >= 3 && args[1] == "--config" {
      &args[2]
    } else {
      "config.toml"
    };

    return toml::from_str(
      &fs::read_to_string(config_path).expect(&format!("Failed to read {}", config_path)),
    )
    .expect(&format!("Failed to parse {}", config_path));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_config_from_args() {
    let args = vec![
      "autobahn".to_string(),
      "--config".to_string(),
      "test_config.toml".to_string(),
    ];
    let config = Config::from_args(args);
    assert_eq!(config.log_level.unwrap(), LogLevel::Debug);
    assert_eq!(config.self_addr, Address::from_str("0.0.0.0:8080").unwrap());
    assert_eq!(
      config.others,
      Some(vec![
        Address::from_str("127.0.0.1:8081").unwrap(),
        Address::from_str("127.0.0.1:8082").unwrap()
      ])
    );
  }
}
