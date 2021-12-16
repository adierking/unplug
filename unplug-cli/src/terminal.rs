use console::Term;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressStyle};
use lazy_static::lazy_static;
use log::{info, log_enabled, Level, Log};
use simplelog::{Color, ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};
use std::sync::Mutex;
use unplug::audio::ProgressHint;

const PROGRESS_UPDATE_RATE: u64 = 10;

/// Progress bar spinner characters.
const TICK_CHARS: &str = r"⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ";

lazy_static! {
    /// The style to use for progress bars.
    static ref PROGRESS_STYLE: ProgressStyle = ProgressStyle::default_bar()
        .template("       {spinner:.cyan} [{eta_precise}] [{bar:40}] {percent}% {msg}")
        .progress_chars("=> ")
        .tick_chars(TICK_CHARS)
        .on_finish(ProgressFinish::AndClear);

    /// The style to use for progress spinners.
    static ref SPINNER_STYLE: ProgressStyle = ProgressStyle::default_spinner()
        .template("       {spinner:.cyan} [{elapsed_precise}] {msg}")
        .tick_chars(TICK_CHARS)
        .on_finish(ProgressFinish::AndClear);

    /// The `Term` to use for visible progress bars.
    static ref PROGRESS_TERM: Term = Term::buffered_stderr();

    /// The currently-active progress bar.
    static ref PROGRESS_BAR: Mutex<Option<ProgressBar>> = Mutex::new(None);
}

/// Makes a `ProgressDrawTarget` using default settings. This returns a hidden target if trace
/// logging is enabled.
fn default_progress_target() -> ProgressDrawTarget {
    if log_enabled!(Level::Trace) {
        ProgressDrawTarget::hidden()
    } else {
        ProgressDrawTarget::term(PROGRESS_TERM.clone(), None)
    }
}

/// Hides the currently-visible progress bar. Returns `true` if the bar was visible beforehand.
fn hide_progress() -> bool {
    let mut lock = PROGRESS_BAR.lock().unwrap();
    if let Some(bar) = &*lock {
        if bar.is_finished() {
            *lock = None;
            return false;
        }
        if !bar.is_hidden() {
            // Disabling steady tick and then ticking ensures that the bar is drawn so we don't
            // accidentally clear a line with a log message on it
            bar.disable_steady_tick();
            bar.tick();
            // Now disable drawing and clear one line up to make room for the log message
            bar.set_draw_target(ProgressDrawTarget::hidden());
            let _ = PROGRESS_TERM.clear_last_lines(1);
            let _ = PROGRESS_TERM.flush();
            return true;
        }
    }
    false
}

/// Shows the progress bar again after it was hidden with `hide_progress()`.
fn show_progress() {
    let lock = PROGRESS_BAR.lock().unwrap();
    if let Some(bar) = &*lock {
        if bar.is_hidden() {
            // Steady tick was disabled in hide_progress()
            bar.enable_steady_tick(1000 / PROGRESS_UPDATE_RATE);
            bar.set_draw_target(default_progress_target());
        }
    }
}

/// Wrapps a logger so that log messages do not interfere with progress bars.
struct ProgressBarLogger<L: Log> {
    inner: L,
}

impl<L: Log> Log for ProgressBarLogger<L> {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        let hidden = hide_progress();
        self.inner.log(record);
        if hidden {
            show_progress();
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Initializes logging to the terminal.
pub fn init_logging(verbosity: u64) {
    let filter = if verbosity >= 2 {
        // Note: trace logs are compiled out in release builds
        LevelFilter::Trace
    } else if verbosity == 1 {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let config = ConfigBuilder::new()
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Trace)
        .set_time_format_str("%T%.3f")
        .set_level_color(Level::Info, Some(Color::Green))
        .build();
    let logger = TermLogger::new(filter, config, TerminalMode::Stderr, ColorChoice::Auto);
    let wrapper = Box::new(ProgressBarLogger { inner: logger });
    log::set_max_level(filter);
    log::set_boxed_logger(wrapper).expect("failed to set global logger");
}

/// Creates a progress bar using the standard style with initial length `len`.
pub fn progress_bar(len: u64) -> ProgressBar {
    let target = default_progress_target();
    let bar = ProgressBar::with_draw_target(len, target).with_style(PROGRESS_STYLE.clone());
    if !bar.is_hidden() {
        *PROGRESS_BAR.lock().unwrap() = Some(bar.clone());
        bar.enable_steady_tick(1000 / PROGRESS_UPDATE_RATE);
    }
    bar
}

/// Creates a progress spinner using the standard style which displays `message`. If trace logging
/// is enabled, the spinner will be hidden and the message will be logged instead.
pub fn progress_spinner(message: String) -> ProgressBar {
    let target = default_progress_target();
    let bar = ProgressBar::with_draw_target(u64::MAX, target).with_style(SPINNER_STYLE.clone());
    if bar.is_hidden() {
        info!("{}", message);
    } else {
        *PROGRESS_BAR.lock().unwrap() = Some(bar.clone());
        bar.set_message(message);
        bar.enable_steady_tick(1000 / PROGRESS_UPDATE_RATE);
    }
    bar
}

/// Updates a progress bar based on an audio progress hint.
pub fn update_audio_progress(bar: &ProgressBar, progress: Option<ProgressHint>) {
    if let Some(progress) = progress {
        if bar.length() != progress.total.get() {
            bar.set_length(progress.total.get());
        }
        bar.set_position(progress.current);
    } else {
        bar.tick();
    }
}
