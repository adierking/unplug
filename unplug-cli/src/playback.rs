use super::terminal::{self, PlaybackController, PlaybackUi};
use crate::terminal::Visibility;
use anyhow::{bail, Result};
use arrayvec::ArrayVec;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, OutputCallbackInfo, SampleRate, SupportedStreamConfig};
use log::{debug, error, info, log_enabled, trace, Level};
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use unplug::audio::format::pcm::Scalable;
use unplug::audio::format::{AnyFormat, Cast, Convert, PcmF32Le, PcmFormat, PcmS16Le, PcmU16Le};
use unplug::audio::volume::{ScaleAmplitude, Volume};
use unplug::audio::{Error, ReadSamples, SampleFilter, Samples};

/// The interval to update the playback UI at.
const UI_UPDATE_INTERVAL: Duration = Duration::from_millis(10);

/// An audio source which is suitable for playback.
pub struct PlaybackSource {
    /// The source reader that samples will be read from.
    reader: Box<dyn ReadSamples<'static, Format = PcmF32Le>>,
    /// The number of channels in the audio.
    channels: usize,
    /// The audio's sample rate.
    sample_rate: u32,
    /// The audio's volume scale.
    volume: f64,
}

impl PlaybackSource {
    /// Creates a new `PlaybackSource` for `reader`.
    pub fn new<F>(reader: impl ReadSamples<'static, Format = F> + 'static) -> Result<Self>
    where
        F: Convert<PcmF32Le>,
    {
        Self::new_impl(reader.convert())
    }

    fn new_impl(reader: Box<dyn ReadSamples<'static, Format = PcmF32Le>>) -> Result<Self> {
        let mut peekable = reader.peekable();
        let first = match peekable.peek_samples()? {
            Some(first) => first,
            None => bail!("audio stream is empty"),
        };
        let channels = first.channels;
        let sample_rate = first.rate;
        Ok(Self { reader: Box::from(peekable), channels, sample_rate, volume: 1.0 })
    }

    /// Changes the source's volume scale to `volume`.
    #[must_use]
    pub fn with_volume(mut self, volume: f64) -> Self {
        self.volume = volume;
        self
    }

    /// Gets the audio's sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the total duration of the audio source.
    /// ***Panics*** if the audio source does not know its length.
    pub fn duration(&self) -> Duration {
        let total_frames = self.reader.data_remaining().unwrap() / (self.channels as u64);
        Duration::from_secs_f64((total_frames as f64) / (self.sample_rate as f64))
    }

    /// Transforms the playback source into a reader to use for buffering. `target_channels` and
    /// `target_rate` are the required channel count and sample rate for the output stream.
    fn into_reader(
        self,
        target_channels: usize,
        target_rate: u32,
    ) -> Box<dyn ReadSamples<'static, Format = PcmF32Le>> {
        let mut audio = self.reader;
        if self.sample_rate != target_rate {
            debug!("Audio will be resampled from {} Hz to {} Hz", self.sample_rate, target_rate);
            audio = Box::from(audio.resample(target_rate));
        }
        if self.channels != target_channels {
            assert_eq!(target_channels, 2);
            debug!("Audio will be converted from mono to stereo");
            audio = Box::from(audio.stereo());
        }
        audio
    }
}

/// Pairs a frame number with the `Instant` it will be played. Useful for calculating the current
/// playback position.
#[derive(Copy, Clone)]
struct PlaybackInstant {
    instant: Instant,
    frame: u64,
}

impl PlaybackInstant {
    fn new(instant: Instant, sample: u64) -> Self {
        Self { instant, frame: sample }
    }

