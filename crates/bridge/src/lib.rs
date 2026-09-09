#![deny(unsafe_op_in_unsafe_fn)]

use ctab_web_protocol::{BridgeFrame, BridgeMessage, Envelope, MAX_FRAME_BYTES, TacticalMessage};
use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_int};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, GetCurrentProcessId};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const QUEUE_CAPACITY: usize = 128;
const PIPE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RESTART_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESTART_WINDOW: Duration = Duration::from_secs(60);
const RESTART_COOLDOWN: Duration = Duration::from_secs(5);
const MAX_RESTARTS_PER_WINDOW: usize = 3;
const MAX_REPLAY_BYTES: usize = 8 * 1024 * 1024;
const MAX_REPLAY_MARKER_FRAMES: usize = 512;
const MAX_REPLAY_ENTITY_FRAMES: usize = 64;
const MAX_REPLAY_POSITION_FRAMES: usize = 16;
const MAX_EXTENSION_ARGUMENT_BYTES: usize = 256 * 1024;

static SERVICE: OnceLock<BridgeService> = OnceLock::new();

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("the bridge is not running")]
    NotRunning,
    #[error("the companion executable was not found at the trusted sibling path")]
    CompanionMissing,
    #[error("the bridge queue is full")]
    QueueFull,
    #[error("the bridge writer has stopped")]
    WriterStopped,
    #[error("the payload is invalid: {0}")]
    InvalidPayload(String),
    #[error("the payload exceeds the bridge frame limit")]
    FrameTooLarge,
    #[error("failed to launch the companion: {0}")]
    Launch(io::Error),
    #[error("failed to resolve the bridge module path")]
    ModulePath,
    #[error("the Windows cryptographic random source is unavailable: {0}")]
    RandomSource(String),
}

#[derive(Debug)]
pub struct BridgeService {
    sender: SyncSender<Vec<u8>>,
    pipe_token: String,
}

impl BridgeService {
    pub fn launch(companion_path: &Path, open_browser: bool) -> Result<Self, BridgeError> {
        validate_companion_path(companion_path)?;
        let pipe_token = random_hex(32)?;
        let pipe_name = format!(
            r"\\.\pipe\ctab-web-{}-{}",
            unsafe { GetCurrentProcessId() },
            random_hex(8)?
        );

        let companion_path = companion_path.to_owned();
        let arma_root = inherited_arma_root();
        let mut child = spawn_companion(
            &companion_path,
            &pipe_name,
            &pipe_token,
            open_browser,
            arma_root.as_deref(),
        )
        .map_err(BridgeError::Launch)?;

        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
        let writer_pipe_token = pipe_token.clone();
        thread::Builder::new()
            .name("ctab-web-pipe-writer".to_owned())
            .spawn(move || {
                let mut restart_limiter = RestartLimiter::default();
                let mut replay = ReplayBuffer::default();
                let mut replacement_tab_session: Option<String> = None;
                for payload in receiver {
                    let new_session = replay.observe(&payload);
                    if let Err(initial_error) =
                        write_payload_with_retry(&pipe_name, &payload, PIPE_CONNECT_TIMEOUT)
                    {
                        let companion_stopped = child.try_wait().ok().flatten().is_some();
                        if companion_stopped && restart_limiter.allow(Instant::now()) {
                            let current_session = replay.session_id.clone();
                            let browser_requested = serde_json::from_slice::<BridgeFrame>(&payload)
                                .ok()
                                .is_some_and(|frame| {
                                    matches!(frame.message, BridgeMessage::OpenBrowser {})
                                });
                            // The pending explicit request opens its own tab after replay.
                            let restart_browser = if browser_requested {
                                false
                            } else if new_session {
                                replacement_tab_session = None;
                                open_browser
                            } else if current_session.is_some()
                                && current_session != replacement_tab_session
                            {
                                replacement_tab_session = current_session;
                                open_browser
                            } else {
                                false
                            };
                            match spawn_companion(
                                &companion_path,
                                &pipe_name,
                                &writer_pipe_token,
                                restart_browser,
                                arma_root.as_deref(),
                            ) {
                                Ok(restarted) => {
                                    child = restarted;
                                    if let Err(error) = replay.replay(&pipe_name, &payload) {
                                        eprintln!("cTab Web bridge restart pipe error: {error}");
                                    }
                                }
                                Err(error) => eprintln!("cTab Web bridge restart failed: {error}"),
                            }
                        } else {
                            eprintln!("cTab Web bridge pipe error: {initial_error}");
                        }
                    }
                }
            })
            .map_err(BridgeError::Launch)?;

        Ok(Self { sender, pipe_token })
    }

