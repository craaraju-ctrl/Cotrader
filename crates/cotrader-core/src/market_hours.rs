//! Market Hours — Multi-timezone trading session support.
//!
//! Different markets have different trading hours:
//! - Crypto: 24/7
//! - NSE: 9:15-15:30 IST
//! - NYSE: 9:30-16:00 ET
//! - TSE: 9:00-15:00 JST (with lunch break)
//! - LSE: 8:00-16:30 GMT

use chrono::{Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// Trading session configuration for an exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSession {
    pub exchange: String,
    pub timezone: String,
    pub open_time: NaiveTime,
    pub close_time: NaiveTime,
    pub lunch_start: Option<NaiveTime>,
    pub lunch_end: Option<NaiveTime>,
    pub days_of_week: Vec<Weekday>,
    pub is_24_7: bool,
}

impl MarketSession {
    /// Check if the market is currently open.
    pub fn is_open(&self) -> bool {
        if self.is_24_7 {
            return true;
        }

        let now = Utc::now();
        let local_time = now.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()); // UTC for now

        // Check day of week
        if !self.days_of_week.contains(&local_time.weekday()) {
            return false;
        }

        // Check time
        let current_time = local_time.time();
        if current_time < self.open_time || current_time > self.close_time {
            return false;
        }

        // Check lunch break
        if let (Some(lunch_start), Some(lunch_end)) = (self.lunch_start, self.lunch_end) {
            if current_time >= lunch_start && current_time <= lunch_end {
                return false;
            }
        }

        true
    }

    /// Get time until market opens (in seconds).
    pub fn seconds_until_open(&self) -> i64 {
        if self.is_24_7 {
            return 0;
        }
        // Simplified: return 0 if open, 3600 if closed
        if self.is_open() { 0 } else { 3600 }
    }
}

/// Get default market sessions for major exchanges.
pub fn default_sessions() -> Vec<MarketSession> {
    vec![
        // Crypto — 24/7
        MarketSession {
            exchange: "BINANCE".to_string(),
            timezone: "UTC".to_string(),
            open_time: NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
            lunch_start: None,
            lunch_end: None,
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun,
            ],
            is_24_7: true,
        },
        // NSE (India) — 9:15-15:30 IST
        MarketSession {
            exchange: "NSE".to_string(),
            timezone: "Asia/Kolkata".to_string(),
            open_time: NaiveTime::from_hms_opt(9, 15, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(15, 30, 0).unwrap(),
            lunch_start: None,
            lunch_end: None,
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri,
            ],
            is_24_7: false,
        },
        // NYSE (US) — 9:30-16:00 ET
        MarketSession {
            exchange: "NYSE".to_string(),
            timezone: "America/New_York".to_string(),
            open_time: NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            lunch_start: None,
            lunch_end: None,
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri,
            ],
            is_24_7: false,
        },
        // TSE (Japan) — 9:00-15:00 JST with lunch
        MarketSession {
            exchange: "TSE".to_string(),
            timezone: "Asia/Tokyo".to_string(),
            open_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(15, 0, 0).unwrap(),
            lunch_start: Some(NaiveTime::from_hms_opt(11, 30, 0).unwrap()),
            lunch_end: Some(NaiveTime::from_hms_opt(12, 30, 0).unwrap()),
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri,
            ],
            is_24_7: false,
        },
        // LSE (UK) — 8:00-16:30 GMT
        MarketSession {
            exchange: "LSE".to_string(),
            timezone: "Europe/London".to_string(),
            open_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(16, 30, 0).unwrap(),
            lunch_start: None,
            lunch_end: None,
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri,
            ],
            is_24_7: false,
        },
        // Forex — 24/5 (Sun 17:00 ET to Fri 17:00 ET)
        MarketSession {
            exchange: "FOREX".to_string(),
            timezone: "UTC".to_string(),
            open_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            close_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            lunch_start: None,
            lunch_end: None,
            days_of_week: vec![
                Weekday::Mon, Weekday::Tue, Weekday::Wed,
                Weekday::Thu, Weekday::Fri,
            ],
            is_24_7: true, // Simplified: treat as 24/5
        },
    ]
}
