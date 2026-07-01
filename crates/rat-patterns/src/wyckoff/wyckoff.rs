pub struct WyckoffDetector;

impl WyckoffDetector {
    pub fn name() -> &'static str { "WyckoffDetector" }

    pub fn detect(&self, data: &[f64]) -> Vec<String> {
        if data.len() < 50 {
            return vec!["Insufficient data for Wyckoff analysis (need 50+ bars)".into()];
        }
        let closes: Vec<f64> = data.iter().skip(3).step_by(5).cloned().collect();
        let volumes: Vec<f64> = data.iter().skip(4).step_by(5).cloned().collect();
        let len = closes.len();
        let mut patterns = Vec::new();

        let avg_vol: f64 = volumes.iter().sum::<f64>() / volumes.len() as f64;
        let recent_vol: f64 = volumes.iter().rev().take(10).sum::<f64>() / 10.0;
        let recent_prices = &closes[len - 20..];
        let min_price = recent_prices.iter().cloned().fold(f64::MAX, f64::min);
        let max_price = recent_prices.iter().cloned().fold(f64::MIN, f64::max);
        let range_pct = (max_price - min_price) / min_price;
        let last = closes.last().copied().unwrap_or(0.0);

        // Phase A: Selling Climax
        if range_pct < 0.05 && recent_vol > avg_vol * 1.5 {
            patterns.push("Phase A: Potential selling climax — high volume in narrow range".into());
        }

        // Phase B: Accumulation/Distribution
        if range_pct < 0.08 {
            let vol_below_avg = recent_vol < avg_vol;
            if vol_below_avg {
                patterns.push("Phase B: Accumulation range — low volume consolidation".into());
            } else {
                patterns.push("Phase B: Distribution range — high volume at resistance".into());
            }
        }

        // Phase C: Spring
        if last < min_price * 1.01 && last > min_price * 0.97 {
            patterns.push(format!("Phase C: Potential spring at {:.2} — shakeout below support", min_price));
        }

        // Phase D: Sign of Strength
        if last > max_price * 1.01 && recent_vol > avg_vol * 1.3 {
            patterns.push(format!("Phase D: Sign of Strength — breakout above resistance {:.2} on volume", max_price));
        }

        if patterns.is_empty() {
            vec!["No clear Wyckoff structure detected".into()]
        } else {
            patterns
        }
    }
}
