use slimchain_common::error::{ensure, Result};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tracing::{Dispatch, Level};
use tracing_appender::non_blocking::WorkerGuard as TracingLoggerGuard;

static METRICS_DISPATCH_INIT: AtomicBool = AtomicBool::new(false);
pub static mut METRICS_DISPATCH: Option<(Dispatch, TracingLoggerGuard)> = None;

#[macro_export]
macro_rules! use_metrics_subscriber {
    ($x:tt) => {
        if let Some((dispatch, _)) = unsafe { $crate::metrics::METRICS_DISPATCH.as_ref() } {
            $crate::tracing::dispatcher::with_default(dispatch, || $x)
        }
    };
}

#[macro_export]
macro_rules! record_time {
    (label: $label:literal, $time:expr) => {
        $crate::record_time!(label: $label, $time, );
    };
    (label: $label:literal, $time:expr, $($fields:tt)*) => {
        match $time {
            t => {
                $crate::use_metrics_subscriber! {{
                    $crate::tracing::trace!(
                        kind = "time",
                        label = $label,
                        time_ms = (t.as_millis() as u64),
                        $($fields)*
                    );
                }};
            }
        }
    };
}

#[macro_export]
macro_rules! record_event {
    (label: $label:literal) => {
        $crate::record_event!(label: $label,);
    };
    (label: $label:literal, $($fields:tt)*) => {
        $crate::use_metrics_subscriber! {{
            $crate::tracing::trace!(
                kind = "event",
                label = $label,
                timestamp = $crate::chrono::Utc::now()
                    .to_rfc3339_opts($crate::chrono::SecondsFormat::Millis, true)
                    .as_str(),
                $($fields)*
            );
        }};
    };
}

pub fn init_metrics_subscriber(writer: impl Write + Send + Sync + 'static) -> Result<()> {
    ensure!(
        !METRICS_DISPATCH_INIT.compare_and_swap(false, true, Ordering::SeqCst),
        "Metrics subscriber already init."
    );

    let (writer, guard) = tracing_appender::non_blocking(writer);
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(Level::TRACE)
        .with_writer(writer)
        .without_time()
        .finish();

    unsafe {
        METRICS_DISPATCH = Some((Dispatch::new(subscriber), guard));
    }

    Ok(())
}

pub fn init_metrics_subscriber_using_file(file: impl AsRef<Path>) -> Result<()> {
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
        crate::init_tracing_for_test();

        let time = Duration::from_millis(2200);
        record_time!(label: "test_time", time);
        record_time!(label: "test_time", time, foo = 1);
        record_event!(label: "test_event", id = 1);
        tracing::error!("An error");
        tracing::info!("An info");
    }
}