    /// Converts the instant to a duration since the start of playback.
    fn to_duration(self, sample_rate: u32) -> Duration {
        Duration::from_secs_f64((self.frame as f64) / (sample_rate as f64))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Status {
    /// Playback is waiting for the buffer to be filled.
    Buffering,
    /// Audio is currently playing.
    Playing,
    /// Audio playback is paused but can be resumed.
    Paused,
    /// Audio playback has finished and cannot be restarted.
    Finished,
}

/// Playback state shared across the threads running a stream.
struct PlaybackState {
    /// The number of channels in the stream.
    channels: usize,
    /// The stream's sample rate.
    sample_rate: u32,
    /// The current playback status.
    status: Status,
    /// Buffered samples.
    buffer: VecDeque<Samples<'static, PcmF32Le>>,
    /// Current offset within the first sample packet in the buffer.
    sample_offset: usize,
    /// Number of frames available in the buffer.
    frames_available: usize,
    /// The index of the next audio frame that will be enqueued.
    next_frame: u64,
    /// Known sample instants. The first instant is used as the base instant for calculating the
    /// playback position and will be removed after the second instant passes.
    instants: ArrayVec<[PlaybackInstant; 2]>,
    /// Volume filter used to adjust sample volumes in real-time.
    volume: Volume<PcmF32Le>,
    /// `true` if no more samples are available for buffering.
    eof: bool,
}

impl PlaybackState {
    /// Creates a new `PlaybackState` with an empty buffer.
    fn new(channels: usize, sample_rate: u32, volume: f64) -> Self {
        Self {
            channels,
            sample_rate,
            status: Status::Buffering,
            buffer: VecDeque::new(),
            sample_offset: 0,
            frames_available: 0,
            next_frame: 0,
            instants: ArrayVec::new(),
            volume: Volume::new(volume),
            eof: false,
        }
    }

    /// Returns `true` if the buffer is running low on samples and more need to be buffered.
    fn buffer_low(&self) -> bool {
        let min_frames = self.sample_rate as usize;
        !self.eof && self.frames_available < min_frames
    }

    /// Clears the internal sample buffer.
    fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.sample_offset = 0;
        self.frames_available = 0;
        self.instants.clear();
        if self.status == Status::Playing {
            self.status = Status::Buffering;
        }
    }

    /// Appends the packets in `samples` to the buffer.
    fn push_samples(
        &mut self,
        samples: impl IntoIterator<Item = Samples<'static, PcmF32Le>>,
    ) -> Result<()> {
        for packet in samples {
            if packet.len % packet.channels != 0 {
                return Err(Error::DifferentChannelSizes.into());
            }
            if packet.channels != self.channels {
                return Err(Error::InconsistentChannels.into());
            }
            if packet.rate != self.sample_rate {
                return Err(Error::InconsistentSampleRate.into());
            }
            let frames = packet.len / packet.channels;
            self.buffer.push_back(packet);
            self.frames_available += frames;
        }
        if self.status == Status::Buffering && !self.buffer_low() {
            self.status = Status::Playing;
        }
        Ok(())
    }

    /// Pops samples from the buffer into `data` and returns the number of filled samples.
    fn pop_samples<F: PcmFormat>(&mut self, data: &mut [F::Data]) -> usize
    where
        F::Data: Scalable,
    {
        assert!(self.status == Status::Playing);
        let mut frame = 0;
        let end_frame = data.len() / self.channels;
        while frame < end_frame {
            let samples = match self.buffer.front_mut() {
                Some(s) => s,
                None => break,
            };
            let available_frames = (samples.len - self.sample_offset) / self.channels;
            let num_frames = available_frames.min(end_frame - frame);
            let out_start = frame * self.channels;
            let in_start = self.sample_offset;
            let in_end = self.sample_offset + num_frames * self.channels;
            let in_data = &mut samples.data.to_mut()[in_start..in_end];
            self.volume.apply(in_data, self.channels, in_data.len()).unwrap();
            for (i, &sample) in in_data.iter().enumerate() {
                data[i + out_start] = F::Data::from_f64(sample.into());
            }
            frame += num_frames;
            self.frames_available -= num_frames;
            if in_end < samples.len {
                self.sample_offset = in_end;
            } else {
                self.buffer.pop_front();
                self.sample_offset = 0;
            }
        }
        if self.frames_available == 0 {
            self.status = if self.eof { Status::Finished } else { Status::Buffering };
        }
        frame * self.channels
    }

    /// Calculates the current playback position. Returns `None` if not known.
    fn position(&mut self) -> Option<Duration> {
        let now = Instant::now();
        // In order to calculate the playback position, we need to know the instant of a frame that
        // recently played. By keeping a queue of instants and removing the front when the next
        // instant is more recent, we ensure that the playback position corrects itself in case
        // playback gets behind or ahead.
        if self.instants.len() > 1 && now >= self.instants[1].instant {
            self.instants.remove(0);
        }
        self.instants
            .get(0)
            .filter(|i| now >= i.instant)
            .map(|i| now - i.instant + i.to_duration(self.sample_rate))
    }

    /// Pauses audio playback at the current position.
    fn pause(&mut self) {
        if self.status != Status::Finished {
            self.status = Status::Paused;
            self.instants.clear();
        }
    }

    /// Resumes audio playback if it was paused.
    fn unpause(&mut self) {
        if self.status == Status::Paused {
            self.status =
                if self.frames_available > 0 { Status::Playing } else { Status::Buffering };
        }
    }

    /// Stops audio playback permanently.
    fn stop(&mut self) {
        self.clear_buffer();
        self.status = Status::Finished;
        self.eof = true;
    }
}

/// Commands that can be sent to control stream operation.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StreamCommand {
    /// The buffer is running low and may need to be filled.
    FillBuffer,
    /// Stop playback as soon as possible.
    Stop,
}

