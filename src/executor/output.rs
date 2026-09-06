use tokio::io::AsyncReadExt;
use tokio::process::Child;
use tokio_util::sync::CancellationToken;

use crate::config::ExecutorMode;
use crate::executor::process::kill_process_tree;
use crate::executor::ExecutorOutcome;

const MAX_CAPTURE_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_CHARS: usize = 2_000;

pub async fn run_spawned(
    mut child: Child,
    cancel: CancellationToken,
    mode: ExecutorMode,
) -> ExecutorOutcome {
    let stream_to_stdout = mode == ExecutorMode::Stream;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_buf = tokio::spawn(read_limited(stdout, stream_to_stdout));
    let err_buf = tokio::spawn(read_limited(stderr, stream_to_stdout));

    enum Finish {
        Status(std::io::Result<std::process::ExitStatus>),
        Cancelled,
    }

    let finish = tokio::select! {
        _ = cancel.cancelled() => {
            if let Some(pid) = child.id() {
                let _ = kill_process_tree(pid);
            }
            let _ = child.kill().await;
            Finish::Cancelled
        }
        status = child.wait() => Finish::Status(status),
    };

    if matches!(finish, Finish::Cancelled) {
        let _ = child.wait().await;
    }

    let stdout_bytes = out_buf.await.unwrap_or_default();
    let stderr_bytes = err_buf.await.unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let stdout = strip_reasoning(&stdout);
    let stderr = strip_reasoning(&stderr);

    match finish {
        Finish::Cancelled => ExecutorOutcome {
            exit_code: None,
            summary: "Executor was cancelled.".into(),
            tests_excerpt: extract_tests_excerpt(&stdout),
            cancelled: true,
            error: None,
        },
        Finish::Status(Ok(status)) => {
            let code = status.code();
            let crashed = code.is_none() && !status.success();
            let summary = sanitize_summary(&stdout, &stderr, code, crashed);
            let error = if crashed {
                Some("OpenCode process crashed (no exit code).".into())
            } else if code.unwrap_or(0) != 0 {
                Some(format!("OpenCode exited with code {}.", code.unwrap_or(-1)))
            } else {
                None
            };
            ExecutorOutcome {
                exit_code: code,
                summary,
                tests_excerpt: extract_tests_excerpt(&stdout),
                cancelled: false,
                error,
            }
        }
        Finish::Status(Err(err)) => ExecutorOutcome {
            exit_code: None,
            summary: format!("Executor crashed: {err}"),
            tests_excerpt: extract_tests_excerpt(&stdout),
            cancelled: false,
            error: Some(err.to_string()),
        },
    }
}

async fn read_limited<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    reader: Option<R>,
    stream_to_stdout: bool,
) -> Vec<u8> {
    let Some(mut reader) = reader else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut at_line_start = true;
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if stream_to_stdout {
                    use std::io::Write;
                    let mut stdout = std::io::stdout();
                    for line in String::from_utf8_lossy(&chunk[..n]).split_inclusive('\n') {
                        if at_line_start {
                            let _ = stdout.write_all(b"[executor] ");
                        }
                        let _ = stdout.write_all(line.as_bytes());
                        at_line_start = line.ends_with('\n');
                    }
                    if !String::from_utf8_lossy(&chunk[..n]).ends_with('\n') {
                        at_line_start = false;
                    }
                    let _ = std::io::stdout().flush();
                }
                if buf.len() < MAX_CAPTURE_BYTES {
                    let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
            Err(_) => break,
        }
    }
    buf
}

pub fn strip_reasoning(text: &str) -> String {
    let mut out = String::new();
    let mut in_think = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<think>") || trimmed.contains("<thinking>") {
            in_think = true;
        }
        if in_think {
            if trimmed.contains("</think>") || trimmed.contains("</thinking>") {
                in_think = false;
            }
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if is_reasoning_event(&value) {
                continue;
            }
            if let Some(text) = json_text(&value) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text);
                continue;
            }
        }
        if trimmed.eq_ignore_ascii_case("thinking")
            || trimmed.starts_with("thinking:")
            || trimmed.starts_with("Reasoning:")
        {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

fn is_reasoning_event(value: &serde_json::Value) -> bool {
    let ty = value
        .get("type")
        .or_else(|| value.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ty.as_str(),
        "thinking" | "reasoning" | "think" | "internal" | "thought"
    )
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.get("text").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = value.get("response").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = value
        .get("part")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    None
}

fn sanitize_summary(stdout: &str, stderr: &str, exit_code: Option<i32>, crashed: bool) -> String {
    if crashed {
        return truncate_chars(
            &format!(
                "OpenCode crashed. {}",
                stderr.trim().lines().next().unwrap_or("")
            ),
            MAX_SUMMARY_CHARS,
        );
    }
    let body = stdout.trim();
    if !body.is_empty() {
        return truncate_chars(body, MAX_SUMMARY_CHARS);
    }
    let err = stderr.trim();
    if !err.is_empty() {
        return truncate_chars(err, MAX_SUMMARY_CHARS);
    }
    match exit_code {
        Some(0) => "OpenCode finished successfully.".into(),
        Some(code) => format!("OpenCode exited with code {code}."),
        None => "OpenCode finished.".into(),
    }
}

pub fn extract_tests_excerpt(stdout: &str) -> Option<String> {
    let mut hits = Vec::new();
    for line in stdout.lines() {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        if lower.contains("test result:")
            || lower.contains("passed")
            || lower.contains("failed")
            || lower.contains("cargo test")
            || lower.contains("npm test")
            || lower.contains("pytest")
        {
            hits.push(l.to_string());
        }
        if hits.len() >= 8 {
            break;
        }
    }
    if hits.is_empty() {
        None
    } else {
        Some(hits.join("\n"))
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}