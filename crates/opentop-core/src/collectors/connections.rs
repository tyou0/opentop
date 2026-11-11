use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState, get_sockets_info};

use super::{Collector, CollectorUpdate, sleep_until};
use crate::snapshot::{Availability, ConnectionsSummary};

/// Aggregate TCP/UDP socket counts, using netstat2.
///
/// Enumerating thousands of sockets can take a noticeable moment, so the scan
/// runs on an internal worker thread and [`Collector::collect`] only picks up
/// the latest summary.
pub struct ConnectionsCollector {
    rx: Receiver<Availability<ConnectionsSummary>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    latest: Availability<ConnectionsSummary>,
    dirty: bool,
}

impl ConnectionsCollector {
    pub fn new(interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("opentop-connections-worker".into())
            .spawn(move || connections_worker(interval, tx, worker_stop))
            .expect("failed to spawn connections worker thread");
        Self {
            rx,
            stop,
            worker: Some(worker),
            latest: Availability::unavailable("scanning sockets…"),
            dirty: true,
        }
    }
}

impl Drop for ConnectionsCollector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn connections_worker(
    interval: Duration,
    tx: Sender<Availability<ConnectionsSummary>>,
    stop: Arc<AtomicBool>,
) {
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let started = Instant::now();
        if tx.send(scan_sockets()).is_err() {
            return;
        }
        sleep_until(started + interval, &stop);
    }
}

fn scan_sockets() -> Availability<ConnectionsSummary> {
    let flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let protocols = ProtocolFlags::TCP | ProtocolFlags::UDP;
    let sockets = match get_sockets_info(flags, protocols) {
        Ok(sockets) => sockets,
        Err(err) => {
            return Availability::unavailable(format!("socket query failed: {err}"));
        }
    };

    let mut summary = ConnectionsSummary {
        total: sockets.len(),
        ..Default::default()
    };
    let mut pids: HashSet<u32> = HashSet::new();

    for socket in &sockets {
        for pid in &socket.associated_pids {
            pids.insert(*pid);
        }
        match &socket.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp) => {
                summary.tcp += 1;
                match tcp.state {
                    TcpState::Established => summary.tcp_established += 1,
                    TcpState::Listen => summary.tcp_listen += 1,
                    TcpState::TimeWait => summary.tcp_wait += 1,
                    _ => summary.tcp_wait += 1,
                }
            }
            ProtocolSocketInfo::Udp(_) => summary.udp += 1,
        }
    }
    summary.processes_with_sockets = pids.len();
    Availability::available(summary)
}

impl Collector for ConnectionsCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        while let Ok(result) = self.rx.try_recv() {
            self.latest = result;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            Some(CollectorUpdate::Connections(self.latest.clone()))
        } else {
            None
        }
    }
}
