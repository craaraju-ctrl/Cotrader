//! Low rules — warnings only, never block.

pub mod max_positions_per_symbol;
pub mod max_total_positions;
pub mod symbol_frequency_cap;
pub mod minimum_hold_time;

use crate::rule::Rule;

pub fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(max_positions_per_symbol::MaxPositionsPerSymbol),
        Box::new(max_total_positions::MaxTotalPositions),
        Box::new(symbol_frequency_cap::SymbolFrequencyCap),
        Box::new(minimum_hold_time::MinimumHoldTime),
    ]
}
