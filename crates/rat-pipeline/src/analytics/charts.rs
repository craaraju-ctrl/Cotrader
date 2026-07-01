//! Chart Generator — ASCII charts for terminal display.

pub struct ChartGenerator;

impl ChartGenerator {
    pub fn sparkline(data: &[f64], width: usize) -> String {
        if data.is_empty() { return String::new(); }
        let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = max - min;
        if range == 0.0 { return "─".repeat(width); }

        let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let step = if data.len() > width { data.len() / width } else { 1 };

        data.iter().step_by(step).take(width).map(|&val| {
            let normalized = ((val - min) / range * 7.0) as usize;
            chars[normalized.min(7)]
        }).collect()
    }

    pub fn bar_chart(data: &[(String, f64)], width: usize) -> String {
        let max_val = data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
        if max_val == 0.0 { return String::new(); }

        let mut result = String::new();
        for (label, value) in data {
            let bar_len = (value / max_val * width as f64) as usize;
            result.push_str(&format!("{}: {}█{}\n", label, "█".repeat(bar_len), value));
        }
        result
    }
}
