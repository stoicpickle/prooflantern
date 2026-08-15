#![cfg(feature = "terminal-test-hooks")]

use std::{
    io::{self, Read, Write},
    sync::{Mutex, MutexGuard, mpsc},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};

const ENTER_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049h";
const LEAVE_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
const PRIVATE_CURSOR_POSITION_QUERY: &[u8] = b"\x1b[?6n";
const CURSOR_POSITION_RESPONSE: &[u8] = b"\x1b[1;1R";
const PRIVATE_CURSOR_POSITION_RESPONSE: &[u8] = b"\x1b[?1;1R";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(5);
static PTY_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug)]
enum ExitPath {
    InjectedNormal,
    InjectedError,
    InjectedPanic,
    QuitKey,
    ControlCKey,
}

impl ExitPath {
    const fn input(self) -> Option<&'static [u8]> {
        match self {
            Self::QuitKey => Some(b"q"),
            Self::ControlCKey => Some(b"\x03"),
            Self::InjectedNormal | Self::InjectedError | Self::InjectedPanic => None,
        }
    }
}

#[test]
fn injected_normal_exit_restores_the_terminal() {
    let _guard = serialize_pty_test();
    let outcome = run_in_pty(ExitPath::InjectedNormal);
    assert!(outcome.status.success(), "{:?}", outcome.status);
    assert_restored(&outcome.output);
}

#[test]
fn q_key_quits_the_real_event_loop_and_restores_the_terminal() {
    let _guard = serialize_pty_test();
    let outcome = run_in_pty(ExitPath::QuitKey);
    assert!(outcome.status.success(), "{:?}", outcome.status);
    assert_restored(&outcome.output);
}

#[test]
fn raw_control_c_quits_the_real_event_loop_and_restores_the_terminal() {
    let _guard = serialize_pty_test();
    let outcome = run_in_pty(ExitPath::ControlCKey);
    assert!(outcome.status.success(), "{:?}", outcome.status);
    assert_restored(&outcome.output);
}

#[test]
fn injected_error_restores_before_reporting_failure() {
    let _guard = serialize_pty_test();
    let outcome = run_in_pty(ExitPath::InjectedError);
    assert!(!outcome.status.success(), "{:?}", outcome.status);
    assert_restored(&outcome.output);
    assert_occurs_before(
        &outcome.output,
        LEAVE_ALTERNATE_SCREEN,
        b"proof-lantern: injected terminal failure",
    );
}

#[test]
fn injected_panic_restores_before_reporting_failure() {
    let _guard = serialize_pty_test();
    let outcome = run_in_pty(ExitPath::InjectedPanic);
    assert!(!outcome.status.success(), "{:?}", outcome.status);
    assert_restored(&outcome.output);
    assert_occurs_before(
        &outcome.output,
        LEAVE_ALTERNATE_SCREEN,
        b"injected terminal panic",
    );
}

struct PtyOutcome {
    status: ExitStatus,
    output: Vec<u8>,
}

fn serialize_pty_test() -> MutexGuard<'static, ()> {
    PTY_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run_in_pty(exit_path: ExitPath) -> PtyOutcome {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: 140,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("test PTY should open");

    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_proof-lantern"));
    command.arg("demo");
    command.env("TERM", "xterm-256color");
    command.env("RUST_BACKTRACE", "0");
    match exit_path {
        ExitPath::InjectedNormal => command.env("PROOF_LANTERN_TEST_TERMINAL_EXIT", "ok"),
        ExitPath::InjectedError => command.env("PROOF_LANTERN_TEST_TERMINAL_EXIT", "error"),
        ExitPath::InjectedPanic => command.env("PROOF_LANTERN_TEST_TERMINAL_EXIT", "panic"),
        ExitPath::QuitKey | ExitPath::ControlCKey => {
            command.env_remove("PROOF_LANTERN_TEST_TERMINAL_EXIT");
        }
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("Proof Lantern should spawn inside the test PTY");
    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("PTY output reader should clone");
    let mut writer = pair
        .master
        .take_writer()
        .expect("PTY input writer should open");
    let (output_sender, output_receiver) = mpsc::sync_channel(1);
    let input = exit_path.input();
    thread::spawn(move || {
        let result = read_terminal_output(&mut reader, &mut writer, input);
        let _ = output_sender.send(result);
    });
    drop(pair.slave);

    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("child status should be readable") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("timed-out child should terminate");
            let _ = child.wait();
            panic!("Proof Lantern did not exit after {PROCESS_TIMEOUT:?} on {exit_path:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };

    drop(pair.master);
    let output = output_receiver
        .recv_timeout(OUTPUT_TIMEOUT)
        .expect("PTY output should close after the child exits")
        .expect("PTY output should remain readable");
    PtyOutcome { status, output }
}

fn read_terminal_output(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    input: Option<&[u8]>,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut cursor_position_answered = false;
    let mut input_sent = false;
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(output);
        }
        output.extend_from_slice(&buffer[..bytes_read]);
        let cursor_position_response = (!cursor_position_answered)
            .then(|| {
                if contains_bytes(&output, CURSOR_POSITION_QUERY) {
                    Some(CURSOR_POSITION_RESPONSE)
                } else if contains_bytes(&output, PRIVATE_CURSOR_POSITION_QUERY) {
                    Some(PRIVATE_CURSOR_POSITION_RESPONSE)
                } else {
                    None
                }
            })
            .flatten();
        if let Some(response) = cursor_position_response {
            writer.write_all(response)?;
            writer.flush()?;
            cursor_position_answered = true;
        }
        if !input_sent && contains_bytes(&output, ENTER_ALTERNATE_SCREEN) {
            if let Some(input) = input {
                writer.write_all(input)?;
                writer.flush()?;
            }
            input_sent = true;
        }
    }
}

fn assert_restored(output: &[u8]) {
    assert_last_occurs_before(output, ENTER_ALTERNATE_SCREEN, LEAVE_ALTERNATE_SCREEN);
    assert_last_occurs_before(output, HIDE_CURSOR, SHOW_CURSOR);
    assert_last_occurs_before(output, LEAVE_ALTERNATE_SCREEN, SHOW_CURSOR);
}

fn assert_occurs_before(output: &[u8], first: &[u8], second: &[u8]) {
    let first_index = find_bytes(output, first);
    let second_search_start = first_index + first.len();
    let _second_index = second_search_start + find_bytes(&output[second_search_start..], second);
}

fn assert_last_occurs_before(output: &[u8], first: &[u8], second: &[u8]) {
    let first_index = find_last_bytes(output, first);
    let second_index = find_last_bytes(output, second);
    assert!(
        first_index < second_index,
        "final {first:?} must occur before final {second:?}"
    );
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| {
            panic!(
                "missing {needle:?}; tail={:?}",
                String::from_utf8_lossy(&haystack[haystack.len().saturating_sub(500)..])
            )
        })
}

fn find_last_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
        .unwrap_or_else(|| {
            panic!(
                "missing {needle:?}; tail={:?}",
                String::from_utf8_lossy(&haystack[haystack.len().saturating_sub(500)..])
            )
        })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
