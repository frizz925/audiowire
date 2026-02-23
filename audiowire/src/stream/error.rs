use slog::{Logger, error};

pub fn create_error_cb(log: Logger) -> impl Fn(i32, &str) + 'static {
    move |code: i32, message: &str| {
        error!(log, "Audio stream error"; "code" => code, "message" => message);
    }
}
