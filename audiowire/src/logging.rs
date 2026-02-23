use std::{env, str::FromStr};

use slog::{Drain, Level, o};

pub fn initialize() -> slog::Logger {
    let level = env::var("RUST_LOG")
        .map_err(|_| ())
        .and_then(|s| Level::from_str(s.as_str()))
        .unwrap_or(Level::Info);

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog::LevelFilter::new(drain, level).fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
}