    pub fn enqueue(&self, envelope: Envelope) -> Result<(), BridgeError> {
        envelope
            .validate()
            .map_err(|error| BridgeError::InvalidPayload(error.to_string()))?;
        self.enqueue_message(BridgeMessage::Publish { envelope })
    }

    pub fn request_browser_open(&self) -> Result<(), BridgeError> {
        self.enqueue_message(BridgeMessage::OpenBrowser {})
    }

    fn enqueue_message(&self, message: BridgeMessage) -> Result<(), BridgeError> {
        let frame = BridgeFrame {
            pipe_token: self.pipe_token.clone(),
            message,
        };
        let payload = serde_json::to_vec(&frame)
            .map_err(|error| BridgeError::InvalidPayload(error.to_string()))?;
        if payload.len() > MAX_FRAME_BYTES {
            return Err(BridgeError::FrameTooLarge);
        }
        self.sender.try_send(payload).map_err(|error| match error {
            TrySendError::Full(_) => BridgeError::QueueFull,
            TrySendError::Disconnected(_) => BridgeError::WriterStopped,
        })
    }
}

fn spawn_companion(
    companion_path: &Path,
    pipe_name: &str,
    pipe_token: &str,
    open_browser: bool,
    arma_root: Option<&Path>,
) -> io::Result<std::process::Child> {
    let mut command = Command::new(companion_path);
    command
        .arg("--pipe-name")
        .arg(pipe_name)
        .arg("--pipe-token")
        .arg(pipe_token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(root) = arma_root {
        command.arg("--arma-root").arg(root);
    }
    if !open_browser {
        command.arg("--no-browser");
    }
    command.spawn()
}

#[derive(Debug, Default)]
struct RestartLimiter {
    attempts: VecDeque<Instant>,
}

#[derive(Clone, Debug)]
struct ReplayFrame {
    sequence: u64,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
struct ReplayBuffer {
    session_id: Option<String>,
    snapshot: Option<ReplayFrame>,
    entities: VecDeque<ReplayFrame>,
    positions: VecDeque<ReplayFrame>,
    markers: VecDeque<ReplayFrame>,
    marker_bytes: usize,
}

impl ReplayBuffer {
    fn observe(&mut self, payload: &[u8]) -> bool {
        let Ok(frame) = serde_json::from_slice::<BridgeFrame>(payload) else {
            return false;
        };
        let BridgeMessage::Publish { envelope } = frame.message else {
            return false;
        };
        let mut new_session = false;
        let replay_frame = ReplayFrame {
            sequence: envelope.sequence,
            payload: payload.to_vec(),
        };
        match envelope.message {
            TacticalMessage::SessionSnapshot(_) => {
                new_session = self.session_id.as_deref() != Some(envelope.session_id.as_str());
                self.session_id = Some(envelope.session_id.clone());
                self.snapshot = Some(replay_frame);
                self.entities.clear();
                self.positions.clear();
                self.markers.clear();
                self.marker_bytes = 0;
            }
            TacticalMessage::EntityDelta(_) => {
                self.entities.push_back(replay_frame);
                while self.entities.len() > MAX_REPLAY_ENTITY_FRAMES {
                    self.entities.pop_front();
                }
            }
            TacticalMessage::PositionDelta(_) => {
                self.positions.push_back(replay_frame);
                while self.positions.len() > MAX_REPLAY_POSITION_FRAMES {
                    self.positions.pop_front();
                }
            }
            TacticalMessage::MarkerDelta(_) => {
                self.marker_bytes = self.marker_bytes.saturating_add(replay_frame.payload.len());
                self.markers.push_back(replay_frame);
                while self.markers.len() > MAX_REPLAY_MARKER_FRAMES
                    || self.marker_bytes > MAX_REPLAY_BYTES
                {
                    if let Some(removed) = self.markers.pop_front() {
                        self.marker_bytes = self.marker_bytes.saturating_sub(removed.payload.len());
                    }
                }
            }
            TacticalMessage::Heartbeat(_) | TacticalMessage::Error(_) => {}
        }
        new_session
    }

    fn recovery_payloads<'a>(&'a self, pending: &'a [u8]) -> io::Result<Vec<&'a [u8]>> {
        let pending_frame: BridgeFrame = serde_json::from_slice(pending)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let browser_requested = matches!(pending_frame.message, BridgeMessage::OpenBrowser {});
        if self.snapshot.is_none() && !browser_requested {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "no mission snapshot is available for recovery",
            ));
        }
        let mut frames =
            Vec::with_capacity(self.markers.len() + self.entities.len() + self.positions.len() + 1);
        if let Some(snapshot) = self.snapshot.as_ref() {
            frames.push(snapshot);
            frames.extend(self.entities.iter());
            frames.extend(self.positions.iter());
            frames.extend(self.markers.iter());
        }
        frames.sort_by_key(|frame| frame.sequence);
        let mut payloads: Vec<&[u8]> = frames
            .iter()
            .map(|frame| frame.payload.as_slice())
            .collect();
        // Heartbeats and one-shot controls are not retained in the replay buffer.
        if matches!(
            pending_frame.message,
            BridgeMessage::OpenBrowser {}
                | BridgeMessage::Publish {
                    envelope: Envelope {
                        message: TacticalMessage::Heartbeat(_),
                        ..
                    }
                }
        ) {
            payloads.push(pending);
        }
        Ok(payloads)
    }

    fn replay(&self, pipe_name: &str, pending: &[u8]) -> io::Result<()> {
        for payload in self.recovery_payloads(pending)? {
            write_payload_with_retry(pipe_name, payload, RESTART_CONNECT_TIMEOUT)?;
        }
        Ok(())
    }
}

