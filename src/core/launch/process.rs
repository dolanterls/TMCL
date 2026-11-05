use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::core::config::models::ProcessPriority;
use crate::core::error::{Error, Result};
use crate::core::launch::command::CommandSpec;
use crate::core::launch::log::crash_report_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    GameStarted { pid: u32 },
    GameLog { stream: &'static str, line: String },
    GameExited { code: Option<i32> },
}

/// How much of stdout is held back for a launch that fails.
///
/// With `logs.game_debug` off, stdout is not written to the log file at all —
/// which is why a launch could fail and leave a zero-byte log behind, telling
/// nobody anything. The tail is enough to hold the exception that ended it.
const STDOUT_TAIL: usize = 200;

#[derive(Debug, Clone)]
pub struct GameRunResult {
    pub pid: u32,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub log_path: std::path::PathBuf,
    pub crash_report: Option<std::path::PathBuf>,
}

pub async fn run(spec: &CommandSpec, log_path: &Path) -> Result<GameRunResult> {
    run_with_events(spec, log_path, None).await
}

pub async fn run_with_events(
    spec: &CommandSpec,
    log_path: &Path,
    events_sender: Option<mpsc::UnboundedSender<GameEvent>>,
) -> Result<GameRunResult> {
    run_with_events_and_priority(spec, log_path, events_sender, ProcessPriority::Default).await
}

pub async fn run_with_events_and_priority(
    spec: &CommandSpec,
    log_path: &Path,
    events_sender: Option<mpsc::UnboundedSender<GameEvent>>,
    priority: ProcessPriority,
) -> Result<GameRunResult> {
    run_with_events_and_priority_and_logging(spec, log_path, events_sender, true, priority).await
}

pub async fn run_with_events_and_priority_and_logging(
    spec: &CommandSpec,
    log_path: &Path,
    events_sender: Option<mpsc::UnboundedSender<GameEvent>>,
    capture_debug_output: bool,
    priority: ProcessPriority,
) -> Result<GameRunResult> {
    if let Some(parent) = log_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(path) = std::env::var_os("TMCL_DUMP_COMMAND") {
        // A failed launch can exit before writing a single line, and then the
        // log file says nothing about what was asked of the JVM. This dumps the
        // exact program, arguments and classpath to a file instead.
        let _ = std::fs::write(path, describe_command(spec));
    }
    let mut child = Command::new(&spec.program)
        .args(&spec.args)
        .env("CLASSPATH", &spec.classpath)
        .current_dir(&spec.current_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| Error::Launch(format!("start Minecraft: {error}")))?;
    let pid = child
        .id()
        .ok_or_else(|| Error::Launch("Minecraft has no PID".into()))?;
    if let Err(error) = crate::platform::windows::set_process_priority(pid, priority) {
        let _ = child.kill().await;
        return Err(error);
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Launch("missing stdout pipe".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Launch("missing stderr pipe".into()))?;
    let (sender, mut receiver) = mpsc::unbounded_channel();
    spawn_reader("stdout", stdout, sender.clone());
    spawn_reader("stderr", stderr, sender);
    let start = Instant::now();
    let mut file = tokio::fs::File::create(log_path).await?;
    if let Some(sender) = &events_sender {
        let _ = sender.send(GameEvent::GameStarted { pid });
    }
    let mut crash_report = None;
    let mut status = None;
    let mut readers_closed = false;
    // Only filled when stdout is not being written line by line.
    let mut stdout_tail: VecDeque<String> = VecDeque::new();
    while status.is_none() || !readers_closed {
        tokio::select! {
            result = child.wait(), if status.is_none() => status = Some(result?),
            line = receiver.recv(), if !readers_closed => match line {
                Some((stream, line)) => {
                    if crash_report.is_none() {
                        crash_report = crash_report_path(&line);
                    }
                    if capture_debug_output || stream == "stderr" {
                        file.write_all(format!("[{stream}] {line}\n").as_bytes())
                            .await?;
                    } else if stream == "stdout" {
                        if stdout_tail.len() == STDOUT_TAIL {
                            stdout_tail.pop_front();
                        }
                        stdout_tail.push_back(line.clone());
                    }
                    // The live log page always mirrors stdout and stderr, even
                    // when verbose file logging is disabled. Otherwise a
                    // normally running game appears to produce no logs at all.
                    if let Some(sender) = &events_sender {
                        let _ = sender.send(GameEvent::GameLog { stream, line });
                    }
                }
                None => readers_closed = true,
            }
        }
    }
    let exit_code = status.expect("process status recorded").code();
    // A launch that ended badly has to say why. Everything the game wrote went
    // to stdout, so without this the file would be empty exactly when it is
    // needed.
    if !matches!(exit_code, Some(0)) && !stdout_tail.is_empty() {
        let code = exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "no exit code".into());
        file.write_all(
            format!(
                "# the game exited with {code}; its last {} stdout line(s) follow, \
                 captured because logs.game_debug is off\n",
                stdout_tail.len()
            )
            .as_bytes(),
        )
        .await?;
        for line in stdout_tail {
            file.write_all(format!("[stdout] {line}\n").as_bytes())
                .await?;
        }
    }
    file.flush().await?;
    let exited = GameEvent::GameExited { code: exit_code };
    if let Some(sender) = events_sender {
        let _ = sender.send(exited);
    }
    Ok(GameRunResult {
        pid,
        exit_code,
        duration: start.elapsed(),
        log_path: log_path.to_path_buf(),
        crash_report,
    })
}

