use std::collections::HashMap;
use chrono::{DateTime, Duration, Utc};
use crate::types::{Candle, Trade};

fn round_to_interval(ts: DateTime<Utc>, interval_secs: i64) -> DateTime<Utc> {
    let timestamp = ts.timestamp();
    let remainder = timestamp % interval_secs;
    DateTime::from_timestamp(timestamp - remainder, 0).unwrap()
}

pub struct CandleStore {
    candles: HashMap<String, HashMap<String, Vec<Candle>>>,
}

impl CandleStore {
    pub fn new() -> Self {
        Self { candles: HashMap::new() }
    }

    fn interval_secs(interval: &str) -> i64 {
        match interval {
            "1m" => 60,
            "5m" => 300,
            "15m" => 900,
            "1h" => 3600,
            "4h" => 14400,
            "1d" => 86400,
            _ => 60,
        }
    }

    pub fn add_trade(&mut self, trade: &Trade) {
        let intervals = ["1m", "5m", "15m", "1h", "4h", "1d"];
        for interval in intervals {
            let secs = Self::interval_secs(interval);
            let open_time = round_to_interval(trade.timestamp, secs);
            let close_time = open_time + Duration::seconds(secs);

            let sym_candles = self.candles
                .entry(trade.symbol.clone())
                .or_default()
                .entry(interval.to_string())
                .or_default();

            let candle_price = trade.price;
            if let Some(last) = sym_candles.last_mut() {
                if last.open_time == open_time {
                    last.high = last.high.max(candle_price);
                    last.low = last.low.min(candle_price);
                    last.close = candle_price;
                    last.volume += trade.quantity;
                    last.trades += 1;
                    continue;
                }
            }

            sym_candles.push(Candle {
                symbol: trade.symbol.clone(),
                interval: interval.to_string(),
                open_time,
                close_time,
                open: candle_price,
                high: candle_price,
                low: candle_price,
                close: candle_price,
                volume: trade.quantity,
                trades: 1,
            });
        }
    }

    pub fn get_candles(&self, symbol: &str, interval: &str, limit: usize) -> Vec<Candle> {
        self.candles
            .get(symbol)
            .and_then(|syms| syms.get(interval))
            .map(|c| {
                let mut c = c.clone();
                c.reverse();
                c.truncate(limit);
                c.reverse();
                c
            })
            .unwrap_or_default()
    }

    pub fn get_all_candles(&self, symbol: &str, interval: &str) -> Vec<Candle> {
        self.candles
            .get(symbol)
            .and_then(|syms| syms.get(interval))
            .cloned()
            .unwrap_or_default()
    }
}
