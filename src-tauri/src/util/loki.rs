//! Minimal tracing → Loki layer using `reqwest` 0.12 directly.
//!
//! Replaces `tracing-loki` after its built-in shipper silently stopped
//! delivering pushes — Loki's own ingest metrics showed zero backend lines
//! received despite `tracing-loki` reporting successful init. The crate's
//! `BackgroundTask` future ran but never made it onto the wire for reasons
//! that weren't worth diagnosing given how small the alternative is.
//!
//! This layer:
//!   - captures every event's timestamp, level, target, message + non-
//!     message fields
//!   - queues onto an unbounded tokio mpsc
//!   - a dedicated std::thread runs a current-thread tokio runtime that
//!     drains the queue, batches entries (every 500 ms or when 100 accrue),
//!     and POSTs to `{url}/loki/api/v1/push`
//!   - emits `eprintln!` lines on bind/push failure so broken dev setups
//!     surface at boot instead of silently swallowing logs
//!
//! The JSON shape matches the one the frontend uses (see `src/util/log.ts`)
//! so `{app="chronimage"}` queries in Loki interleave both streams cleanly.

use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

/// Single log event in the mpsc queue.
#[derive(Debug)]
struct LogLine {
    timestamp_ns: u128,
    level: &'static str,
    target: String,
    message: String,
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

impl<S: Subscriber> Layer<S> for LokiLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let message = visitor.finalize();
        // Unbounded channel; send can only fail if the shipper thread has
        // died. Dropping the event in that case is the right call — we
        // don't want logging to block the app.
        let _ = self.tx.send(LogLine {
            timestamp_ns: now,
            level: meta.level().as_str(),
            target: meta.target().to_string(),
            message,
        });
    }
}

// ── Field visitor ───────────────────────────────────────────────────────────

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl MessageVisitor {
    fn finalize(self) -> String {
        let suffix = if self.fields.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = self
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!(" {}", parts.join(" "))
        };
        match self.message {
            Some(m) => format!("{m}{suffix}"),
            None => suffix.trim_start().to_string(),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields
                .push((field.name().to_string(), format!("{value:?}")));
        }
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
    // Group entries by level so Loki sees a separate stream per severity
    // (matches `tracing-loki`'s behaviour + lets Grafana filter by level).
    let mut by_level: HashMap<&'static str, Vec<[String; 2]>> = HashMap::new();
    for entry in taken {
        let formatted = format!("{} {}", entry.target, entry.message);
        by_level
            .entry(entry.level)
            .or_default()
            .push([entry.timestamp_ns.to_string(), formatted]);
    }
    let streams: Vec<serde_json::Value> = by_level
        .into_iter()
        .map(|(level, values)| {
            let mut stream: HashMap<&str, String> = HashMap::new();
            for (k, v) in labels {
                stream.insert(k.as_str(), v.clone());
            }
            // Loki uses lowercase level names by convention.
            let lowered = level.to_lowercase();
            stream.insert("level", lowered);
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
