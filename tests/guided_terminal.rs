use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use proof_lantern::{DisplayState, evaluate, load_project};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static PTY_MUTEX: Mutex<()> = Mutex::new(());
// PTY input represents keypresses: Enter is CR, not a text-file LF.
const ANSWERS: &[u8] = b"Recipe Box\rSave a recipe and find it tomorrow.\r3\rAdd a recipe\rThe entered recipe appears.\rSave the recipe\rThe app reports it saved.\rReopen the recipe\rClose and reopen; the recipe remains.\r";

#[test]
fn real_terminal_previews_then_saves_a_loadable_unknown_journey() {
    exercise(b"save\r", true, true);
}

#[test]
fn real_terminal_cancels_without_creating_files() {
    exercise(b":cancel\r", false, false);
}

#[cfg(unix)]
#[test]
fn cooked_control_c_exits_without_creating_files() {
    exercise(b"\x03", false, false);
}

#[cfg(unix)]
#[test]
fn cooked_eof_cancels_without_creating_files() {
    exercise(b"\x04", false, false);
}

fn exercise(final_input: &'static [u8], preview_first: bool, should_save: bool) {
    let _guard = PTY_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "proof-lantern-guided-pty-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 40,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_proof-lantern"));
    command.args(["init", "--guided"]);
    command.arg(&root);
    command.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    let config = root.join(".proof-lantern");
    let worker_config = config.clone();
    thread::spawn(move || {
        let result = drive_guided_setup(
            &mut reader,
            &mut writer,
            &worker_config,
            final_input,
            preview_first,
        );
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            drop(pair.master);
            let output = receiver.recv_timeout(Duration::from_secs(5));
            let detail = match &output {
                Ok(Ok(bytes)) => format!(
                    "{:?}",
                    String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(1500)..])
                ),
                other => format!("{other:?}"),
            };
            panic!("guided setup did not finish within 10 seconds; transcript tail: {detail}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(pair.master);
    let output = String::from_utf8(
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(
        !output.contains("\x1b[?1049h"),
        "guided setup must not enter raw alternate-screen UI"
    );
    if should_save {
        assert!(status.success(), "{output}");
        assert!(
            output.contains("YOUR JOURNEY / PREVIEW (not saved)"),
            "{output}"
        );
        assert!(
            output.contains("proof-lantern explain step-1 ."),
            "{output}"
        );
        assert!(
            output.contains("proof-lantern record step-1 passed"),
            "{output}"
        );
        let (spec, observations) = load_project(&root).unwrap();
        assert_eq!(spec.capabilities.len(), 3);
        assert!(
            evaluate(spec, observations)
                .unwrap()
                .capabilities
                .iter()
                .all(|c| c.display == DisplayState::Unknown)
        );
    } else {
        assert!(!config.exists(), "{output}");
        if final_input != b"\x03" {
            assert!(status.success(), "{output}");
        }
    }
}

fn drive_guided_setup(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    config: &Path,
    final_input: &[u8],
    preview_first: bool,
) -> Result<Vec<u8>, String> {
    let mut transcript = Vec::new();
    let mut buffer = [0; 4096];
    let mut answered = false;
    let mut confirmed = false;
    let mut cursor_answered = false;
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if error.kind() == io::ErrorKind::BrokenPipe
                    || (cfg!(unix) && error.raw_os_error() == Some(5)) =>
            {
                break;
            }
            Err(error) => return Err(error.to_string()),
        };
        transcript.extend_from_slice(&buffer[..count]);
        let output = String::from_utf8_lossy(&transcript);
        // ConPTY inherits the cursor and can wait for this response before
        // emitting application output. Accumulation handles split sequences.
        if !cursor_answered {
            let response: Option<&[u8]> = if output.contains("\x1b[6n") {
                Some(b"\x1b[1;1R")
            } else if output.contains("\x1b[?6n") {
                Some(b"\x1b[?1;1R")
            } else {
                None
            };
            if let Some(response) = response {
                writer
                    .write_all(response)
                    .and_then(|()| writer.flush())
                    .map_err(|error| error.to_string())?;
                cursor_answered = true;
            }
        }
        let input = if !answered && output.contains("Project name [") {
            answered = true;
            Some(if preview_first { ANSWERS } else { final_input })
        } else if preview_first
            && !confirmed
            && output.contains("Choose save, edit, or cancel [cancel]: ")
        {
            if config.exists() {
                return Err("setup wrote before confirmation".into());
            }
            confirmed = true;
            Some(final_input)
        } else {
            None
        };
        if let Some(input) = input {
            writer
                .write_all(input)
                .and_then(|()| writer.flush())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(transcript)
}

// A scripted peer blocks progress unless the real driver writes the expected
// response. This reproduces ConPTY's protocol gate without needing Windows.
#[derive(Clone, Default)]
struct CapturedInput(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl Write for CapturedInput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ScriptedPeer {
    input: CapturedInput,
    steps: std::collections::VecDeque<(Vec<u8>, Vec<u8>)>,
    end: io::ErrorKind,
}

impl Read for ScriptedPeer {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some((expected, output)) = self.steps.pop_front() else {
            return Err(self.end.into());
        };
        if *self.input.0.borrow() != expected {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "peer is waiting for terminal input",
            ));
        }
        buffer[..output.len()].copy_from_slice(&output);
        Ok(output.len())
    }
}

#[test]
fn protocol_answers_fragmented_cursor_queries_before_application_input() {
    for (query, response) in [
        (b"\x1b[6n".as_slice(), b"\x1b[1;1R".as_slice()),
        (b"\x1b[?6n".as_slice(), b"\x1b[?1;1R".as_slice()),
    ] {
        for split in 1..query.len() {
            let mut writer = CapturedInput::default();
            let mut peer = ScriptedPeer {
                input: writer.clone(),
                steps: [
                    (vec![], query[..split].to_vec()),
                    (vec![], query[split..].to_vec()),
                    (response.to_vec(), b"Project name [test]: ".to_vec()),
                    (
                        [response, b":cancel\r"].concat(),
                        b"Nothing was saved.".to_vec(),
                    ),
                ]
                .into(),
                end: io::ErrorKind::BrokenPipe,
            };
            let result = drive_guided_setup(
                &mut peer,
                &mut writer,
                Path::new("unused"),
                b":cancel\r",
                false,
            );
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(*writer.0.borrow(), [response, b":cancel\r"].concat());
        }
    }
}

#[test]
fn protocol_hangup_is_eof_but_unexpected_read_errors_still_fail() {
    for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::PermissionDenied] {
        let mut writer = CapturedInput::default();
        let mut peer = ScriptedPeer {
            input: writer.clone(),
            steps: [].into(),
            end: kind,
        };
        let result = drive_guided_setup(
            &mut peer,
            &mut writer,
            Path::new("unused"),
            b":cancel\r",
            false,
        );
        assert_eq!(
            result.is_ok(),
            kind == io::ErrorKind::BrokenPipe,
            "{result:?}"
        );
    }
}
