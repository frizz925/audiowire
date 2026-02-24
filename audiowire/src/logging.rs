use std::{env, io::stderr, str::FromStr};

use chrono::Local;
use slog::{Drain, Level, o};

pub fn initialize() -> slog::Logger {
    let level = env::var("RUST_LOG")
        .ok()
        .and_then(|s| Level::from_str(s.as_str()).ok())
        .unwrap_or(Level::Info);

    let disable_timestamp = env::var("DISABLE_LOG_TIMESTAMP")
        .map(|s| s == "true")
        .unwrap_or_default();

    let plain = slog_term::PlainSyncDecorator::new(stderr());
    let drain = slog_term::FullFormat::new(plain)
        .use_custom_timestamp(move |f| {
            if !disable_timestamp {
                let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                write!(f, "{ts}")
            } else {
                Ok(())
            }
        })
        .build()
        .fuse();
    let drain = slog::LevelFilter::new(drain, level).fuse();

    slog::Logger::root(drain, o!())
}