impl RestartLimiter {
    fn allow(&mut self, now: Instant) -> bool {
        while self
            .attempts
            .front()
            .is_some_and(|attempt| now.duration_since(*attempt) >= RESTART_WINDOW)
        {
            self.attempts.pop_front();
        }
        if self.attempts.len() >= MAX_RESTARTS_PER_WINDOW
            || self
                .attempts
                .back()
                .is_some_and(|attempt| now.duration_since(*attempt) < RESTART_COOLDOWN)
        {
            return false;
        }
        self.attempts.push_back(now);
        true
    }
}

fn inherited_arma_root() -> Option<PathBuf> {
    let mut candidates = Vec::with_capacity(2);
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.to_path_buf());
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.push(current);
    }
    for candidate in candidates {
        if candidate.join("arma3_x64.exe").is_file() && candidate.join("Addons").is_dir() {
            return candidate.canonicalize().ok();
        }
    }
    None
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn send_frame_to_pipe(
    pipe_name: &str,
    pipe_token: &str,
    envelope: Envelope,
    timeout: Duration,
) -> Result<(), BridgeError> {
    envelope
        .validate()
        .map_err(|error| BridgeError::InvalidPayload(error.to_string()))?;
    let payload = serde_json::to_vec(&BridgeFrame {
        pipe_token: pipe_token.to_owned(),
        message: BridgeMessage::Publish { envelope },
    })
    .map_err(|error| BridgeError::InvalidPayload(error.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(BridgeError::FrameTooLarge);
    }
    write_payload_with_retry(pipe_name, &payload, timeout)
        .map_err(|error| BridgeError::InvalidPayload(error.to_string()))
}

fn write_payload_with_retry(pipe_name: &str, payload: &[u8], timeout: Duration) -> io::Result<()> {
    let started = Instant::now();
    loop {
        match OpenOptions::new().write(true).open(pipe_name) {
            Ok(mut pipe) => return write_framed(&mut pipe, payload),
            Err(error)
                if started.elapsed() < timeout
                    && matches!(
                        error.kind(),
                        io::ErrorKind::NotFound
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::PermissionDenied
                    ) =>
            {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error),
        }
    }
}

fn write_framed(pipe: &mut File, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame is too large"))?;
    pipe.write_all(&length.to_le_bytes())?;
    pipe.write_all(payload)?;
    pipe.flush()
}

fn ensure_started() -> Result<&'static BridgeService, BridgeError> {
    if let Some(service) = SERVICE.get() {
        return Ok(service);
    }
    let companion = trusted_companion_path()?;
    let candidate = BridgeService::launch(&companion, true)?;
    let _ = SERVICE.set(candidate);
    SERVICE.get().ok_or(BridgeError::NotRunning)
}

