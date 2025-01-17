use std::env;

use slog::{o, Drain, Logger, OwnedKV, SendSyncRefUnwindSafeKV};

#[inline]
pub fn term_logger() -> Logger {
    term_logger_with_values(o!())
}

pub fn term_logger_with_values<T>(kv: OwnedKV<T>) -> Logger
where
    T: SendSyncRefUnwindSafeKV + 'static,
{
    let timestamp_disabled = env::var("LOG_TIMESTAMP_DISABLED")
        .map(|s| s == "1")
        .unwrap_or_default();
    let decorator = slog_term::TermDecorator::new().build();
    let root_builder = slog_term::FullFormat::new(decorator);
    let builder = if timestamp_disabled {
        root_builder.use_custom_timestamp(|_| Ok(()))
    } else {
        root_builder.use_custom_timestamp(|f| {
            let dt = chrono::Local::now();
            let ts = dt.format("%Y-%m-%d %H:%M:%S.%3f");
            f.write(ts.to_string().as_bytes()).map(|_| ())
        })
    };
    let format_drain = builder.build().fuse();
    let drain = slog_async::Async::new(format_drain).build().fuse();
    slog::Logger::root(drain, kv)
}
