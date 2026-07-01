pub struct ChartPatternDetector;

impl ChartPatternDetector {
    pub fn name() -> &'static str { "ChartPatternDetector" }

    pub fn detect(&self, data: &[f64]) -> Vec<String> {
        if data.len() < 30 {
            return vec!["Insufficient data for chart pattern detection (need 30+ bars)".into()];
        }
        let mut patterns = Vec::new();
        let closes: Vec<f64> = data.iter().skip(3).step_by(5).cloned().collect();
        let len = closes.len();

        // Double Top detection
        if len >= 20 {
            let mut peaks = Vec::new();
            for i in 2..len - 2 {
                if closes[i] > closes[i - 1] && closes[i] > closes[i + 1]
                    && closes[i] > closes[i - 2] && closes[i] > closes[i + 2] {
                    peaks.push((i, closes[i]));
                }
            }
            if peaks.len() >= 2 {
                let (i1, p1) = peaks[peaks.len() - 2];
                let (i2, p2) = peaks[peaks.len() - 1];
                if (p1 - p2).abs() / p1.max(0.01) < 0.02 && (i2 - i1) >= 5 {
                    patterns.push(format!("Double Top detected — peaks at bars {} and {} (price {:.2})", i1, i2, p1));
                }
            }
        }

        // Double Bottom detection
        if len >= 20 {
            let mut troughs = Vec::new();
            for i in 2..len - 2 {
                if closes[i] < closes[i - 1] && closes[i] < closes[i + 1]
                    && closes[i] < closes[i - 2] && closes[i] < closes[i + 2] {
                    troughs.push((i, closes[i]));
                }
            }
            if troughs.len() >= 2 {
                let (i1, t1) = troughs[troughs.len() - 2];
                let (i2, t2) = troughs[troughs.len() - 1];
                if (t1 - t2).abs() / t1.max(0.01) < 0.02 && (i2 - i1) >= 5 {
                    patterns.push(format!("Double Bottom detected — troughs at bars {} and {} (price {:.2})", i1, i2, t1));
                }
            }
        }

        // Trendline break
        if len >= 10 {
            let recent_high = closes[len - 10..].iter().cloned().fold(f64::MIN, f64::max);
            let recent_low = closes[len - 10..].iter().cloned().fold(f64::MAX, f64::min);
            let last = *closes.last().unwrap_or(&0.0);
            if last > recent_high * 0.99 {
                patterns.push(format!("Breakout above recent resistance at {:.2}", recent_high));
            }
            if last < recent_low * 1.01 {
                patterns.push(format!("Breakdown below recent support at {:.2}", recent_low));
            }
        }

        if patterns.is_empty() {
            vec!["No chart patterns detected".into()]
        } else {
            patterns
        }
    }
}
