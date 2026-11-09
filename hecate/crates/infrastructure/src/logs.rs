//! Logs: one JSON line per event on stdout with the keys of the Python version (research R11):
//! `ts`, `level`, `logger`, `event`, the explicit fields and, for an error, only its kind.

use std::fmt;
use std::io::Write;

use hecate_domain::instant::isoformat;
use serde_json::{Map, Value};
use time::OffsetDateTime;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

pub struct JsonLines;

struct Fields {
    event: String,
    extra: Map<String, Value>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record(field, Value::from(format!("{value:?}")));
    }
}

impl Fields {
    fn record(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.event = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
        } else {
            self.extra.insert(field.name().to_string(), value);
        }
    }
}

fn level_name(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "error",
        Level::WARN => "warning",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

/// One log line as the JSON lines logger writes it, without the line break.
pub fn render(event: &Event<'_>) -> String {
    let mut fields = Fields {
        event: String::new(),
        extra: Map::new(),
    };
    event.record(&mut fields);
    let mut document = Map::new();
    document.insert(
        "ts".into(),
        Value::from(isoformat(OffsetDateTime::now_utc())),
    );
    document.insert(
        "level".into(),
        Value::from(level_name(event.metadata().level())),
    );
    document.insert("logger".into(), Value::from(event.metadata().target()));
    document.insert("event".into(), Value::from(fields.event));
    document.extend(fields.extra);
    Value::Object(document).to_string()
}

impl<S: Subscriber> Layer<S> for JsonLines {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut line = render(event);
        line.push('\n');
        let _ = std::io::stdout().lock().write_all(line.as_bytes());
    }
}

/// Installs the JSON lines logger at level info.
pub fn configure_logging() {
    let _ = tracing_subscriber::registry()
        .with(JsonLines)
        .with(LevelFilter::INFO)
        .try_init();
}
