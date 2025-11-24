use std::{future::Future, sync::Arc, time};

use get_if_addrs::{get_if_addrs, IfAddr, Ifv4Addr};
use log::{debug, info, warn};
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::time::{interval, Duration};

const SERVICE: &str = "_autobahn._udp.local.";
const REBROADCAST_INTERVAL: Duration = Duration::from_secs(2);

pub struct Discovery {
  mdns: ServiceDaemon,
  receiver: Receiver<ServiceEvent>,
  self_ip: String,
  self_port: u16,
}

impl Discovery {
  pub fn new(port: u16) -> Arc<Self> {
    let ip = get_local_ip_system_dependent().unwrap_or_else(|| {
      warn!("Failed to get system-dependent IP, falling back to first non-loopback IP");
      get_first_non_loopback_ip().expect("Failed to get any valid IP address")
    });

    info!(
      "Initializing Discovery service with IP: {}, port: {}",
      ip, port
    );

    Self::new_with_ip(ip, port)
  }

  pub fn new_with_ip(ip: String, port: u16) -> Arc<Self> {
    info!(
      "Creating new Discovery instance with IP: {}, port: {}",
      ip, port
    );

    let mdns = ServiceDaemon::new().unwrap();
    let receiver = mdns.browse(SERVICE).unwrap();

    Arc::new(Self {
      mdns,
      receiver,
      self_ip: ip,
      self_port: port,
    })
  }

  pub async fn start_discovery_loop<F, Fut>(self: Arc<Self>, on_discovery: F)
  where
    F: Fn(String, u16) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
  {
    info!("Starting discovery loop");
    while let Ok(event) = self.clone().receiver.recv_async().await {
      match event {
        ServiceEvent::ServiceResolved(info) => {
          debug!("Resolved service: {:?}", info);
          let address = info.get_properties().get("ip");
          let port = info.get_properties().get("port");
          if let (Some(addr), Some(port)) = (address, port) {
            let addr = String::from_utf8(addr.val().unwrap().to_vec()).unwrap();
            let port = String::from_utf8(port.val().unwrap().to_vec())
              .unwrap()
              .parse::<u16>()
              .unwrap();

            if addr == self.self_ip {
              info!("Skipping own service at {}:{}", addr, port);
              continue;
            }

            on_discovery(addr, port).await;
          }
        }
        _ => {
          debug!("Received other mDNS event: {:?}", event);
        }
      }
    }
  }

  fn create_service_info(&self) -> ServiceInfo {
    let host_name = "autobahn.local.";
    let props = [
      ("ip", self.self_ip.clone()),
      ("port", self.self_port.to_string()),
    ];

    let info = ServiceInfo::new(
      SERVICE,
      "AutobahnDiscovery",
      host_name,
      self.self_ip.clone(),
      self.self_port as u16,
      props.as_slice(),
    )
    .unwrap();

    debug!("Created service info: {:?}", info);
    info
  }

  pub fn run_discovery_server(self: Arc<Self>) {
    let info = self.create_service_info();
    info!("Registering discovery server with info: {:?}", info);
    match self.mdns.register(info) {
      Ok(_) => info!("Successfully registered mDNS service"),
      Err(e) => warn!("Failed to register mDNS service: {}", e),
    }
  }

  pub async fn run_discovery_server_continuous(self: Arc<Self>) {
    info!("Starting continuous discovery server");
    let mut interval = interval(REBROADCAST_INTERVAL);
    loop {
      let info = self.create_service_info();
      debug!("Re-registering discovery server");

      match self.mdns.register(info) {
        Ok(_) => debug!("Successfully re-registered mDNS service"),
        Err(e) => warn!("Failed to re-register mDNS service: {}", e),
      }

      interval.tick().await;
    }
  }
}

fn get_local_ip_system_dependent() -> Option<String> {
  if cfg!(target_os = "linux") {
    get_local_ip("eth0")
  } else if cfg!(target_os = "macos") {
    get_local_ip("eth0").or_else(|| get_local_ip("en0"))
  } else if cfg!(target_os = "windows") {
    get_local_ip("Ethernet").or_else(|| get_local_ip("Wi-Fi"))
  } else {
    None
  }
}

fn get_first_non_loopback_ip() -> Option<String> {
  let ifaces = get_if_addrs().ok()?;
  for iface in ifaces {
    if let IfAddr::V4(Ifv4Addr { ip, .. }) = iface.addr {
      if !ip.is_loopback() && !ip.is_unspecified() {
        return Some(ip.to_string());
      }
    }
  }
  None
}

fn get_local_ip(iface_name: &str) -> Option<String> {
  let ifaces = get_if_addrs().ok()?;
  for iface in ifaces {
    if iface.name == iface_name {
      if let IfAddr::V4(Ifv4Addr { ip, .. }) = iface.addr {
        return Some(ip.to_string());
      }
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_local_ip_system_dependent() {
    let ip = get_local_ip_system_dependent().unwrap();
    println!("IP: {}", ip);
  }
}
