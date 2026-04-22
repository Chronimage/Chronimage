//! Minimal tracing → Loki layer using `reqwest` 0.12 directly.
//!
//! Replaces `tracing-loki` after its built-in shipper silently stopped
//! delivering pushes — Loki's own ingest metrics showed zero backend lines
//! received despite `tracing-loki` reporting successful init. The crate's
//! `BackgroundTask` future ran but never made it onto the wire for reasons
//! that weren't worth diagnosing given how small the alternative is.
//!
//! ## What ships
//!
//! **Stream labels** (indexed, low cardinality — filter cheaply in Loki):
//!
//! - `app`, `env`, `layer=backend`, `pid` (from [`LokiLayer::spawn`])
//! - `level` (derived per-event; splits the batch into one stream per
//!   severity so Grafana's level filter works as-expected)
//!
//! **Structured metadata** (per-entry, not indexed — queryable via
//! `| logfmt` / `label_format`):
//!
//! - `target` — full `module::path` of the event
//! - `span` — name of the closest enclosing span when present (pulled
//!   via `LookupSpan`)
//! - One key per non-message field on the event (e.g. `source_id=42`,
//!   `flow_id=abc`, `email=foo@bar`). Keys are lowercased and `.`/`-`
//!   in names are normalised to `_` for Grafana's parser.
//!
//! The log line itself is just the human message + field tail, matching
//! the default `fmt::layer` output. This keeps line filters in Grafana
//! (`|= "picker"`) intuitive while giving precise label filters for
//! programmatic queries.
//!
//! ## Why split labels vs. metadata
//!
//! Labels are indexed — each unique label-value combo becomes a separate
//! stream. High cardinality blows up Loki. Things like `source_id` or
//! `flow_id` have unbounded value sets, so they go in structured metadata
//! which is stored per-entry but not indexed.

use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

/// Single log event in the mpsc queue.
#[derive(Debug)]
struct LogLine {
    timestamp_ns: u128,
    level: &'static str,
    target: String,
    span: Option<String>,
    message: String,
    fields: Vec<(String, String)>,
}

/// Labels attached to every stream this layer pushes.
pub type Labels = Vec<(String, String)>;

/// Tracing layer that forwards events to Loki via a background thread.
pub struct LokiLayer {
    tx: mpsc::UnboundedSender<LogLine>,
}

impl LokiLayer {
    /// Spawn the shipper thread and return a layer you can install on a
    /// `tracing_subscriber::Registry`. `push_url` is the full URL to
    /// POST to (e.g. `http://localhost:3101/loki/api/v1/push`).
    pub fn spawn(push_url: String, labels: Labels) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("loki-shipper".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("loki-shipper: runtime init failed: {e}");
                        return;
                    }
                };
                eprintln!(
                    "loki-shipper: thread started, target={push_url}, labels={}",
                    format_labels(&labels)
                );
                rt.block_on(ship_loop(push_url, labels, rx));
            })
            .expect("spawn loki-shipper thread");
        LokiLayer { tx }
    }
}

impl<S> Layer<S> for LokiLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let meta = event.metadata();

        // Visit the event's fields so we can separate the `message` text
        // from structured keys.
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // Closest enclosing span name, if any. Spans let callers stamp
        // "operation context" onto nested events without repeating the
        // same k/v on every log line.
        let span = ctx
            .event_scope(event)
            .and_then(|mut s| s.next().map(|sp| sp.name().to_string()));

        let message = visitor.message.unwrap_or_default();
        let fields = visitor.fields;

        // Unbounded channel; send can only fail if the shipper thread has
        // died. Dropping the event in that case is the right call — we
        // don't want logging to block the app.
        let _ = self.tx.send(LogLine {
            timestamp_ns: now,
            level: meta.level().as_str(),
            target: meta.target().to_string(),
            span,
            message,
            fields,
        });
    }
}