fn trusted_companion_path() -> Result<PathBuf, BridgeError> {
    let module_path = current_module_path()?;
    let directory = module_path.parent().ok_or(BridgeError::ModulePath)?;
    Ok(directory.join("ctab-web-companion.exe"))
}

fn validate_companion_path(path: &Path) -> Result<(), BridgeError> {
    let expected_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("ctab-web-companion.exe"));
    if !expected_name || !path.is_file() {
        return Err(BridgeError::CompanionMissing);
    }
    Ok(())
}

fn current_module_path() -> Result<PathBuf, BridgeError> {
    let mut module: HMODULE = std::ptr::null_mut();
    let flags =
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    let address = current_module_path as *const () as *const u16;
    let success = unsafe { GetModuleHandleExW(flags, address, &mut module) };
    if success == 0 {
        return Err(BridgeError::ModulePath);
    }

    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        return Err(BridgeError::ModulePath);
    }
    buffer.truncate(length as usize);
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer)))
}

fn random_hex(bytes: usize) -> Result<String, BridgeError> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random).map_err(|error| BridgeError::RandomSource(error.to_string()))?;
    let mut output = String::with_capacity(bytes * 2);
    for byte in random {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(output)
}

fn dispatch(function: &str, arguments: &[String]) -> Result<String, BridgeError> {
    match function {
        "start" => {
            ensure_started()?;
            Ok(r#"{"ok":true,"status":"started"}"#.to_owned())
        }
        "publish" => {
            let encoded = arguments.first().ok_or_else(|| {
                BridgeError::InvalidPayload("publish requires one JSON envelope".to_owned())
            })?;
            if encoded.len() > MAX_EXTENSION_ARGUMENT_BYTES {
                return Err(BridgeError::FrameTooLarge);
            }
            let envelope: Envelope = serde_json::from_str(encoded)
                .map_err(|error| BridgeError::InvalidPayload(error.to_string()))?;
            ensure_started()?.enqueue(envelope)?;
            Ok(r#"{"ok":true,"status":"queued"}"#.to_owned())
        }
        "open_browser" => {
            ensure_started()?.request_browser_open()?;
            Ok(r#"{"ok":true,"status":"browser_open_requested"}"#.to_owned())
        }
        "version" => Ok(format!(r#"{{"ok":true,"version":"{VERSION}"}}"#)),
        _ => Err(BridgeError::InvalidPayload(
            "unsupported bridge function".to_owned(),
        )),
    }
}

fn dispatch_json(function: &str, arguments: &[String]) -> (String, c_int) {
    match dispatch(function, arguments) {
        Ok(response) => (response, 0),
        Err(error) => {
            let response = serde_json::json!({ "ok": false, "error": error.to_string() });
            (response.to_string(), 1)
        }
    }
}

fn decode_arma_string_argument(value: String) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\"\"", "\"")
    } else {
        value
    }
}

unsafe fn read_c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn write_output(output: *mut c_char, output_size: c_int, value: &str) {
    if output.is_null() || output_size <= 0 {
        return;
    }
    let capacity = output_size as usize;
    let bytes = value.as_bytes();
    let copy_length = bytes.len().min(capacity.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.cast::<u8>(), copy_length);
        *output.add(copy_length) = 0;
    }
}

/// Writes the extension version into an Arma-owned output buffer.
///
/// # Safety
///
/// Arma must provide a writable buffer of at least `output_size` bytes.
#[cfg(not(target_arch = "x86"))]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RVExtensionVersion(output: *mut c_char, output_size: c_int) {
    unsafe { rv_extension_version_impl(output, output_size) };
}