/// Notifications that can be sent to the main thread as part of stream playback.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StreamNotification {
    /// The stream has samples buffered and is ready for playback to begin.
    Buffered,
    /// Playback has reached the end and the stream should be torn down.
    Finished,
}

/// State for the thread that manages the sample buffer.
struct BufferThread {
    /// A reference to the shared playback state.
    state: Arc<Mutex<PlaybackState>>,
    /// The reader to read fully-processed samples from.
    reader: Box<dyn ReadSamples<'static, Format = PcmF32Le>>,
    /// The receiver for stream commands.
    command_recv: Receiver<StreamCommand>,
    /// A sender for sending stream notifications.
    notify_send: Sender<StreamNotification>,
    /// `true` if the stream has reached the end and no more samples can be buffered.
    eof: bool,
}

impl BufferThread {
    fn run(&mut self) {
        self.fill_buffer();
        while let Ok(command) = self.command_recv.recv() {
            match command {
                StreamCommand::FillBuffer => self.fill_buffer(),
                StreamCommand::Stop => break,
            }
        }
        self.state.lock().unwrap().stop();
    }

    fn fill_buffer(&mut self) {
        if self.eof {
            return;
        }

        let mut state = self.state.lock().unwrap();
        let min_frames = state.sample_rate as usize;
        let mut frames = state.frames_available;
        drop(state); // Don't keep the lock held while we're processing samples
        if frames >= min_frames {
            return;
        }

        let mut samples = vec![];
        trace!("Refilling sample buffer: {}/{}", frames, min_frames);
        while frames < min_frames {
            match self.reader.read_samples().unwrap() {
                Some(packet) => {
                    trace!("Finished processing {} samples", packet.len);
                    frames += packet.len / packet.channels;
                    samples.push(packet);
                }
                None => {
                    trace!("Reached end of buffer, setting EOF");
                    self.eof = true;
                    break;
                }
            }
        }

        state = self.state.lock().unwrap();
        state.eof = self.eof;
        let old_status = state.status;
        state.push_samples(samples).expect("failed to extend sample buffer");
        if old_status == Status::Buffering && state.status == Status::Playing {
            debug!("Buffered {} audio frames", state.frames_available);
            self.notify_send.send(StreamNotification::Buffered).unwrap();
        }
    }
}

/// State for the callback which fills the audio device's sample buffer.
struct SampleCallback {
    /// A reference to the shared playback state. Can be `None` if playback has completed.
    state: Option<Arc<Mutex<PlaybackState>>>,
    /// A sender for sending stream commands.
    command_send: Sender<StreamCommand>,
    /// A sender for sending stream notifications.
    notify_send: Sender<StreamNotification>,
    /// `true` if the buffer is low and a `FillBuffer` command has already been sent.
    buffer_low: bool,
}

impl SampleCallback {
    fn write_samples<F>(&mut self, data: &mut [F::Data], info: &OutputCallbackInfo)
    where
        F: PcmFormat,
        F::Data: cpal::Sample + Default + Scalable,
    {
        let now = Instant::now();
        let mut state = match &mut self.state {
            Some(state) => state.lock().unwrap(),
            None => {
                data.fill(F::Data::default());
                return;
            }
        };

        let status = state.status;
        if status != Status::Playing {
            drop(state);
            if status == Status::Finished {
                self.state = None; // Release our reference to the state so it can be freed
            }
            data.fill(F::Data::default());
            return;
        }

        // We roughly know the instant when this function was called, and we know the delta between
        // the playback and callback instants, so we can add them to approximate the instant when
        // the first frame will play. See `PlaybackState::position()`.
        if !state.instants.is_full() {
            let timestamp = info.timestamp();
            let delta = timestamp.playback.duration_since(&timestamp.callback).unwrap_or_default();
            let instant = PlaybackInstant::new(now + delta, state.next_frame);
            state.instants.push(instant);
        }

        // It's possible that we could refill the buffer and then immediately end up in a "buffer
        // low" state after the next pop. Reset the `buffer_low` flag first so that we can catch
        // this transition. For example, `npc_papa_mupyokakkoee` will hang without this.
        if self.buffer_low && !state.buffer_low() {
            self.buffer_low = false;
        }

        let num_samples = state.pop_samples::<F>(data);
        state.next_frame += (num_samples / state.channels) as u64;

        if state.status == Status::Finished {
            trace!("Sending notification: {:?}", StreamNotification::Finished);
            self.notify_send.send(StreamNotification::Finished).unwrap();
        } else if !self.buffer_low && state.buffer_low() {
            trace!("Sending command: {:?}", StreamCommand::FillBuffer);
            self.command_send.send(StreamCommand::FillBuffer).unwrap();
            self.buffer_low = true;
        }

        drop(state);
        data[num_samples..].fill(F::Data::default());
    }
}

