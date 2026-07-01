//! Health Checker — System health monitoring.

pub struct HealthChecker {
    pub pipeline_healthy: bool,
    pub memory_healthy: bool,
    pub broker_healthy: bool,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            pipeline_healthy: true,
            memory_healthy: false,
            broker_healthy: false,
        }
    }

    pub async fn check_all(&self) -> HealthStatus {
        HealthStatus {
            pipeline: self.pipeline_healthy,
            memory: self.memory_healthy,
            broker: self.broker_healthy,
            overall: self.pipeline_healthy && self.memory_healthy,
        }
    }

    pub fn update_memory(&mut self, healthy: bool) {
        self.memory_healthy = healthy;
    }

    pub fn update_broker(&mut self, healthy: bool) {
        self.broker_healthy = healthy;
    }
}

#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub pipeline: bool,
    pub memory: bool,
    pub broker: bool,
    pub overall: bool,
}
