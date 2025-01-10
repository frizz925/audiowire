use std::{env, error::Error, sync::atomic::Ordering, thread::sleep, time::Duration};

use audiowire::{
    handlers::handle_signal, initialize, logging::term_logger, start_playback, start_record,
    terminate, Config, SampleFormat, Stream,
};
use slog::info;

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

    let mut record = start_record(input.as_deref(), config)?;
    record
        .device_name()
        .map(|s| info!(logger, "Record started, device: {}", s))
        .unwrap_or_else(|| info!(logger, "Record started"));

    let mut playback = start_playback(output.as_deref(), config)?;
    playback
        .device_name()
        .map(|s| info!(logger, "Playback started, device: {}", s))
        .unwrap_or_else(|| info!(logger, "Playback started"));

    let term = handle_signal()?;
    while !term.load(Ordering::Relaxed) {
        // Do nothing lmao
        sleep(Duration::from_secs(1));
    }

    record.stop()?;
    info!(logger, "Record stopped");

    playback.stop()?;
    info!(logger, "PLayback stopped");

    terminate()?;

    Ok(())
}