/// The main handle to an audio playback stream.
pub struct PlaybackStream {
    /// A reference to the shared playback state.
    state: Arc<Mutex<PlaybackState>>,
    /// The internal output stream.
    output: Option<cpal::Stream>,
    /// A sender for sending stream commands.
    command_send: Sender<StreamCommand>,
    /// A sender for sending stream notifications.
    notify_send: Sender<StreamNotification>,
    /// The receiver for stream notifications.
    notify_recv: Receiver<StreamNotification>,
    /// If not `None`, the handle to the thread which manages the buffer.
    buffer_thread: Option<JoinHandle<()>>,
    /// `true` if the stream has received a `Ready` notification.
    ready: bool,
    /// `true` if the stream has stopped or received a `Finished` notification.
    done: bool,
}

impl PlaybackStream {
    fn new(
        reader: Box<dyn ReadSamples<'static, Format = PcmF32Le>>,
        channels: usize,
        sample_rate: u32,
        volume: f64,
    ) -> Self {
        let (command_send, command_recv) = mpsc::channel();
        let (notify_send, notify_recv) = mpsc::channel();
        let mut stream = Self {
            state: Arc::new(Mutex::new(PlaybackState::new(channels, sample_rate, volume))),
            output: None,
            command_send,
            notify_send: notify_send.clone(),
            notify_recv,
            buffer_thread: None,
            ready: false,
            done: false,
        };
        stream.buffer_thread = Some({
            let mut buffer = BufferThread {
                state: Arc::clone(&stream.state),
                reader,
                command_recv,
                notify_send,
                eof: false,
            };
            thread::spawn(move || buffer.run())
        });
        stream
    }

    /// Runs the stream until it is ready for playback to begin.
    fn run_until_buffered(&mut self) {
        self.run_until(|s| s.done || s.ready)
    }

    /// Receives and processes stream notifications until music finishes playing.
    pub fn run_until_finished(&mut self) {
        self.run_until(|s| s.done)
    }

    /// Internal method which receives and processes notifications until a predicate becomes true.
    fn run_until(&mut self, predicate: impl Fn(&Self) -> bool) {
        while !predicate(self) {
            let notification = self.notify_recv.recv().unwrap();
            self.handle_notification(notification);
        }
    }

    /// Receives and processes one stream notification. This will block for no longer than
    /// `timeout`. Returns `true` if the stream should keep running.
    pub fn run_timeout(&mut self, timeout: Duration) -> bool {
        if self.done {
            return false;
        }
        let notification = match self.notify_recv.recv_timeout(timeout) {
            Ok(notification) => notification,
            Err(RecvTimeoutError::Timeout) => return true,
            Err(e) => panic!("{:#}", e),
        };
        self.handle_notification(notification);
        !self.done
    }

    fn handle_notification(&mut self, notification: StreamNotification) {
        trace!("Received notification: {:?}", notification);
        match notification {
            StreamNotification::Buffered => self.ready = true,
            StreamNotification::Finished => self.stop(),
        }
    }

    /// Returns a closure for filling an audio device's sample buffer.
    fn sample_callback<F>(&self) -> impl FnMut(&mut [F::Data], &OutputCallbackInfo)
    where
        F: PcmFormat,
        F::Data: cpal::Sample + Default + Scalable,
    {
        let mut callback = SampleCallback {
            state: Some(Arc::clone(&self.state)),
            command_send: self.command_send.clone(),
            notify_send: self.notify_send.clone(),
            buffer_low: false,
        };
        move |data, info| callback.write_samples::<F>(data, info)
    }

    /// Calculates the current playback position. Returns `None` if not known.
    fn position(&self) -> Option<Duration> {
        self.state.lock().unwrap().position()
    }

    /// Pauses audio playback at the current position.
    fn pause(&self) {
        self.state.lock().unwrap().pause();
    }

    /// Resumes audio playback after it was paused.
    fn unpause(&self) {
        self.state.lock().unwrap().unpause();
    }

    /// Retrieves the current volume scale.
    fn volume(&self) -> f64 {
        self.state.lock().unwrap().volume.volume()
    }

    /// Sets the volume scale to `volume`.
    fn set_volume(&self, volume: f64) {
        self.state.lock().unwrap().volume.set_volume(volume)
    }

