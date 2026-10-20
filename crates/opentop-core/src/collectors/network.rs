use std::time::{Duration, Instant};

use sysinfo::Networks;

use super::{Collector, CollectorUpdate};
use crate::snapshot::NetworkInterface;

/// Per-interface network throughput and totals, using sysinfo.
pub struct NetworkCollector {
    networks: Networks,
    updated_at: Instant,
}

impl NetworkCollector {
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            updated_at: Instant::now(),
        }
    }
}

impl Collector for NetworkCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        if self.updated_at.elapsed() < Duration::from_millis(250) {
            // First tick: give deltas a meaningful window before dividing.
            std::thread::sleep(Duration::from_millis(400));
        }
        self.networks.refresh(true);
        let elapsed = self.updated_at.elapsed().as_secs_f64().max(0.1);
        self.updated_at = Instant::now();

        let mut interfaces: Vec<NetworkInterface> = self
            .networks
            .list()
            .iter()
            .map(|(name, data)| {
                let rx_rate = data.received() as f64 / elapsed;
                let tx_rate = data.transmitted() as f64 / elapsed;
                NetworkInterface {
                    name: name.clone(),
                    mac: data.mac_address().to_string(),
                    ips: data.ip_networks().iter().map(|ip| ip.to_string()).collect(),
                    state: format!("{:?}", data.operational_state()).to_lowercase(),
                    mtu: data.mtu(),
                    rx_bytes_per_sec: rx_rate,
                    tx_bytes_per_sec: tx_rate,
                    total_rx_bytes: data.total_received(),
                    total_tx_bytes: data.total_transmitted(),
                    total_rx_packets: data.total_packets_received(),
                    total_tx_packets: data.total_packets_transmitted(),
                    rx_errors: data.total_errors_on_received(),
                    tx_errors: data.total_errors_on_transmitted(),
                }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        Some(CollectorUpdate::Network(interfaces))
    }
}

impl Default for NetworkCollector {
    fn default() -> Self {
        Self::new()
    }
}