unsafe fn rv_extension_version_impl(output: *mut c_char, output_size: c_int) {
    unsafe { write_output(output, output_size, VERSION) };
}

/// Handles the legacy single-command Arma extension ABI.
///
/// # Safety
///
/// Arma must provide valid null-terminated input and a writable output buffer
/// of at least `output_size` bytes.
#[cfg(not(target_arch = "x86"))]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RVExtension(
    output: *mut c_char,
    output_size: c_int,
    function: *const c_char,
) {
    unsafe { rv_extension_impl(output, output_size, function) };
}

unsafe fn rv_extension_impl(output: *mut c_char, output_size: c_int, function: *const c_char) {
    let function = unsafe { read_c_string(function) };
    let (response, _) = dispatch_json(&function, &[]);
    unsafe { write_output(output, output_size, &response) };
}

/// Handles the argument-array Arma extension ABI.
///
/// # Safety
///
/// Arma must provide `argc` valid null-terminated argument pointers and a
/// writable output buffer of at least `output_size` bytes.
#[cfg(not(target_arch = "x86"))]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn RVExtensionArgs(
    output: *mut c_char,
    output_size: c_int,
    function: *const c_char,
    argv: *const *const c_char,
    argc: c_int,
) -> c_int {
    unsafe { rv_extension_args_impl(output, output_size, function, argv, argc) }
}

unsafe fn rv_extension_args_impl(
    output: *mut c_char,
    output_size: c_int,
    function: *const c_char,
    argv: *const *const c_char,
    argc: c_int,
) -> c_int {
    let function = unsafe { read_c_string(function) };
    let count = argc.max(0) as usize;
    let arguments = if argv.is_null() || count == 0 {
        Vec::new()
    } else {
        let pointers = unsafe { std::slice::from_raw_parts(argv, count.min(32)) };
        pointers
            .iter()
            .map(|pointer| decode_arma_string_argument(unsafe { read_c_string(*pointer) }))
            .collect()
    };
    let (response, code) = dispatch_json(&function, &arguments);
    unsafe { write_output(output, output_size, &response) };
    code
}

#[cfg(target_arch = "x86")]
#[unsafe(no_mangle)]
unsafe extern "C" fn ctab_web_rv_extension_version_impl(output: *mut c_char, output_size: c_int) {
    unsafe { rv_extension_version_impl(output, output_size) };
}

#[cfg(target_arch = "x86")]
#[unsafe(no_mangle)]
unsafe extern "C" fn ctab_web_rv_extension_impl(
    output: *mut c_char,
    output_size: c_int,
    function: *const c_char,
) {
    unsafe { rv_extension_impl(output, output_size, function) };
}

