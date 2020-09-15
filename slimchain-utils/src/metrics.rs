pub use serde_json;

use crossbeam_channel::{bounded, Sender};
use once_cell::sync::OnceCell;
use serde_json::Value as JsonValue;
use slimchain_common::error::{anyhow, Result};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
    thread::{self, JoinHandle},
};

const BUFFERED_ENTRY_SIZE: usize = 10_000;
pub static METRICS_DISPATCH: OnceCell<Dispatch> = OnceCell::new();

pub struct Dispatch {
    sender: Sender<DispatchEvent>,
}

impl Dispatch {
    pub fn add_entry(&self, value: JsonValue) {
        self.sender.try_send(DispatchEvent::Entry(value)).ok();
    }
}

enum DispatchEvent {
    Shutdown,
    Entry(JsonValue),
}

pub struct Guard {
    sender: Sender<DispatchEvent>,
    handler: Option<JoinHandle<()>>,
}

impl Guard {
    fn new(writer: impl Write + Send + Sync + 'static) -> Result<Self> {
        let (tx, rx) = bounded(BUFFERED_ENTRY_SIZE);
        METRICS_DISPATCH
            .set(Dispatch { sender: tx.clone() })
            .map_err(|_e| anyhow!("Metrics already init."))?;
        let handler = thread::spawn(move || {
            let mut writer = writer;
            while let Ok(entry) = rx.recv() {
                match entry {
                    DispatchEvent::Shutdown => break,
                    DispatchEvent::Entry(value) => {
                        serde_json::to_writer(&mut writer, &value).ok();
                        writeln!(writer).ok();
                    }
                }
            }
            writer.flush().ok();
        });
        Ok(Self {
            sender: tx,
            handler: Some(handler),
        })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.sender.send(DispatchEvent::Shutdown).ok();
        if let Some(handler) = self.handler.take() {
            handler.join().ok();
        }
    }
}

#[macro_export]
macro_rules! __record_entry {
    ($x:expr) => {
        if let Some(dispatch) = $crate::metrics::METRICS_DISPATCH.get() {
            dispatch.add_entry($x);
        }
    };
}

#[macro_export]
macro_rules! record_time {
    ($label:literal, $time:expr) => {
        $crate::record_time!($label, $time, );
    };
    ($label:literal, $time:expr, $($fields:tt)*) => {
        match $time {
            t => {
                let fields = $crate::metrics::serde_json::json!({ $($fields)* });
                let entry = $crate::metrics::serde_json::json!({
                    "k": "time",
                    "l": $label,
                    "t": t.as_millis() as u64,
                    "v": fields,
                });
                $crate::__record_entry!(entry);
            }
        }
    };
}

#[macro_export]
macro_rules! record_event {
    ($label:literal) => {
        $crate::record_event!($label,);
    };
    ($label:literal, $($fields:tt)*) => {{
        let ts = $crate::chrono::Utc::now().to_rfc3339_opts($crate::chrono::SecondsFormat::Millis, true);
        let fields = $crate::metrics::serde_json::json!({ $($fields)* });
        let entry = $crate::metrics::serde_json::json!({
            "k": "event",
            "l": $label,
            "ts": ts.as_str(),
            "v": fields,
        });
        $crate::__record_entry!(entry);
    }};
}

pub fn init_metrics_subscriber(writer: impl Write + Send + Sync + 'static) -> Result<Guard> {
    Guard::new(writer)
}

pub fn init_metrics_subscriber_using_file(file: impl AsRef<Path>) -> Result<Guard> {
    let f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    let w = BufWriter::new(f);
    init_metrics_subscriber(w)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn test() {
        let _guard = crate::init_tracing_for_test();

        let time = Duration::from_millis(2200);
        record_time!("test_time", time);
        record_time!("test_time", time, "foo": 1);
        record_event!("test_event", "id": 1);
        tracing::error!("An error");
        tracing::info!("An info");
    }
}
