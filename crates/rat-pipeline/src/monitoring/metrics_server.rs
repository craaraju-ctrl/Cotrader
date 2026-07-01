//! Metrics Server — HTTP endpoint for metrics.

pub struct MetricsServer {
    port: u16,
}

impl MetricsServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(&self) {
        println!("[Metrics] Server starting on port {}", self.port);
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}
