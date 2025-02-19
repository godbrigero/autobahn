use std::fs;

use crate::util::Address;

#[derive(serde::Deserialize, Clone)]
pub struct Config {
  pub debug: bool,
  pub self_addr: Address,
  pub others: Vec<Address>,
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
