#![allow(clippy::mutex_atomic)]

use crate::common::format_duration;
use console::Term;
use crossterm::cursor::{Hide, MoveToColumn, MoveToPreviousLine, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::queue;
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType};
use crossterm::tty::IsTty;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressStyle};
use lazy_static::lazy_static;
use log::{info, log_enabled, Level, Log};
use simplelog::{Color, ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};
use std::convert::TryInto;
use std::io::{self, StdoutLock, Write};
use std::sync::Mutex;
use std::time::Duration;
use unplug::audio::ProgressHint;

const PROGRESS_UPDATE_RATE: u64 = 10;

/// Progress bar spinner characters.
const TICK_CHARS: &str = r"⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ";
const TICK_CHARS_PAUSED: &str = r"⠿ ";

/// The minimum audio volume the playback UI can set.
const MIN_VOLUME: i32 = 0;
/// The maximum audio volume the playback UI can set.
const MAX_VOLUME: i32 = 100;

lazy_static! {
    /// The style to use for progress bars.
    static ref PROGRESS_STYLE: ProgressStyle = ProgressStyle::default_bar()
        .template("       {spinner:.cyan} [{eta_precise}] [{bar:40}] {percent}% {wide_msg}")
        .progress_chars("=> ")
        .tick_chars(TICK_CHARS)
        .on_finish(ProgressFinish::AndClear);

    /// The style to use for progress spinners.
    static ref SPINNER_STYLE: ProgressStyle = ProgressStyle::default_spinner()
        .template("       {spinner:.cyan} [{elapsed_precise}] {wide_msg}")
        .tick_chars(TICK_CHARS)
        .on_finish(ProgressFinish::AndClear);

    /// The style to use for audio playback.
    static ref PLAYBACK_STYLE: ProgressStyle = ProgressStyle::default_bar()
        .template("{spinner:.cyan} [{prefix}] <{wide_bar}> {msg}")
        .progress_chars("-|-")
        .tick_chars(TICK_CHARS)
        .on_finish(ProgressFinish::AndClear);

    /// The style to use for paused audio playback.
    static ref PLAYBACK_PAUSED_STYLE: ProgressStyle = ProgressStyle::default_bar()
        .template("{spinner:.cyan} [{prefix}] <{wide_bar}> {msg}")
        .progress_chars("-|-")
        .tick_chars(TICK_CHARS_PAUSED)
        .on_finish(ProgressFinish::AndClear);

    /// The `Term` to use for visible progress bars.
    static ref PROGRESS_TERM: Term = Term::buffered_stderr();

    /// The currently-active progress bar.
    static ref PROGRESS_BAR: Mutex<Option<ProgressBar>> = Mutex::new(None);

    /// `true` if the terminal is known to be in raw mode.
    static ref IN_RAW_MODE: Mutex<bool> = Mutex::new(false);
}

