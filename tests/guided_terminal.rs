use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use proof_lantern::{DisplayState, evaluate, load_project};
use std::{
    fs,
    io::{Read, Write},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static PTY_MUTEX: Mutex<()> = Mutex::new(());
const ANSWERS: &[u8] = b"Recipe Box\nSave a recipe and find it tomorrow.\n3\nAdd a recipe\nThe entered recipe appears.\nSave the recipe\nThe app reports it saved.\nReopen the recipe\nClose and reopen; the recipe remains.\n";

#[test]
fn real_terminal_previews_then_saves_a_loadable_unknown_journey() {
    exercise(b"save\n", true, true);
}

#[test]
fn real_terminal_cancels_without_creating_files() {
    exercise(b":cancel\n", false, false);
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
        let mut transcript = Vec::new();
        let mut buffer = [0; 4096];
        let mut answered = false;
        let mut confirmed = false;
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.raw_os_error() == Some(5) => break, // Unix PTY hangup.
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                    return;
                }
            };
            transcript.extend_from_slice(&buffer[..count]);
            let output = String::from_utf8_lossy(&transcript);
            let input = if !answered && output.contains("Project name [") {
                answered = true;
                Some(if preview_first { ANSWERS } else { final_input })
            } else if preview_first
                && !confirmed
                && output.contains("Choose save, edit, or cancel [cancel]: ")
            {
                if worker_config.exists() {
                    let _ = sender.send(Err("setup wrote before confirmation".into()));
                    return;
                }
                confirmed = true;
                Some(final_input)
            } else {
                None
            };
            if let Some(input) = input {
                if let Err(error) = writer.write_all(input).and_then(|()| writer.flush()) {
                    let _ = sender.send(Err(error.to_string()));
                    return;
                }
            }
        }
        let _ = sender.send(Ok(transcript));
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("guided setup did not finish within 10 seconds");
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
