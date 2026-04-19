//! Telemetry call sites. v1 ships no telemetry — every `event()` is a no-op,
//! but the call sites exist so opt-in analytics can be enabled in a future
//! release with zero code churn.

/// Record an event. First argument is the event name in snake_case.
///
/// ```ignore
/// chronimage::telemetry::event("import_completed", &[("source_id", "42"), ("count", "1000")]);
/// ```
#[allow(clippy::needless_pass_by_value)]
pub fn event(_name: &str, _props: &[(&str, &str)]) {
    // Intentionally empty in v1.
}

/// Record a timing measurement in milliseconds.
#[allow(clippy::needless_pass_by_value)]
pub fn timing(_name: &str, _ms: u64) {
    // Intentionally empty in v1.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_is_noop() {
        event("test", &[("k", "v")]);
    }

    #[test]
    fn timing_is_noop() {
        timing("test", 42);
    }
}
