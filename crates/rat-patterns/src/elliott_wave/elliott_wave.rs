pub struct ElliottWaveDetector;

impl ElliottWaveDetector {
    pub fn name() -> &'static str { "ElliottWaveDetector" }

    pub fn detect(&self, data: &[f64]) -> Vec<String> {
        if data.len() < 25 {
            return vec!["Insufficient data for Elliott Wave analysis (need 25+ bars)".into()];
        }
        let closes: Vec<f64> = data.iter().skip(3).step_by(5).cloned().collect();
        let len = closes.len();

        // Find swing points (peaks and troughs)
        let mut swings: Vec<(usize, f64, bool)> = Vec::new(); // (index, price, is_peak)
        for i in 2..len - 2 {
            if closes[i] > closes[i - 1] && closes[i] > closes[i + 1] {
                swings.push((i, closes[i], true));
            }
            if closes[i] < closes[i - 1] && closes[i] < closes[i + 1] {
                swings.push((i, closes[i], false));
            }
        }

        if swings.len() < 5 {
            return vec!["Not enough swing points for wave analysis".into()];
        }

        let mut waves = Vec::new();

        // Count alternating swings
        let last5: Vec<_> = swings.iter().rev().take(5).rev().collect();
        let is_impulse = last5.windows(2).all(|w| {
            (w[0].2 && !w[1].2) || (!w[0].2 && w[1].2)
        });

        if is_impulse && last5.len() == 5 {
            let wave1 = last5[0].1;
            let wave3 = last5[2].1;
            let wave5 = last5[4].1;

            if last5[0].2 && !last5[4].2 {
                waves.push(format!(
                    "Potential impulse wave up: W1={:.2}, W3={:.2}, W5={:.2} — bearish reversal zone",
                    wave1, wave3, wave5
                ));
            } else if !last5[0].2 && last5[4].2 {
                waves.push(format!(
                    "Potential impulse wave down: W1={:.2}, W3={:.2}, W5={:.2} — bullish reversal zone",
                    wave1, wave3, wave5
                ));
            }

            // Check wave 3 extension
            if wave3 > wave1 * 1.618 {
                waves.push("Wave 3 extended beyond 1.618 — strong trend".into());
            }
        } else {
            waves.push("Corrective wave structure detected — wait for impulse completion".into());
        }

        if waves.is_empty() {
            waves.push("No clear Elliott Wave pattern".into());
        }
        waves
    }
}