/// Makes a `ProgressDrawTarget` using default settings. This returns a hidden target if trace
/// logging is enabled.
fn default_progress_target() -> ProgressDrawTarget {
    if log_enabled!(Level::Trace) {
        ProgressDrawTarget::hidden()
    } else {
        ProgressDrawTarget::term(PROGRESS_TERM.clone(), Some(1000 / PROGRESS_UPDATE_RATE))
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
        if *IN_RAW_MODE.lock().unwrap() {
            // In raw mode, we have to move the cursor back to the start of the line
            eprint!("\r");
        }
        if hidden {
            show_progress();
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Returns `true` if stdout is a TTY.
pub fn is_tty() -> bool {
    io::stdout().is_tty()
}

pub fn enable_raw_mode() {
    let mut lock = IN_RAW_MODE.lock().unwrap();
    if !*lock {
        crossterm::terminal::enable_raw_mode().unwrap();
        *lock = true;
    }
}

pub fn disable_raw_mode() {
    let mut lock = IN_RAW_MODE.lock().unwrap();
    if *lock {
        crossterm::terminal::disable_raw_mode().unwrap();
        *lock = false;
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

/// Creates a progress bar which shows audio playback progress.
pub fn progress_playback(duration: Duration, message: String) -> ProgressBar {
    let prefix = format_duration(Duration::default());
    let message = format!("[{}] {}", format_duration(duration), message);
    let length = duration.as_millis().try_into().unwrap();
    let target = default_progress_target();
    let bar = ProgressBar::with_draw_target(length, target)
        .with_style(PLAYBACK_STYLE.clone())
        .with_prefix(prefix)
        .with_message(message);
    if !bar.is_hidden() {
        *PROGRESS_BAR.lock().unwrap() = Some(bar.clone());
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

/// Updates a playback progress bar with the current position in the stream.
pub fn update_playback_position(bar: &ProgressBar, mut position: Duration) {
    let length = Duration::from_millis(bar.length());
    position = position.min(length);
    bar.set_prefix(format_duration(position));
    bar.set_position(position.as_millis().try_into().unwrap());
}

/// Sets a playback progress bar's style based on whether playback is paused.
pub fn set_playback_paused(bar: &ProgressBar, paused: bool) {
    bar.set_style(if paused { PLAYBACK_PAUSED_STYLE.clone() } else { PLAYBACK_STYLE.clone() });
}

/// Trait for a `PlaybackUi` controller.
pub trait PlaybackController {
    fn update(&mut self) -> bool;
    fn stop(&mut self);

    fn name(&self) -> &str;
    fn duration(&self) -> Duration;
    fn position(&self) -> Option<Duration>;

    fn pause(&mut self);
    fn unpause(&mut self);

    fn volume(&self) -> f64;
    fn set_volume(&self, volume: f64);
}

/// TUI visibility modes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    Visible,
}

/// A terminal-based user interface for audio playback.
pub struct PlaybackUi<T: PlaybackController> {
    /// The backend controller which manages the audio stream.
    controller: T,
    /// The UI visibility mode.
    visibility: Visibility,
    /// `true` if the UI has been initialized with `initialize()`.
    initialized: bool,
    /// `true` if playback is paused.
    paused: bool,
    //// The playback volume as a percentage.
    volume: i32,
    /// The progress bar showing playback progress.
    progress: ProgressBar,
}

impl<T: PlaybackController> PlaybackUi<T> {
    /// Creates a new `PlaybackUi` with `controller` as the backend. `visibility` controls whether
    /// to draw the UI.
    pub fn new(controller: T, visibility: Visibility) -> Self {
        Self {
            controller,
            visibility,
            initialized: false,
            paused: false,
            progress: ProgressBar::hidden(),
            volume: 100,
        }
    }

    /// Displays and runs the UI until audio has stopped playing.
    pub fn run(mut self) {
        if !self.initialized {
            self.initialize();
        }
        while self.controller.update() {
            if self.visibility == Visibility::Visible {
                self.update_ui();
            }
            if event::poll(Duration::from_secs(0)).unwrap_or_default() {
                if let Ok(Event::Key(key)) = event::read() {
                    self.handle_key(key);
                }
            }
        }
    }

    /// Initializes the UI and initially draws it if necessary.
    fn initialize(&mut self) {
        assert!(!self.initialized);
        self.volume = (self.controller.volume() * 100.0).round() as i32;
        if self.visibility == Visibility::Visible {
            self.show_ui();
        }
        // Raw mode is always necessary in order for key input to work
        enable_raw_mode();
        self.initialized = true;
    }

    /// Draws the UI for the first time.
    fn show_ui(&mut self) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        write!(handle, "\n\n").unwrap();
        self.draw_controls_locked(&mut handle);
        queue!(handle, Hide, MoveToPreviousLine(1)).unwrap();
        handle.flush().unwrap();
        drop(handle);

        let duration = self.controller.duration();
        let name = self.controller.name().to_owned();
        self.progress = progress_playback(duration, name);
    }

    /// Clears the UI, returning the terminal to the state it was in before `show_ui()`.
    fn hide_ui(&mut self) {
        self.progress.finish_using_style();

        // Clearing the progress bar will put us on the line right after the first blank line we
        // printed. Move to it and clear down to clear the entire interface.
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        queue!(handle, MoveToPreviousLine(1), Clear(ClearType::FromCursorDown), Show).unwrap();
        handle.flush().unwrap();
    }

    /// Updates the UI after playback has updated.
    fn update_ui(&mut self) {
        if let Some(position) = self.controller.position() {
            update_playback_position(&self.progress, position);
        }
    }

    /// Redraws the controls line.
    fn draw_controls(&mut self) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        self.draw_controls_locked(&mut handle);
        handle.flush().unwrap();
    }

    /// Redraws the controls line with stdout already locked.
    fn draw_controls_locked(&mut self, handle: &mut StdoutLock<'_>) {
        queue!(handle, Clear(ClearType::CurrentLine), MoveToColumn(0)).unwrap();
        write!(
            handle,
            "              {} Stop  /  {} {}  /  {} Volume ({}%)",
            "[ENTER]".bold(),
            "[SPACE]".bold(),
            if self.paused { "Play" } else { "Pause" },
            "[UP][DOWN]".bold(),
            self.volume,
        )
        .unwrap();
    }

    /// Handles a key event.
    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => self.stop(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.stop(),
            KeyCode::Char(' ' | 'p') => self.toggle_paused(),
            KeyCode::Up | KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_volume(5),
            KeyCode::Down | KeyCode::Char('-') => self.adjust_volume(-5),
            _ => (),
        }
    }

    /// Stops playback in response to user input.
    fn stop(&mut self) {
        self.controller.stop();
    }

    /// Toggles whether playback is paused.
    fn toggle_paused(&mut self) {
        self.paused = !self.paused;
        if self.paused {
            self.controller.pause();
        } else {
            self.controller.unpause();
        }
        if self.visibility == Visibility::Visible {
            set_playback_paused(&self.progress, self.paused);
            self.draw_controls();
        }
    }

    /// Adds `delta` to the playback volume.
    fn adjust_volume(&mut self, delta: i32) {
        self.volume = (self.volume + delta).clamp(MIN_VOLUME, MAX_VOLUME);
        self.controller.set_volume(f64::from(self.volume) / 100.0);
        if self.visibility == Visibility::Visible {
            self.draw_controls();
        }
    }
}

impl<T: PlaybackController> Drop for PlaybackUi<T> {
    fn drop(&mut self) {
        if self.initialized {
            disable_raw_mode(); // Raw mode is always enabled
            if self.visibility == Visibility::Visible {
                self.hide_ui();
            }
        }
    }
}