// ── Field visitor ───────────────────────────────────────────────────────────

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl FieldVisitor {
    fn record(&mut self, name: &str, value: String) {
        if name == "message" {
            self.message = Some(value);
        } else {
            self.fields.push((normalise_key(name), value));
        }
    }
}

/// Loki structured-metadata keys are logfmt-parsed in Grafana; `.` and
/// `-` break that parser. Lowercase them + swap the separators for `_`.
fn normalise_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch == '.' || ch == '-' {
            out.push('_');
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field.name(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field.name(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field.name(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record(field.name(), format!("{value:?}"));
    }
}

// ── Background shipper ──────────────────────────────────────────────────────

async fn ship_loop(push_url: String, labels: Labels, mut rx: mpsc::UnboundedReceiver<LogLine>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("loki-shipper: reqwest client init failed: {e}");
            return;
        }
    };
    let mut batch: Vec<LogLine> = Vec::with_capacity(100);
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            line = rx.recv() => {
                match line {
                    Some(l) => {
                        batch.push(l);
                        if batch.len() >= 100 {
                            flush(&client, &push_url, &labels, &mut batch).await;
                        }
                    }
                    None => {
                        flush(&client, &push_url, &labels, &mut batch).await;
                        return;
                    }
                }
            }
            _ = tick.tick() => {
                if !batch.is_empty() {
                    flush(&client, &push_url, &labels, &mut batch).await;
                }
            }
        }
    }
}

async fn flush(client: &reqwest::Client, url: &str, labels: &Labels, batch: &mut Vec<LogLine>) {
    if batch.is_empty() {
        return;
    }
    let taken = std::mem::take(batch);
    // Group by level so Loki sees one stream per severity; Grafana's level
    // picker depends on it.
    let mut by_level: HashMap<&'static str, Vec<serde_json::Value>> = HashMap::new();
    for entry in taken {
        // Per-entry structured metadata — not indexed, queryable via
        // `| logfmt` or `label_format` in Grafana.
        let mut metadata: HashMap<String, String> = HashMap::with_capacity(2 + entry.fields.len());
        metadata.insert("target".into(), entry.target.clone());
        if let Some(span) = &entry.span {
            metadata.insert("span".into(), span.clone());
        }
        for (k, v) in &entry.fields {
            // Don't clobber the reserved keys above if a caller happens
            // to stamp a field called "target" or "span" — prefix theirs.
            if k == "target" || k == "span" {
                metadata.insert(format!("field_{k}"), v.clone());
            } else {
                metadata.insert(k.clone(), v.clone());
            }
        }

        // Line body: `target message field=value field=value`. Keeps the
        // default fmt::layer shape so `|= "picker"` line-filters in
        // Grafana still work.
        let field_tail = if entry.fields.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = entry
                .fields
                .iter()
                .map(|(k, v)| format!(" {k}={v}"))
                .collect();
            parts.join("")
        };
        let line = if entry.message.is_empty() {
            format!("{}{field_tail}", entry.target)
        } else {
            format!("{} {}{field_tail}", entry.target, entry.message)
        };

        by_level
            .entry(entry.level)
            .or_default()
            .push(serde_json::json!([
                entry.timestamp_ns.to_string(),
                line,
                metadata,
            ]));
    }

    let streams: Vec<serde_json::Value> = by_level
        .into_iter()
        .map(|(level, values)| {
            let mut stream: HashMap<&str, String> = HashMap::new();
            for (k, v) in labels {
                stream.insert(k.as_str(), v.clone());
            }
            stream.insert("level", level.to_lowercase());
            serde_json::json!({ "stream": stream, "values": values })
        })
        .collect();
    let body = serde_json::json!({ "streams": streams });
    match client.post(url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("loki-shipper: push rejected HTTP {status}: {body}");
        }
        Err(e) => eprintln!("loki-shipper: push failed: {e}"),
    }
}

fn format_labels(labels: &Labels) -> String {
    labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}
