use std::{env, error::Error, ffi::c_void, ptr, sync::atomic::Ordering, thread::sleep};

use audiowire::{
    handlers::handle_signal, initialize, logging::term_logger, terminate, Config, SampleFormat,
    Stream, StreamBuilder,
};
use slog::{error, info, Logger};

fn error_cb(err: i32, message: &str, userdata: *mut c_void) {
    let logger = unsafe { &ptr::read(userdata as *mut Logger) };
    error!(logger, "Error {}: {}", err, message);
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args();
    let input = args.nth(1);
    let output = args.next();
    let config = Config {
        channels: 2,
        sample_rate: 48000,
        sample_format: SampleFormat::F32,
        buffer_frames: 480,
        max_buffer_frames: 4800,
    };

    initialize()?;

    let logger = term_logger();

    let mut record = StreamBuilder::new(config)
        .error_cb(error_cb, Some(logger.clone()))
        .start_record("Source", input.as_deref())?;
    record
        .device_name()
        .map(|s| info!(logger, "Record started, device: {}", s))
        .unwrap_or_else(|| info!(logger, "Record started"));

    let mut playback = StreamBuilder::new(config)
        .error_cb(error_cb, Some(logger.clone()))
        .start_playback("Sink", output.as_deref())?;
    playback
        .device_name()
        .map(|s| info!(logger, "Playback started, device: {}", s))
        .unwrap_or_else(|| info!(logger, "Playback started"));

    let term = handle_signal()?;
    let duration = config.buffer_duration();
    let bufsize = config.buffer_size();
    let mut buf = [0u8; 65536];
    while !term.load(Ordering::Relaxed) {
        while record.peek() >= bufsize {
            let read = record.read(&mut buf[..bufsize]);
            playback.write(&buf[..read]);
        }
        sleep(duration);
    }

    record.stop()?;
    info!(logger, "Record stopped");

    playback.stop()?;
    info!(logger, "PLayback stopped");

    terminate()?;

    Ok(())
}
