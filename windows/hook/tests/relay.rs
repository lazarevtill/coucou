//! Runs the real `coucou-hook.exe` against a private pipe and checks what comes out
//! the other end. Every test uses its own pipe name through `COUCOU_PIPE`, so none
//! of this can ever reach a live Coucou.
#![cfg(windows)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};

/// A one-shot pipe server: the returned channel yields the first line a client sends.
fn one_shot_server(pipe_name: &str) -> mpsc::Receiver<String> {
    let wide: Vec<u16> = format!(r"\\.\pipe\{pipe_name}").encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            1 << 16,
            1 << 16,
            0,
            None,
        )
    };
    assert!(!handle.is_invalid(), "could not create the test pipe");

    // HANDLE is not Send; the raw value is.
    let raw = handle.0 as usize;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let handle = HANDLE(raw as *mut _);
        if let Err(err) = ConnectNamedPipe(handle, None) {
            if err.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                return;
            }
        }
        let mut data = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut n = 0u32;
            if ReadFile(handle, Some(&mut buf), Some(&mut n), None).is_err() || n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n as usize]);
            if data.contains(&b'\n') {
                break;
            }
        }
        let _ = DisconnectNamedPipe(handle);
        let _ = CloseHandle(handle);
        let _ = tx.send(String::from_utf8_lossy(&data).trim().to_string());
    });
    rx
}

/// Runs the relay with a clean terminal environment plus `env`.
fn run_hook(pipe: &str, event: &str, stdin: &str, env: &[(&str, &str)]) -> (std::process::Output, Duration) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_coucou-hook"));
    cmd.arg(event)
        .env("COUCOU_PIPE", pipe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in ["TERM_PROGRAM", "GIT_ASKPASS", "WT_SESSION", "VSCODE_PID", "TERM_SESSION_ID", "CLAUDE_CODE_SSE_PORT"] {
        cmd.env_remove(var);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let started = Instant::now();
    let mut child = cmd.spawn().expect("the relay should start");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out, started.elapsed())
}

#[test]
fn the_relay_forwards_the_event_with_its_process_chain_and_a_path_free_host_hint() {
    let pipe = format!("coucou-test-{}-relay", std::process::id());
    let received = one_shot_server(&pipe);

    // A Cursor terminal, as measured on a live machine (user name replaced).
    let askpass = r"c:\Users\someone\AppData\Local\Programs\cursor\resources\app\extensions\git\dist\askpass.sh";
    let (out, _) = run_hook(
        &pipe,
        "UserPromptSubmit",
        r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","cwd":"C:\\proj","prompt":"hi"}"#,
        &[("TERM_PROGRAM", "vscode"), ("GIT_ASKPASS", askpass), ("CLAUDE_CODE_SSE_PORT", "36005")],
    );
    assert!(out.status.success());

    let line = received.recv_timeout(Duration::from_secs(10)).expect("the relay should have connected");
    let sent: Value = serde_json::from_str(&line).unwrap();

    assert_eq!(sent["session_id"], "s1");
    assert_eq!(sent["term_program"], "vscode");
    assert_eq!(sent["host_hint"], "cursor");
    assert_eq!(sent["ide_port"], "36005");
    assert!(sent.get("session_pid").is_none(), "the misnamed field must be gone");

    // The relay's parent is this test process, so the chain must start with it.
    let chain = sent["host_chain"].as_array().expect("host_chain must be an array");
    assert!(!chain.is_empty() && chain.len() <= 12, "unexpected chain length {}", chain.len());
    assert_eq!(chain[0]["pid"].as_u64(), Some(u64::from(std::process::id())));
    for entry in chain {
        let exe = entry["exe"].as_str().expect("exe must be a string");
        assert!(!exe.contains('\\') && !exe.contains('/'), "{exe:?} must be a file name, not a path");
        assert!(entry["pid"].is_u64());
        assert_eq!(entry.as_object().unwrap().len(), 2, "only pid and exe may be sent");
    }

    // Nothing that names the user may leave the relay.
    assert!(!line.contains("someone"), "the askpass path leaked into the payload: {line}");
}

#[test]
fn a_relay_with_nobody_listening_exits_quietly_and_fast() {
    // Claude Code must never be held up by Coucou being closed, chain walk included.
    let (out, took) = run_hook(
        &format!("coucou-test-{}-nobody", std::process::id()),
        "PreToolUse",
        r#"{"hook_event_name":"PreToolUse","session_id":"s2","cwd":"C:\\proj"}"#,
        &[],
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "stdout must stay empty so Claude Code is untouched");
    assert!(took < Duration::from_secs(3), "took {took:?}");
}
