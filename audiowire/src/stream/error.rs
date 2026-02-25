use slog::{Logger, error};

use crate::backend::stream::ErrorFn;

pub fn create_error_cb(log: Logger) -> impl ErrorFn + 'static {
    move |err| {
        error!(log, "Audio stream error"; "error" => err);
    }
}
