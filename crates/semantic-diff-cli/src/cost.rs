//! F20: prompt-cost estimation.
//!
//! Heuristics:
//! - Token count ≈ `chars / 4` (rounded up). This is a coarse approximation
//!   that's accurate to within ~20% for English/code prompts on most modern
//!   tokenizers (cl100k, Claude, etc.). Good enough for a cost preview.
//! - Output token estimate ≈ 30% of input tokens. Code-review responses tend
//!   to be substantially shorter than the prompt; this is a deliberate
//!   under-estimate so the preview doesn't scare users away.
//!
//! Both heuristics are intentionally cheap (no tokenizer dependency).

use semantic_diff_core::config::CostEntry;

/// Heuristic token estimate: ~4 chars per token, rounded up.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

/// Heuristic output-token estimate: ~30% of input.
pub fn estimate_output_tokens(input_tokens: u64) -> u64 {
    (input_tokens as f64 * 0.3) as u64
}

/// Compute the USD cost for a section given a cost entry (rates per million tokens).
pub fn estimate_cost(input_tokens: u64, output_tokens_est: u64, entry: &CostEntry) -> f64 {
    (input_tokens as f64 / 1_000_000.0) * entry.input_per_mtok
        + (output_tokens_est as f64 / 1_000_000.0) * entry.output_per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_rounds_up() {
        // 5 chars / 4 = 1.25 → 2.
        assert_eq!(estimate_tokens("hello"), 2);
        // 8 chars / 4 = 2 → 2.
        assert_eq!(estimate_tokens("aaaabbbb"), 2);
        // 9 chars / 4 = 2.25 → 3.
        assert_eq!(estimate_tokens("aaaabbbbc"), 3);
    }

    #[test]
    fn estimate_tokens_is_deterministic() {
        let s = "the quick brown fox jumps over the lazy dog";
        assert_eq!(estimate_tokens(s), estimate_tokens(s));
    }

    #[test]
    fn estimate_output_tokens_is_30_percent() {
        assert_eq!(estimate_output_tokens(1000), 300);
        assert_eq!(estimate_output_tokens(0), 0);
        assert_eq!(estimate_output_tokens(1), 0); // floor: 0.3 → 0
    }

    #[test]
    fn estimate_cost_known_entry() {
        // claude:sonnet-4 default: 3.0 in / 15.0 out per Mtok.
        let entry = CostEntry { input_per_mtok: 3.0, output_per_mtok: 15.0 };
        // 1M input + 1M output = 3 + 15 = 18.
        let c = estimate_cost(1_000_000, 1_000_000, &entry);
        assert!((c - 18.0).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn estimate_cost_zero_tokens() {
        let entry = CostEntry { input_per_mtok: 100.0, output_per_mtok: 100.0 };
        assert_eq!(estimate_cost(0, 0, &entry), 0.0);
    }

    #[test]
    fn estimate_cost_partial() {
        // 100k input × 3.0 / Mtok = 0.3 USD.
        let entry = CostEntry { input_per_mtok: 3.0, output_per_mtok: 15.0 };
        let c = estimate_cost(100_000, 0, &entry);
        assert!((c - 0.3).abs() < 1e-9, "got {c}");
    }
}
