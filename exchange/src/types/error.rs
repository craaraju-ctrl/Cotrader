use thiserror::Error;

/// Binance-style error codes for pro-trader API responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Unknown = -1000,
    Disconnected = -1001,
    Unauthorized = -1002,
    TooManyRequests = -1003,
    ServerBusy = -1005,
    InvalidJson = -1006,
    InvalidSymbol = -1013,
    InvalidOrderType = -1015,
    InvalidSide = -1018,
    InvalidPrice = -1021,
    InvalidQuantity = -1022,
    InvalidTimeInForce = -1023,
    OrderNotFound = -2011,
    InsufficientBalance = -2015,
    LiquidityInsufficient = -2016,
    OrderWouldImmediatelyMatch = -2017,
    IcebergRequiresGtc = -2018,
    OcoOrderFailed = -2019,
    CancelOrderFailed = -2020,
    TrailingStopInvalid = -2021,
    PositionLimitExceeded = -2022,
    InternalError = -5000,
}

impl ErrorCode {
    pub fn code(&self) -> i32 { *self as i32 }
}

#[derive(Debug, Error)]
pub struct ExchangeError {
    pub code: ErrorCode,
    pub msg: String,
}

impl ExchangeError {
    pub fn new(code: ErrorCode, msg: impl Into<String>) -> Self {
        Self { code, msg: msg.into() }
    }

    pub fn to_http_status(&self) -> axum::http::StatusCode {
        match self.code {
            ErrorCode::OrderNotFound => axum::http::StatusCode::NOT_FOUND,
            ErrorCode::Unauthorized => axum::http::StatusCode::UNAUTHORIZED,
            ErrorCode::TooManyRequests | ErrorCode::ServerBusy => axum::http::StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::InsufficientBalance | ErrorCode::PositionLimitExceeded => axum::http::StatusCode::FORBIDDEN,
            _ => axum::http::StatusCode::BAD_REQUEST,
        }
    }

    pub fn binance_json(&self) -> serde_json::Value {
        serde_json::json!({
            "code": self.code.code(),
            "msg": self.msg,
        })
    }
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.code(), self.msg)
    }
}

// ── Convenience constructors ───────────────────────────────

pub fn err_order_not_found(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::OrderNotFound, msg)
}
pub fn err_invalid_order(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::InvalidOrderType, msg)
}
pub fn err_insufficient_balance(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::InsufficientBalance, msg)
}
pub fn err_invalid_price(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::InvalidPrice, msg)
}
pub fn err_invalid_quantity(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::InvalidQuantity, msg)
}
pub fn err_rate_limit(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::TooManyRequests, msg)
}
pub fn err_position_limit(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::PositionLimitExceeded, msg)
}
pub fn err_internal(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::InternalError, msg)
}
pub fn err_liquidity(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::LiquidityInsufficient, msg)
}
pub fn err_oco(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::OcoOrderFailed, msg)
}
pub fn err_iceberg(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::IcebergRequiresGtc, msg)
}
pub fn err_trailing_stop(msg: impl Into<String>) -> ExchangeError {
    ExchangeError::new(ErrorCode::TrailingStopInvalid, msg)
}

pub type ExchangeResult<T> = Result<T, ExchangeError>;
