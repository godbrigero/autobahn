use futures_util::{pin_mut, StreamExt};
use get_if_addrs::{get_if_addrs, IfAddr, Ifv4Addr};
use log::{debug, info, warn};
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
  collections::{HashMap, HashSet},
  fs,
  future::Future,
  sync::Arc,
  time::Duration,
};
use tokio::{runtime::Handle, sync::Mutex, time};

const SERVICE: &str = "_autobahn._udp.local.";
const REBROADCAST_INTERVAL: Duration = Duration::from_secs(2);

pub struct Discovery {
  mdns: ServiceDaemon,
  receiver: Receiver<ServiceEvent>,
  addresses_found: Mutex<HashMap<String, String>>,
  self_ip: String,
  self_port: u16,
}

impl Discovery {
  pub fn new(port: u16) -> Self {
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

  pub fn new_with_ip(ip: String, port: u16) -> Self {
    info!(
      "Creating new Discovery instance with IP: {}, port: {}",
      ip, port
    );
    let mdns = ServiceDaemon::new().unwrap();
    let receiver = mdns.browse(SERVICE).unwrap();

    Self {
      mdns,
      receiver,
      addresses_found: Mutex::new(HashMap::new()),
      self_ip: ip,
      self_port: port,
    }
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
          if let Some(addr) = info.get_addresses().iter().next() {
            if addr.to_string() == self.self_ip {
              debug!("Skipping own service at {}", addr);
              continue;
            }

            let mut addresses_found = self.addresses_found.lock().await;
            if addresses_found.contains_key(&info.get_fullname().to_string()) {
              continue;
            }

            addresses_found.insert(info.get_fullname().to_string(), addr.to_string());

            info!(
              "Found new service: {} at {}:{} with properties: {:?}",
              info.get_fullname(),
              addr,
              info.get_port(),
              info.get_properties(),
            );

            drop(addresses_found);

            on_discovery(addr.to_string(), info.get_port() as u16).await;
          }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
          let mut addresses_found = self.addresses_found.lock().await;
          if let Some(addr) = addresses_found.remove(&fullname.to_string()) {
            info!("Service removed: {} at {}", fullname, addr);
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
    let mut interval = time::interval(REBROADCAST_INTERVAL);
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
