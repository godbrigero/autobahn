use futures_util::{pin_mut, StreamExt};
use get_if_addrs::{get_if_addrs, IfAddr, Ifv4Addr};
use log::debug;
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
  collections::{HashMap, HashSet},
  fs,
  future::Future,
  sync::Arc,
  time::Duration,
};
use tokio::{runtime::Handle, sync::Mutex};

const SERVICE: &str = "_autobahn._udp.local.";

pub struct Discovery {
  mdns: ServiceDaemon,
  receiver: Receiver<ServiceEvent>,
  addresses_found: Mutex<HashMap<String, String>>,
  self_ip: String,
  self_port: u16,
}

impl Discovery {
  pub fn new(port: u16) -> Self {
    Self::new_with_ip(get_local_ip_system_dependent().unwrap(), port)
  }

  pub fn new_with_ip(ip: String, port: u16) -> Self {
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
    while let Ok(event) = self.clone().receiver.recv_async().await {
      match event {
        ServiceEvent::ServiceResolved(info) => {
          if let Some(addr) = info.get_addresses().iter().next() {
            if addr.to_string() == self.self_ip {
              continue;
            }

            let mut addresses_found = self.addresses_found.lock().await;
            addresses_found.insert(info.get_fullname().to_string(), addr.to_string());

            debug!(
              "Found: {} at {}:{}",
              info.get_fullname(),
              addr,
              info.get_port(),
            );

            drop(addresses_found);

            on_discovery(addr.to_string(), info.get_port() as u16).await;
          }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
          let mut addresses_found = self.addresses_found.lock().await;
          addresses_found.remove(&fullname.to_string());
          debug!("Lost: {}", fullname);
        }
        _ => {}
      }
    }
  }

  pub fn run_discovery_server(self: Arc<Self>) {
    let host_name = "autobahn.local.";
    let props = [("ip", self.self_ip.clone())];

    let info = ServiceInfo::new(
      SERVICE,
      "AutobahnDiscovery",
      host_name,
      self.self_ip.clone(),
      self.self_port as u16,
      props.as_slice(),
    )
    .unwrap();

    debug!("Registering discovery server with info: {:?}", info);

    self.mdns.register(info).unwrap();
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
