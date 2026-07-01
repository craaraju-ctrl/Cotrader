pub struct HarmonicDetector;

impl HarmonicDetector {
    pub fn name() -> &'static str { "HarmonicDetector" }

    pub fn detect(&self, data: &[f64]) -> Vec<String> {
        if data.len() < 25 {
            return vec!["Insufficient data for harmonic pattern detection (need 25+ bars)".into()];
        }
        let closes: Vec<f64> = data.iter().skip(3).step_by(5).cloned().collect();
        let len = closes.len();
        let mut patterns = Vec::new();

        // Find 5 swing points (X, A, B, C, D)
        let mut swings: Vec<(usize, f64)> = Vec::new();
        for i in 2..len - 2 {
            if closes[i] > closes[i - 1] && closes[i] > closes[i + 1] {
                swings.push((i, closes[i]));
            }
            if closes[i] < closes[i - 1] && closes[i] < closes[i + 1] {
                swings.push((i, closes[i]));
            }
        }

        if swings.len() >= 5 {
            let recent: Vec<_> = swings.iter().rev().take(5).rev().collect();
            let xa = (recent[0].1 - recent[1].1).abs();
            let ab = (recent[1].1 - recent[2].1).abs();
            let bc = (recent[2].1 - recent[3].1).abs();
            let cd = (recent[3].1 - recent[4].1).abs();

            if xa > 0.001 {
                let ab_ratio = ab / xa;
                let bc_ratio = bc / ab;
                let cd_ratio = cd / bc;

                // Gartley: AB = 0.618 XA, BC = 0.382-0.886 AB, CD = 1.27-1.618 BC
                if (ab_ratio - 0.618).abs() < 0.15 && bc_ratio > 0.382 && bc_ratio < 0.886 && cd_ratio > 1.27 {
                    patterns.push(format!(
                        "Potential Gartley pattern — AB/XA={:.3} (target 0.618), BC/AB={:.3}, CD/BC={:.3}",
                        ab_ratio, bc_ratio, cd_ratio
                    ));
                }
                // Butterfly: AB = 0.786 XA, CD = 1.618 BC
                if (ab_ratio - 0.786).abs() < 0.15 && (cd_ratio - 1.618).abs() < 0.3 {
                    patterns.push(format!(
                        "Potential Butterfly pattern — AB/XA={:.3} (target 0.786), CD/BC={:.3} (target 1.618)",
                        ab_ratio, cd_ratio
                    ));
                }
                // Bat: AB = 0.382-0.50 XA, CD = 0.886 XA
                if ab_ratio > 0.382 && ab_ratio < 0.50 {
                    let xd = (recent[0].1 - recent[4].1).abs();
                    let xd_ratio = xd / xa;
                    if (xd_ratio - 0.886).abs() < 0.15 {
                        patterns.push(format!(
                            "Potential Bat pattern — AB/XA={:.3}, XD/XA={:.3} (target 0.886)",
                            ab_ratio, xd_ratio
                        ));
                    }
                }
            }
        }

        if patterns.is_empty() {
            vec!["No harmonic patterns detected".into()]
        } else {
            patterns
        }
    }
}
