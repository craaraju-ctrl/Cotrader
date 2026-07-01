pub struct MoneyManager;

impl MoneyManager {
    pub fn name() -> &'static str { "MoneyManager" }
    pub fn role() -> &'static str { "Money Manager" }

    pub fn kelly_size(&self, win_rate: f64, avg_win: f64, avg_loss: f64) -> String {
        let b = if avg_loss > 0.0 { avg_win / avg_loss } else { 1.0 };
        let q = 1.0 - win_rate;
        let kelly = (b * win_rate - q) / b;
        let half_kelly = (kelly / 2.0).max(0.0);
        format!(
            "Kelly sizing: Win rate={:.1}%, Payoff={:.2}\n\
             Full Kelly: {:.1}% | Half Kelly: {:.1}%\n\
             Recommendation: Use half Kelly ({:.1}%) for conservative sizing",
            win_rate * 100.0, b, kelly * 100.0, half_kelly * 100.0, half_kelly * 100.0
        )
    }

    pub fn heat_adjust(&self, size: f64, heat: f64) -> String {
        let adjustment = if heat > 0.06 { 0.5 } else if heat > 0.04 { 0.75 } else { 1.0 };
        let adjusted = size * adjustment;
        format!(
            "Heat adjustment: Size {:.2} to {:.2} (heat={:.1}%, factor={:.0}%)",
            size, adjusted, heat * 100.0, adjustment * 100.0
        )
    }

    pub fn conviction_scale(&self, base_size: f64, conviction: f64, regime: &str) -> String {
        let regime_factor = match regime {
            "trending_bull" => 1.2,
            "trending_bear" => 0.6,
            "ranging" => 0.8,
            "volatile" => 0.5,
            _ => 1.0,
        };
        let scaled = base_size * conviction * regime_factor;
        format!(
            "Conviction scaling: Base {:.2} x conviction {:.2} x regime {} ({:.1}) = {:.2}",
            base_size, conviction, regime, regime_factor, scaled
        )
    }

    pub fn max_position(&self, equity: f64, risk_pct: f64, stop_distance: f64) -> String {
        let risk_amount = equity * risk_pct;
        let position_size = if stop_distance > 0.0 { risk_amount / stop_distance } else { 0.0 };
        format!(
            "Max position: Equity ${:.0} x risk {:.1}% = ${:.0} risk\n\
             Stop distance: {:.2} | Max size: {:.4} units",
            equity, risk_pct * 100.0, risk_amount, stop_distance, position_size
        )
    }
}
