use slog::{o, Drain, Logger, OwnedKV, SendSyncRefUnwindSafeKV};

#[inline]
pub fn term_logger() -> Logger {
    term_logger_with_values(o!())
}

pub fn term_logger_with_values<T>(kv: OwnedKV<T>) -> Logger
where
    T: SendSyncRefUnwindSafeKV + 'static,
{
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator)
        .use_custom_timestamp(|f| {
            let dt = chrono::Local::now();
            let ts = dt.format("%Y-%m-%d %H:%M:%S.%3f");
            f.write(ts.to_string().as_bytes()).map(|_| ())
        })
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, kv)
}
