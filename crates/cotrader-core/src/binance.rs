//! Binance symbol normalization and classification logic.
//!
//! Pure functions only — no API calls. All live data fetching has been moved
//! to Tredo Exchange (the single price gateway).

const EQUITY_SYMBOLS: &[&str] = &[
    "NIFTY", "SENSEX", "BANKNIFTY", "RELIANCE", "TCS", "INFY", "HDFC", "ICICIBANK",
    "AAPL", "MSFT", "GOOG", "AMZN", "TSLA", "NVDA", "META", "NFLX", "AMD", "INTC",
    "PYPL", "QCOM", "ADBE", "CRM", "CSCO", "PEP", "KO", "NKE", "DIS", "V", "MA",
    "JPM", "BAC", "WMT", "COST", "PG", "HD", "XOM", "CVX", "UNH", "LLY", "JNJ",
    "MRK", "PFE", "ABBV", "ABT", "MDT", "T", "VZ", "CMCSA", "C", "WFC", "MS", "GS",
    "BLK", "SCHW", "AMAT", "LRCX", "ASML", "TSM", "AVGO", "ORCL", "IBM", "ACN", "TXN",
    "MU", "NOW", "PANW", "FTNT", "CRWD", "DDOG", "NET", "OKTA", "SNOW", "U", "PLTR",
    "MSTR", "COIN", "HOOD", "BABA", "PDD", "JD", "BIDU", "NTES", "LI", "XPEV", "NIO",
    "TME", "F", "GM", "GE", "FSLR", "ENPH", "SEDG", "RUN", "SPWR", "CAT", "DE", "HON",
    "LMT", "GD", "NOC", "RTX", "BA", "UPS", "FDX", "SBUX", "MCD", "CMG", "YUM", "TGT",
    "TJX", "DG", "DLTR", "ROST", "ABNB",
];

const KNOWN_CRYPTO: &[&str] = &[
    "BTC", "ETH", "SOL", "BNB", "XRP", "ADA", "DOGE", "AVAX", "MATIC", "POL", "LINK", "DOT",
    "ATOM", "LTC", "BCH", "UNI", "AAVE", "NEAR", "ICP", "FIL", "APT", "ARB", "OP", "SUI", "INJ",
    "TIA", "SEI", "PEPE", "WIF", "SHIB", "TON", "TRX", "XLM", "BONK", "FLOKI", "RENDER", "FET",
    "RNDR", "HBAR", "VET", "ALGO", "FTM", "SAND", "MANA", "CRV", "MKR", "COMP", "SNX", "RUNE",
    "STX", "IMX", "GRT", "ENS", "LDO", "BLUR", "JUP", "PYTH", "WLD", "STRK", "ENA", "LUNA", "FTT",
    "KAVA", "ZIL", "AXS", "CHZ", "ENJ", "ONE", "HOT", "QTUM", "ONT", "BAT", "ZRX", "ZEC", "DASH",
    "XMR", "ETC", "EOS", "NEO", "IOTA", "XTZ", "KNC", "LRC", "SXP", "YFI", "BAL", "OXT", "SUSHI",
    "1INCH", "WOO", "JASMY", "GMT", "KAS", "ORDI", "AEVO", "ETHFI", "BOME", "MEW", "TURBO", "MEME",
    "NOT", "ONDO", "IO", "PENDLE", "JTO", "RAY", "FIDA", "OM", "ARK", "PHB", "ACH", "RSR", "CHR",
    "MINA", "DYDX", "GALA", "AR", "FLOW", "THETA", "EGLD", "CELO", "RLC", "GMX", "JOE", "ALPHA",
    "CVX", "FXS", "LQTY",
];

/// Normalize user-facing symbol to base asset (BTCUSDT → BTC).
pub fn normalize_base_symbol(symbol: &str) -> String {
    let upper = symbol.trim().to_uppercase();
    if upper.ends_with("USDT") {
        upper.trim_end_matches("USDT").to_string()
    } else if upper.ends_with("USD") {
        upper.trim_end_matches("USD").to_string()
    } else if upper.ends_with("BUSD") {
        upper.trim_end_matches("BUSD").to_string()
    } else {
        upper
    }
}

/// Candidate Binance spot pairs for a base symbol (primary + aliases).
pub fn pair_candidates(base: &str) -> Vec<String> {
    let base = normalize_base_symbol(base);
    let mut pairs = vec![format!("{base}USDT")];
    match base.as_str() {
        "MATIC" => pairs.push("POLUSDT".to_string()),
        "POL" => pairs.push("MATICUSDT".to_string()),
        "PEPE" => pairs.push("1000PEPEUSDT".to_string()),
        "SHIB" => pairs.push("1000SHIBUSDT".to_string()),
        "BONK" => pairs.push("1000BONKUSDT".to_string()),
        "FLOKI" => pairs.push("1000FLOKIUSDT".to_string()),
        "LUNC" => pairs.push("1000LUNCUSDT".to_string()),
        "XEC" => pairs.push("1000XECUSDT".to_string()),
        _ => {}
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

pub fn to_binance_pair(symbol: &str) -> String {
    let norm = normalize_base_symbol(symbol);
    let std_pair = format!("{norm}USDT");
    let candidates = pair_candidates(symbol);
    if candidates.contains(&std_pair) {
        std_pair
    } else {
        candidates.into_iter().next().unwrap_or(std_pair)
    }
}

pub fn is_crypto_symbol(symbol: &str) -> bool {
    let upper = symbol.trim().to_uppercase();
    if upper.ends_with(".NS") || upper.ends_with(".BO") {
        return false;
    }
    if EQUITY_SYMBOLS.iter().any(|&eq| upper == eq) {
        return false;
    }
    let base = normalize_base_symbol(&upper);
    if upper.ends_with("USDT") || upper.ends_with("USD") {
        return true;
    }
    KNOWN_CRYPTO.contains(&base.as_str())
}

/// Map a user-facing symbol to its Yahoo Finance ticker.
/// Indian stocks need the `.NS` suffix; NIFTY needs `^NSEI`.
pub fn yahoo_symbol(symbol: &str) -> String {
    let upper = symbol.trim().to_uppercase();
    match upper.as_str() {
        "NIFTY" => "^NSEI".to_string(),
        "RELIANCE" => "RELIANCE.NS".to_string(),
        "TCS" => "TCS.NS".to_string(),
        "INFY" => "INFY.NS".to_string(),
        "HDFCBANK" => "HDFCBANK.NS".to_string(),
        "ICICIBANK" => "ICICIBANK.NS".to_string(),
        "WIPRO" => "WIPRO.NS".to_string(),
        "TATAMOTORS" => "TATAMOTORS.NS".to_string(),
        "ADANIENT" => "ADANIENT.NS".to_string(),
        "BAJFINANCE" => "BAJFINANCE.NS".to_string(),
        "SBIN" => "SBIN.NS".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_usdt() {
        assert_eq!(normalize_base_symbol("BTCUSDT"), "BTC");
        assert_eq!(normalize_base_symbol("btc"), "BTC");
    }

    #[test]
    fn pair_candidates_include_aliases() {
        let pepe = pair_candidates("PEPE");
        assert!(pepe.contains(&"PEPEUSDT".to_string()));
        assert!(pepe.contains(&"1000PEPEUSDT".to_string()));
    }

    #[test]
    fn is_crypto_rejects_equities() {
        assert!(!is_crypto_symbol("NIFTY"));
        assert!(!is_crypto_symbol("RELIANCE"));
        assert!(is_crypto_symbol("BTC"));
        assert!(is_crypto_symbol("ETHUSDT"));
    }
}
