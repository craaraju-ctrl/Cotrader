//! Asset Classes — Configuration for different market types.

#[derive(Debug, Clone)]
pub enum AssetClass {
    Equity,
    FAndO,
    Crypto,
    Commodity,
    Forex,
}

impl AssetClass {
    pub fn from_symbol(symbol: &str) -> Self {
        let upper = symbol.to_uppercase();
        if upper.ends_with("USDT") || upper.ends_with("USD") {
            AssetClass::Crypto
        } else if upper.contains("NIFTY") || upper.contains("BANKNIFTY") {
            AssetClass::FAndO
        } else if ["GOLD", "SILVER", "CRUDE", "COPPER"].iter().any(|&c| upper.contains(c)) {
            AssetClass::Commodity
        } else if upper.len() == 6 && upper.chars().all(|c| c.is_ascii_alphabetic()) {
            AssetClass::Forex
        } else {
            AssetClass::Equity
        }
    }

    pub fn trading_hours(&self) -> (u32, u32) {
        match self {
            AssetClass::Crypto => (0, 24),
            AssetClass::Equity => (9, 15),
            AssetClass::FAndO => (9, 15),
            AssetClass::Commodity => (9, 23),
            AssetClass::Forex => (0, 24),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetConfig {
    pub asset_class: AssetClass,
    pub tick_size: f64,
    pub lot_size: f64,
    pub max_leverage: f64,
    pub margin_requirement: f64,
}

impl AssetConfig {
    pub fn for_class(class: &AssetClass) -> Self {
        match class {
            AssetClass::Crypto => Self {
                asset_class: class.clone(),
                tick_size: 0.01,
                lot_size: 0.001,
                max_leverage: 20.0,
                margin_requirement: 0.05,
            },
            AssetClass::Equity => Self {
                asset_class: class.clone(),
                tick_size: 0.05,
                lot_size: 1.0,
                max_leverage: 5.0,
                margin_requirement: 0.20,
            },
            AssetClass::FAndO => Self {
                asset_class: class.clone(),
                tick_size: 0.05,
                lot_size: 1.0,
                max_leverage: 10.0,
                margin_requirement: 0.10,
            },
            _ => Self {
                asset_class: class.clone(),
                tick_size: 0.01,
                lot_size: 1.0,
                max_leverage: 10.0,
                margin_requirement: 0.10,
            },
        }
    }
}
