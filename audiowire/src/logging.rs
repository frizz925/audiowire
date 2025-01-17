use std::{
    env,
    io::{self, Write},
};

use slog::{o, Drain, Logger, OwnedKV, Record, SendSyncRefUnwindSafeKV};
use slog_term::{CountingWriter, RecordDecorator, ThreadSafeTimestampFn};

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
        root_builder.use_custom_header_print(print_msg_header)
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

fn print_msg_header(
    _fn_timestamp: &dyn ThreadSafeTimestampFn<Output = io::Result<()>>,
    mut rd: &mut dyn RecordDecorator,
    record: &Record,
    use_file_location: bool,
) -> io::Result<bool> {
    rd.start_level()?;
    write!(rd, "{}", record.level().as_short_str())?;

    if use_file_location {
        rd.start_location()?;
        write!(
            rd,
            "[{}:{}:{}]",
            record.location().file,
            record.location().line,
            record.location().column
        )?;
    }

    rd.start_whitespace()?;
    write!(rd, " ")?;

    rd.start_msg()?;
    let mut count_rd = CountingWriter::new(&mut rd);
    write!(count_rd, "{}", record.msg())?;
    Ok(count_rd.count() != 0)
}
