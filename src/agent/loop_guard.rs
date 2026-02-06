//! Loop guard for agentic tool-calling loops.
//!
//! Detects when the LLM is stuck calling the same tool repeatedly with
//! similar arguments/results and injects a hint to force reconsideration.

use std::collections::VecDeque;

/// Tracks recent tool calls and detects stuck loops.
pub struct LoopGuard {
    /// Recent (tool_name, arguments_hash, result_snippet) entries.
    recent: VecDeque<(String, u64, String)>,
    /// How many consecutive same-tool-same-result calls trigger intervention.
    threshold: usize,
}

impl LoopGuard {
    /// Create a new guard. `threshold` is how many consecutive identical
    /// results from the same tool trigger a hint (default: 3).
    pub fn new(threshold: usize) -> Self {
        Self {
            recent: VecDeque::with_capacity(threshold + 1),
            threshold,
        }
    }

    /// Record a tool call and its result. Returns `Some(hint)` if the LLM
    /// appears stuck and should be told to stop retrying.
    pub fn record(&mut self, tool_name: &str, arguments: &str, result: &str) -> Option<String> {
        let arg_hash = Self::simple_hash(arguments);
        let result_snippet = Self::snippet(result);

        self.recent.push_back((tool_name.to_string(), arg_hash, result_snippet.clone()));

        // Keep only the last `threshold` entries
        while self.recent.len() > self.threshold {
            self.recent.pop_front();
        }

        // Check if all recent entries are the same tool with same result
        if self.recent.len() >= self.threshold {
            let all_same = self.recent.iter().all(|(name, _, snip)| {
                name == tool_name && *snip == result_snippet
            });

            if all_same {
                self.recent.clear(); // Reset so we don't keep firing
                return Some(format!(
                    "[SYSTEM] The tool '{}' has returned the same result {} times in a row. \
                     Do NOT call this tool again with a similar query. \
                     Instead, respond to the user with what you already know, \
                     or try a completely different approach.",
                    tool_name, self.threshold
                ));
            }
        }

        None
    }

    /// Reset the guard (e.g., between conversations).
    pub fn reset(&mut self) {
        self.recent.clear();
    }

    /// Simple non-cryptographic hash for argument deduplication.
    fn simple_hash(s: &str) -> u64 {
        let mut h: u64 = 0;
        for b in s.bytes() {
            h = h.wrapping_mul(31).wrapping_add(b as u64);
        }
        h
    }

    /// Take the first 200 chars of a result for comparison.
    fn snippet(s: &str) -> String {
        if s.len() <= 200 {
            s.to_string()
        } else {
            s[..200].to_string()
        }
    }
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_trigger_on_different_results() {
        let mut guard = LoopGuard::new(3);
        assert!(guard.record("web_search", r#"{"q":"a"}"#, "result 1").is_none());
        assert!(guard.record("web_search", r#"{"q":"b"}"#, "result 2").is_none());
        assert!(guard.record("web_search", r#"{"q":"c"}"#, "result 3").is_none());
    }

    #[test]
    fn triggers_on_repeated_same_result() {
        let mut guard = LoopGuard::new(3);
        let result = "No results found";
        assert!(guard.record("web_search", r#"{"q":"a"}"#, result).is_none());
        assert!(guard.record("web_search", r#"{"q":"b"}"#, result).is_none());
        assert!(guard.record("web_search", r#"{"q":"c"}"#, result).is_some());
    }

    #[test]
    fn different_tools_dont_trigger() {
        let mut guard = LoopGuard::new(3);
        let result = "error";
        assert!(guard.record("tool_a", "{}", result).is_none());
        assert!(guard.record("tool_b", "{}", result).is_none());
        assert!(guard.record("tool_a", "{}", result).is_none());
    }

    #[test]
    fn resets_after_trigger() {
        let mut guard = LoopGuard::new(2);
        let result = "same";
        assert!(guard.record("t", "{}", result).is_none());
        assert!(guard.record("t", "{}", result).is_some());
        // After trigger, internal state is cleared
        assert!(guard.record("t", "{}", result).is_none());
    }
}
