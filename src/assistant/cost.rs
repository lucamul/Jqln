//! A rough dollar estimate for a session's token use. The table is hand-kept
//! and will drift; an unknown model just shows the token count with no price.

/// USD per million tokens, `(input, output)`, matched by the first substring
/// that appears in the model id. Order matters — longer / more specific keys
/// first.
const PRICES: &[(&str, f64, f64)] = &[
    ("claude-opus", 15.0, 75.0),
    ("claude-3-5-haiku", 0.80, 4.0),
    ("claude-haiku", 1.0, 5.0),
    ("claude-3-5-sonnet", 3.0, 15.0),
    ("claude-sonnet", 3.0, 15.0),
    ("claude-3-opus", 15.0, 75.0),
    ("claude-3-haiku", 0.25, 1.25),
    ("gpt-4o-mini", 0.15, 0.60),
    ("gpt-4o", 2.50, 10.0),
    ("gpt-4.1-mini", 0.40, 1.60),
    ("gpt-4.1-nano", 0.10, 0.40),
    ("gpt-4.1", 2.0, 8.0),
    ("gpt-5-mini", 0.25, 2.0),
    ("gpt-5", 1.25, 10.0),
    ("o4-mini", 1.10, 4.40),
    ("o3-mini", 1.10, 4.40),
    ("o3", 2.0, 8.0),
];

/// Estimated USD for `input`/`output` tokens on `model`, or `None` when the
/// model is not in the table.
pub fn estimate(model: &str, input: u64, output: u64) -> Option<f64> {
    let m = model.to_lowercase();
    let (_, pin, pout) = PRICES.iter().find(|(k, _, _)| m.contains(k))?;
    Some((input as f64 * pin + output as f64 * pout) / 1_000_000.0)
}

/// `"48.2k tok"` — a compact token count.
pub fn tokens(n: u64) -> String {
    if n < 1000 {
        format!("{n} tok")
    } else {
        format!("{:.1}k tok", n as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_model_prices_and_unknown_does_not() {
        // 1M in + 1M out on sonnet = $3 + $15.
        let c = estimate("claude-sonnet-4-5", 1_000_000, 1_000_000).unwrap();
        assert!((c - 18.0).abs() < 1e-6);
        assert!(estimate("some-local-model", 100, 100).is_none());
        // Substring match works on a dated id.
        assert!(estimate("gpt-4o-2024-11-20", 0, 0).is_some());
    }

    #[test]
    fn token_formatting() {
        assert_eq!(tokens(950), "950 tok");
        assert_eq!(tokens(48_200), "48.2k tok");
    }
}