    /// Stops the stream and waits for all threads to clean up.
    fn stop(&mut self) {
        if let Some(thread) = self.buffer_thread.take() {
            // The buffer thread can only stop if it receives the `Stop` command.
            self.send_command(StreamCommand::Stop);
            thread.join().unwrap();
            self.done = true;
        }
    }

    fn send_command(&self, command: StreamCommand) {
        trace!("Sending command: {:?}", command);
        self.command_send.send(command).unwrap();
    }
}

impl Drop for PlaybackStream {
    fn drop(&mut self) {
        // Do not let any threads dangle!
        self.stop();
    }
}

/// An output device capable of playing an audio stream.
pub struct PlaybackDevice {
    device: Device,
    config: SupportedStreamConfig,
}

impl PlaybackDevice {
    /// Opens the system default playback device, targeting (but not guaranteeing) a sample rate of `target_rate`.
    pub fn open_default(target_rate: u32) -> Result<Self> {
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(device) => device,
            None => bail!("no audio output device available"),
        };
        info!("Using audio output device: {}", device.name().unwrap_or_default());

        // Use the default config with a clamped sample rate
        // TODO: Sort configs by closest sample rate instead of using the cpal heuristics?
        let mut configs = match device.supported_output_configs() {
            Ok(c) => c.collect(),
            Err(_) => vec![],
        };
        if configs.is_empty() {
            bail!("no audio output configuration available");
        }
        configs.sort_unstable_by(|a, b| a.cmp_default_heuristics(b));
        let config = configs.pop().unwrap();
        let sample_rate = target_rate.clamp(config.min_sample_rate().0, config.max_sample_rate().0);
        let config = config.with_sample_rate(SampleRate(sample_rate));

        if config.channels() != 2 {
            bail!("only stereo audio devices are supported right now");
        }
        debug!(
            "Output configuration: {:?}, {} channels, {} Hz",
            config.sample_format(),
            config.channels(),
            config.sample_rate().0
        );

        Ok(Self { device, config })
    }

    /// Starts playing `source` and returns the stream that can be used to control it.
    pub fn play(&mut self, source: PlaybackSource) -> PlaybackStream {
        match self.config.sample_format() {
            cpal::SampleFormat::I16 => self.play_impl::<PcmS16Le>(source),
            cpal::SampleFormat::U16 => self.play_impl::<PcmU16Le>(source),
            cpal::SampleFormat::F32 => self.play_impl::<PcmF32Le>(source),
        }
    }

    fn play_impl<F>(&mut self, source: PlaybackSource) -> PlaybackStream
    where
        F: PcmFormat + ScaleAmplitude + Cast<AnyFormat>,
        F::Data: cpal::Sample + Default + Scalable,
    {
        let channels = self.config.channels() as usize;
        let sample_rate = self.config.sample_rate().0;
        let volume = source.volume;
        let reader = source.into_reader(channels, sample_rate);
        let mut stream = PlaybackStream::new(reader, channels, sample_rate, volume);
        stream.run_until_buffered();
        let output = self
            .device
            .build_output_stream(
                &self.config.clone().into(),
                stream.sample_callback::<F>(),
                |error| error!("Playback error: {:#}", error),
            )
            .expect("failed to create output stream");
        output.play().unwrap();
        stream.output = Some(output);
        stream
    }
}

struct UiController {
    stream: PlaybackStream,
    duration: Duration,
    name: String,
}

impl PlaybackController for UiController {
    fn update(&mut self) -> bool {
        self.stream.run_timeout(UI_UPDATE_INTERVAL)
    }
    fn stop(&mut self) {
        self.stream.stop();
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn duration(&self) -> Duration {
        self.duration
    }
    fn position(&self) -> Option<Duration> {
        self.stream.position()
    }
    fn pause(&mut self) {
        self.stream.pause();
    }
    fn unpause(&mut self) {
        self.stream.unpause();
    }
    fn volume(&self) -> f64 {
        self.stream.volume()
    }
    fn set_volume(&self, volume: f64) {
        self.stream.set_volume(volume)
    }
}

pub fn play(device: &mut PlaybackDevice, source: PlaybackSource, name: String) {
    let duration = source.duration();
    let mut stream = device.play(source);
    if terminal::is_tty() {
        // In trace mode, we want to see trace logs, but key input should still work. Run the UI, but
        // set it to not actually display anything.
        let tracing = log_enabled!(Level::Trace);
        let visibility = if tracing { Visibility::Hidden } else { Visibility::Visible };
        PlaybackUi::new(UiController { stream, duration, name }, visibility).run();
    } else {
        stream.run_until_finished();
    }
}
