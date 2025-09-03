use std::fmt::Display;

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

pub fn mb_to_bytes(mb: f32) -> usize {
    (mb * 1024.0 * 1024.0) as usize
}
