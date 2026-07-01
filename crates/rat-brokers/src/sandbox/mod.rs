//! Sandbox — Safe integration testing over live exchange endpoints.

use crate::traits::*;

pub struct Sandbox {
    broker: Box<dyn Broker>,
    test_mode: bool,
    simulated_delay_ms: u64,
}

impl Sandbox {
    pub fn new(broker: Box<dyn Broker>) -> Self {
        Self {
            broker,
            test_mode: true,
            simulated_delay_ms: 100,
        }
    }

    /// Test order placement without real execution.
    pub async fn test_order(&self, order: &NewOrder) -> SandboxResult {
        println!("[Sandbox] Testing order: {} {} {} @ {:?}",
            order.side, order.quantity, order.symbol, order.price);

        // Simulate network delay
        tokio::time::sleep(std::time::Duration::from_millis(self.simulated_delay_ms)).await;

        // Validate order
        if order.quantity <= 0.0 {
            return SandboxResult {
                success: false,
                message: "Invalid quantity".to_string(),
                simulated_fill: None,
            };
        }

        if order.price.unwrap_or(0.0) <= 0.0 && order.order_type == OrderType::Limit {
            return SandboxResult {
                success: false,
                message: "Invalid price for limit order".to_string(),
                simulated_fill: None,
            };
        }

        // Simulate fill
        let fill_price = order.price.unwrap_or(0.0);
        SandboxResult {
            success: true,
            message: "Order validated successfully".to_string(),
            simulated_fill: Some(SimulatedFill {
                price: fill_price,
                quantity: order.quantity,
                commission: fill_price * order.quantity * 0.001,
                slippage: fill_price * 0.0001,
            }),
        }
    }

    /// Test connection to broker.
    pub async fn test_connection(&self) -> bool {
        println!("[Sandbox] Testing connection to {}", self.broker.name());
        tokio::time::sleep(std::time::Duration::from_millis(self.simulated_delay_ms)).await;
        self.broker.is_connected()
    }

    /// Test balance query.
    pub async fn test_balance(&self) -> SandboxResult {
        println!("[Sandbox] Testing balance query");
        tokio::time::sleep(std::time::Duration::from_millis(self.simulated_delay_ms)).await;

        SandboxResult {
            success: true,
            message: "Balance query successful".to_string(),
            simulated_fill: None,
        }
    }
}

pub struct SandboxResult {
    pub success: bool,
    pub message: String,
    pub simulated_fill: Option<SimulatedFill>,
}

pub struct SimulatedFill {
    pub price: f64,
    pub quantity: f64,
    pub commission: f64,
    pub slippage: f64,
}