#[cfg(target_arch = "x86")]
#[unsafe(no_mangle)]
unsafe extern "C" fn ctab_web_rv_extension_args_impl(
    output: *mut c_char,
    output_size: c_int,
    function: *const c_char,
    argv: *const *const c_char,
    argc: c_int,
) -> c_int {
    unsafe { rv_extension_args_impl(output, output_size, function, argv, argc) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctab_web_protocol::synthetic_snapshot;
    use std::ffi::CString;

    #[test]
    fn version_call_is_bounded_and_null_terminated() {
        let mut output = [b'X' as c_char; 5];
        unsafe { rv_extension_version_impl(output.as_mut_ptr(), output.len() as c_int) };
        assert_eq!(output[4], 0);
        let value = unsafe { CStr::from_ptr(output.as_ptr()) };
        assert_eq!(value.to_bytes(), &VERSION.as_bytes()[..4]);
    }

    #[test]
    fn args_reject_unknown_function_without_panicking() {
        let function = CString::new("unknown").expect("static string");
        let mut output = [0 as c_char; 256];
        let code = unsafe {
            rv_extension_args_impl(
                output.as_mut_ptr(),
                output.len() as c_int,
                function.as_ptr(),
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(code, 1);
        let response = unsafe { CStr::from_ptr(output.as_ptr()) }.to_string_lossy();
        assert!(response.contains("unsupported bridge function"));
    }

    #[test]
    fn validates_the_exact_companion_filename() {
        let wrong = Path::new("not-the-companion.exe");
        assert!(matches!(
            validate_companion_path(wrong),
            Err(BridgeError::CompanionMissing)
        ));
    }

    #[test]
    fn dispatch_version_does_not_start_a_process() {
        let (response, code) = dispatch_json("version", &[]);
        assert_eq!(code, 0);
        assert!(response.contains(VERSION));
    }

    #[test]
    fn decodes_legacy_arma_string_arguments() {
        let encoded = r#""{""protocol_version"":1}""#.to_owned();
        assert_eq!(
            decode_arma_string_argument(encoded),
            r#"{"protocol_version":1}"#
        );
        assert_eq!(decode_arma_string_argument("plain".to_owned()), "plain");
    }

    #[test]
    fn enqueue_path_stays_below_the_five_millisecond_p99_budget() {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let service = BridgeService {
            sender,
            pipe_token: "a".repeat(64),
        };
        let drain = thread::spawn(move || while receiver.recv().is_ok() {});
        let mut samples = Vec::with_capacity(1_000);
        for sequence in 0..1_000 {
            let mut envelope = synthetic_snapshot();
            envelope.sequence = sequence;
            let started = Instant::now();
            service.enqueue(envelope).expect("enqueue snapshot");
            samples.push(started.elapsed());
        }
        drop(service);
        drain.join().expect("drain queue");
        samples.sort_unstable();
        let p99 = samples[989];
        eprintln!("Phase 1 bridge enqueue p99: {p99:?}");
        assert!(
            p99 < Duration::from_millis(5),
            "enqueue p99 exceeded 5 ms: {p99:?}"
        );
    }

    #[test]
    fn browser_open_request_uses_the_authenticated_control_channel() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let service = BridgeService {
            sender,
            pipe_token: "a".repeat(64),
        };

        service
            .request_browser_open()
            .expect("enqueue browser-open request");

        let payload = receiver.recv().expect("receive browser-open request");
        let frame: BridgeFrame =
            serde_json::from_slice(&payload).expect("decode browser-open request");
        assert_eq!(frame.pipe_token, "a".repeat(64));
        assert_eq!(frame.message, BridgeMessage::OpenBrowser {});
    }

    #[test]
    fn recovery_delivers_browser_request_once_after_the_tactical_state() {
        let mut replay = ReplayBuffer::default();
        let encode = |message| {
            serde_json::to_vec(&BridgeFrame {
                pipe_token: "a".repeat(64),
                message,
            })
            .expect("encode recovery frame")
        };
        let snapshot = encode(BridgeMessage::Publish {
            envelope: synthetic_snapshot(),
        });
        let mut delta = synthetic_snapshot();
        delta.sequence = 2;
        delta.message = TacticalMessage::MarkerDelta(ctab_web_protocol::MarkerDelta {
            updated: Vec::new(),
            removed: vec!["marker-objective".to_owned()],
        });
        let delta = encode(BridgeMessage::Publish { envelope: delta });
        let request = encode(BridgeMessage::OpenBrowser {});
        replay.observe(&snapshot);
        replay.observe(&delta);
        assert!(!replay.observe(&request));

        assert_eq!(
            replay
                .recovery_payloads(&request)
                .expect("recover browser request"),
            vec![snapshot.as_slice(), delta.as_slice(), request.as_slice()]
        );
        assert_eq!(
            replay
                .recovery_payloads(&delta)
                .expect("recover tactical delta"),
            vec![snapshot.as_slice(), delta.as_slice()]
        );

        let mut heartbeat = synthetic_snapshot();
        heartbeat.sequence = 3;
        heartbeat.message =
            TacticalMessage::Heartbeat(ctab_web_protocol::Heartbeat { uptime_ms: 42.0 });
        let heartbeat = encode(BridgeMessage::Publish {
            envelope: heartbeat,
        });
        replay.observe(&heartbeat);
        assert_eq!(
            replay
                .recovery_payloads(&heartbeat)
                .expect("recover heartbeat"),
            vec![snapshot.as_slice(), delta.as_slice(), heartbeat.as_slice()]
        );
    }

    #[test]
    fn browser_request_can_recover_before_a_snapshot_is_available() {
        let request = serde_json::to_vec(&BridgeFrame {
            pipe_token: "a".repeat(64),
            message: BridgeMessage::OpenBrowser {},
        })
        .expect("encode browser request");
        assert_eq!(
            ReplayBuffer::default()
                .recovery_payloads(&request)
                .expect("recover browser request"),
            vec![request.as_slice()]
        );
    }

    #[test]
    fn restart_limiter_prevents_crash_loops_and_recovers_after_the_window() {
        let start = Instant::now();
        let mut limiter = RestartLimiter::default();
        assert!(limiter.allow(start));
        assert!(!limiter.allow(start + Duration::from_secs(1)));
        assert!(limiter.allow(start + Duration::from_secs(5)));
        assert!(limiter.allow(start + Duration::from_secs(10)));
        assert!(!limiter.allow(start + Duration::from_secs(15)));
        assert!(limiter.allow(start + Duration::from_secs(61)));
    }

    #[test]
    fn replay_buffer_keeps_a_bounded_recovery_snapshot_and_skips_heartbeats() {
        let mut replay = ReplayBuffer::default();
        let snapshot = synthetic_snapshot();
        let snapshot_payload = serde_json::to_vec(&BridgeFrame {
            pipe_token: "a".repeat(64),
            message: BridgeMessage::Publish { envelope: snapshot },
        })
        .expect("encode snapshot frame");
        assert!(replay.observe(&snapshot_payload));

        let heartbeat_payload = serde_json::to_vec(&BridgeFrame {
            pipe_token: "a".repeat(64),
            message: BridgeMessage::Publish {
                envelope: Envelope {
                    protocol_version: ctab_web_protocol::PROTOCOL_VERSION,
                    session_id: "phase1-session".to_owned(),
                    sequence: 2,
                    message: TacticalMessage::Heartbeat(ctab_web_protocol::Heartbeat {
                        uptime_ms: 42.0,
                    }),
                },
            },
        })
        .expect("encode heartbeat frame");
        assert!(!replay.observe(&heartbeat_payload));

        let same_snapshot = replay
            .snapshot
            .as_ref()
            .expect("stored snapshot")
            .payload
            .clone();
        assert!(!replay.observe(&same_snapshot));

        let mut next_session = synthetic_snapshot();
        next_session.session_id = "phase1-session-next".to_owned();
        let next_payload = serde_json::to_vec(&BridgeFrame {
            pipe_token: "a".repeat(64),
            message: BridgeMessage::Publish {
                envelope: next_session,
            },
        })
        .expect("encode next-session frame");
        assert!(replay.observe(&next_payload));
        assert_eq!(replay.session_id.as_deref(), Some("phase1-session-next"));

        assert_eq!(
            replay.snapshot.as_ref().map(|frame| frame.sequence),
            Some(1)
        );
        assert!(replay.entities.is_empty());
        assert!(replay.positions.is_empty());
        assert!(replay.markers.is_empty());
        assert_eq!(replay.marker_bytes, 0);
    }
}
