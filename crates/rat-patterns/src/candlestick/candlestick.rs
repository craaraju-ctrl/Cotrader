pub struct CandlestickDetector;

impl CandlestickDetector {
    pub fn name() -> &'static str { "CandlestickDetector" }

    pub fn detect(&self, data: &[f64]) -> Vec<String> {
        if data.len() < 15 {
            return vec!["Insufficient data for candlestick detection (need 15+ bars)".into()];
        }
        let mut patterns = Vec::new();
        let len = data.len();

        for i in 2..len {
            let o = data[i * 5];
            let h = data[i * 5 + 1];
            let l = data[i * 5 + 2];
            let c = data[i * 5 + 3];
            let body = (c - o).abs();
            let range = h - l;
            if range == 0.0 { continue; }
            let body_ratio = body / range;
            let upper_wick = h - o.max(c);
            let lower_wick = o.min(c) - l;

            // Doji
            if body_ratio < 0.1 && range > 0.0 {
                patterns.push(format!("Doji at bar {} — indecision", i));
            }
            // Hammer (bullish reversal at bottom)
            if lower_wick > body * 2.0 && upper_wick < body * 0.5 && body_ratio < 0.4 {
                patterns.push(format!("Hammer at bar {} — bullish reversal signal", i));
            }
            // Shooting Star (bearish reversal at top)
            if upper_wick > body * 2.0 && lower_wick < body * 0.5 && body_ratio < 0.4 {
                patterns.push(format!("Shooting Star at bar {} — bearish reversal signal", i));
            }
            // Marubozu
            if body_ratio > 0.9 && range > 0.0 {
                let dir = if c > o { "Bullish" } else { "Bearish" };
                patterns.push(format!("{} Marubozu at bar {} — strong conviction", dir, i));
            }
            // Engulfing
            if i >= 1 {
                let prev_o = data[(i - 1) * 5];
                let prev_c = data[(i - 1) * 5 + 3];
                if prev_c < prev_o && c > o && o < prev_c && c > prev_o {
                    patterns.push(format!("Bullish Engulfing at bar {} — reversal", i));
                }
                if prev_c > prev_o && c < o && o > prev_c && c < prev_o {
                    patterns.push(format!("Bearish Engulfing at bar {} — reversal", i));
                }
            }
        }
        if patterns.is_empty() {
            vec!["No candlestick patterns detected".into()]
        } else {
            patterns
        }
    }
}
