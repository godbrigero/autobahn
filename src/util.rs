use std::fmt::Display;

pub mod proto;
pub mod ws;

#[derive(serde::Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Address {
  pub host: String,
  pub port: i32,
}

impl Display for Address {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}:{}", self.host, self.port)
  }
}

impl Address {
  pub fn new(host: String, port: i32) -> Self {
    Self { host, port }
  }

  pub fn build_ws_url(&self) -> String {
    return format!("ws://{}:{}", self.host, self.port);
  }

  pub fn from_str(s: &str) -> Result<Address, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
      return Err("Invalid address format".to_string());
    }
    Ok(Address {
      host: parts[0].to_string(),
      port: parts[1].parse().map_err(|_| "Invalid port number")?,
    })
  }

  pub fn to_string(&self) -> String {
    return format!("{}:{}", self.host, self.port);
  }
}

#[cfg(test)]
mod tests {
  use crate::util::Address;

  fn generate_random_bytes_number(num: usize) -> Vec<u8> {
    (0..num).map(|_| rand::random::<u8>()).collect()
  }

  #[test]
  fn test_address_from_str() {
    let addr = Address::from_str("127.0.0.1:8080").unwrap();
    assert_eq!(addr.host, "127.0.0.1");
    assert_eq!(addr.port, 8080);
  }
}