/// Renders a [`CommandSpec`] for a debug dump, with the access token masked.
fn describe_command(spec: &CommandSpec) -> String {
    use std::fmt::Write;

    let mut text = String::new();
    let _ = writeln!(text, "program: {}", spec.program.display());
    let _ = writeln!(text, "cwd: {}", spec.current_dir.display());
    let _ = writeln!(text, "classpath:");
    for entry in spec.classpath.split(';') {
        let _ = writeln!(text, "  {entry}");
    }
    let _ = writeln!(text, "args:");
    let mut mask_next = false;
    for argument in &spec.args {
        let value = if mask_next { "<redacted>" } else { argument };
        let _ = writeln!(text, "  {value}");
        mask_next = argument == "--accessToken";
    }
    text
}

fn spawn_reader<R>(
    stream: &'static str,
    reader: R,
    sender: mpsc::UnboundedSender<(&'static str, String)>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            // Read bytes and decode them leniently. `lines()` fails on the
            // first byte sequence that is not UTF-8, and a JVM on a Chinese
            // Windows writes its messages in GBK - so the strict decoder
            // stopped at the first line and threw the whole log away, leaving
            // an empty file behind a bare `exit code 1`.
            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    while matches!(buffer.last(), Some(b'\n' | b'\r')) {
                        buffer.pop();
                    }
                    let line = String::from_utf8_lossy(&buffer).into_owned();
                    if sender.send((stream, line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{run_with_events, run_with_events_and_priority_and_logging};
    use crate::core::launch::command::CommandSpec;

    #[tokio::test]
    async fn keeps_log_lines_that_are_not_utf8() {
        // A Chinese Windows JVM writes GBK. The reader used to stop at the
        // first such line, which is why a failed launch left an empty log.
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        super::spawn_reader(
            "stderr",
            &b"\xd5\xd2\xb2\xbb\xb5\xbd\xd6\xf7\xc0\xe0 main class\nplain\n"[..],
            sender,
        );

        let first = receiver.recv().await.expect("first line");
        assert_eq!(first.0, "stderr");
        assert!(first.1.ends_with("main class"), "{:?}", first.1);
        let second = receiver.recv().await.expect("second line");
        assert_eq!(second.1, "plain");
        assert!(receiver.recv().await.is_none(), "reader should stop at EOF");
    }

    #[tokio::test]
    async fn merges_stdout_and_stderr_and_records_exit() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".into(),
                vec![
                    "/C".into(),
                    "echo %CLASSPATH% & echo out & echo err 1>&2".into(),
                ],
            )
        } else {
            (
                "sh".into(),
                vec![
                    "-c".into(),
                    "echo $CLASSPATH; echo out; echo err >&2".into(),
                ],
            )
        };
        let root = std::env::temp_dir().join(format!("tmcl-process-{}", uuid::Uuid::new_v4()));
        let log = root.join("game.log");
        let (events_sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        let result = run_with_events(
            &CommandSpec {
                program,
                args,
                classpath: "tmcl-test-classpath".into(),
                current_dir: root.clone(),
            },
            &log,
            Some(events_sender),
        )
        .await
        .expect("run process");
        assert_eq!(result.exit_code, Some(0));
        let mut received = Vec::new();
        while let Ok(event) = events.try_recv() {
            received.push(event);
        }
        assert!(received.iter().any(|event| matches!(
            event,
            super::GameEvent::GameLog {
                stream: "stdout",
                ..
            }
        )));
        assert!(received.iter().any(|event| matches!(
            event,
            super::GameEvent::GameLog {
                stream: "stderr",
                ..
            }
        )));
        assert!(matches!(
            received.first(),
            Some(super::GameEvent::GameStarted { .. })
        ));
        assert!(matches!(
            received.last(),
            Some(super::GameEvent::GameExited { code: Some(0) })
        ));
        assert!(
            std::fs::read_to_string(log)
                .expect("read log")
                .contains("tmcl-test-classpath")
        );
        std::fs::remove_dir_all(root).expect("clean process test");
    }

    #[tokio::test]
    async fn debug_output_switch_controls_stdout_capture_but_keeps_stderr() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".into(),
                vec!["/C".into(), "echo debug-out & echo error-out 1>&2".into()],
            )
        } else {
            (
                "sh".into(),
                vec!["-c".into(), "echo debug-out; echo error-out >&2".into()],
            )
        };
        let root =
            std::env::temp_dir().join(format!("tmcl-process-debug-{}", uuid::Uuid::new_v4()));
        let log = root.join("game.log");
        let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
        run_with_events_and_priority_and_logging(
            &CommandSpec {
                program,
                args,
                classpath: String::new(),
                current_dir: root.clone(),
            },
            &log,
            Some(sender),
            false,
            crate::core::config::models::ProcessPriority::Default,
        )
        .await
        .expect("run process");
        let output = std::fs::read_to_string(log).expect("read log");
        assert!(!output.contains("debug-out"));
        assert!(output.contains("error-out"));
        let mut live = Vec::new();
        while let Ok(event) = events.try_recv() {
            live.push(event);
        }
        assert!(live.iter().any(|event| matches!(
            event,
            super::GameEvent::GameLog {
                stream: "stdout",
                ..
            }
        )));
        assert!(live.iter().any(|event| matches!(
            event,
            super::GameEvent::GameLog {
                stream: "stderr",
                ..
            }
        )));
        std::fs::remove_dir_all(root).expect("clean process debug test");
    }

    /// A launch that fails has to leave its stdout behind: with
    /// `logs.game_debug` off the file used to be empty exactly when it was
    /// needed.
    #[tokio::test]
    async fn a_failed_launch_keeps_the_stdout_tail() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".into(),
                vec!["/C".into(), "echo boom-on-stdout & exit /b 3".into()],
            )
        } else {
            (
                "sh".into(),
                vec!["-c".into(), "echo boom-on-stdout; exit 3".into()],
            )
        };
        let root = std::env::temp_dir().join(format!("tmcl-process-fail-{}", uuid::Uuid::new_v4()));
        let log = root.join("game.log");
        let result = run_with_events_and_priority_and_logging(
            &CommandSpec {
                program,
                args,
                classpath: String::new(),
                current_dir: root.clone(),
            },
            &log,
            None,
            false,
            crate::core::config::models::ProcessPriority::Default,
        )
        .await
        .expect("run process");

        assert_eq!(result.exit_code, Some(3));
        let text = std::fs::read_to_string(&log).expect("read log");
        assert!(text.contains("boom-on-stdout"), "{text}");
        assert!(text.contains("exited with 3"), "{text}");
        std::fs::remove_dir_all(root).expect("clean failed launch test");
    }

    #[tokio::test]
    async fn a_successful_launch_writes_no_stdout_to_the_log() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".into(),
                vec!["/C".into(), "echo harmless & exit /b 0".into()],
            )
        } else {
            (
                "sh".into(),
                vec!["-c".into(), "echo harmless; exit 0".into()],
            )
        };
        let root = std::env::temp_dir().join(format!("tmcl-process-ok-{}", uuid::Uuid::new_v4()));
        let log = root.join("game.log");
        let result = run_with_events_and_priority_and_logging(
            &CommandSpec {
                program,
                args,
                classpath: String::new(),
                current_dir: root.clone(),
            },
            &log,
            None,
            false,
            crate::core::config::models::ProcessPriority::Default,
        )
        .await
        .expect("run process");

        assert_eq!(result.exit_code, Some(0));
        let text = std::fs::read_to_string(&log).expect("read log");
        assert!(text.is_empty(), "a normal exit stays quiet: {text}");
        std::fs::remove_dir_all(root).expect("clean successful launch test");
    }
}
