use std::process::Stdio;

use crate::config::ExecutorDefinition;
use crate::executor::antigravity::resolve_executable;
use crate::executor::process::{find_executable, parse_command};
use crate::executor::{ExecutorAvailability, ExecutorAvailabilityStatus};

pub fn scan_executor(definition: &ExecutorDefinition) -> ExecutorAvailability {
    // Antigravity must never be launched merely to discover it. Probing the
    // launcher pre-start (a `--version`/`--status` style invocation) starts or
    // queries the desktop app and fails with "can only be used if Antigravity
    // is already running", which also blocks normal startup. Availability is
    // therefore resolved by locating the executable only.
    if definition.kind.eq_ignore_ascii_case("antigravity") {
        return scan_executor_without_probe(definition);
    }

    let command = definition
        .executable
        .as_deref()
        .and_then(|path| path.to_str())
        .unwrap_or(&definition.command);
    let executable = find_executable(command);
    let (version, error, status) = match executable.as_deref() {
        Some(path) => match std::process::Command::new(path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .chain(String::from_utf8_lossy(&output.stderr).lines())
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(ToOwned::to_owned);
                if output.status.success() && text.is_some() {
                    (text, None, ExecutorAvailabilityStatus::Available)
                } else {
                    let reason = text.unwrap_or_else(|| {
                        format!("{} --version returned {}", path.display(), output.status)
                    });
                    (
                        None,
                        Some(reason),
                        ExecutorAvailabilityStatus::VersionProbeFailed,
                    )
                }
            }
            Err(err) => (
                None,
                Some(format!("{}: {err}", path.display())),
                ExecutorAvailabilityStatus::NotExecutable,
            ),
        },
        None => (
            None,
            Some(format!(
                "{} not found on PATH or at the configured path",
                definition.command
            )),
            ExecutorAvailabilityStatus::NotFound,
        ),
    };
    ExecutorAvailability {
        id: definition.id.clone(),
        available: status == ExecutorAvailabilityStatus::Available,
        executable,
        version,
        error,
        status,
    }
}

/// Availability for an executor that must not be spawned during discovery.
/// Only PATH/absolute-path resolution is performed; no child process is run.
fn scan_executor_without_probe(definition: &ExecutorDefinition) -> ExecutorAvailability {
    let program = match definition.executable.as_deref() {
        Some(path) => path.to_string_lossy().into_owned(),
        None => parse_command(&definition.command)
            .map(|(program, _)| program)
            .unwrap_or_else(|_| definition.command.clone()),
    };
    let executable = resolve_executable(&program);
    match executable {
        Some(path) => ExecutorAvailability {
            id: definition.id.clone(),
            available: true,
            executable: Some(path),
            version: None,
            error: None,
            status: ExecutorAvailabilityStatus::Available,
        },
        None => ExecutorAvailability {
            id: definition.id.clone(),
            available: false,
            executable: None,
            version: None,
            error: Some(format!(
                "{} not found on PATH or at the configured path",
                definition.command
            )),
            status: ExecutorAvailabilityStatus::NotFound,
        },
    }
}

pub fn common_executor_definitions() -> Vec<ExecutorDefinition> {
    [
        ("OpenCode", "opencode", "opencode"),
        ("Antigravity", "antigravity", "agy"),
        ("Codex", "codex", "codex"),
        ("Claude Code", "claude", "claude"),
        ("Gemini", "gemini", "gemini"),
        ("Grok", "grok", "grok"),
    ]
    .into_iter()
    .map(|(name, kind, command)| {
        let mut definition = ExecutorDefinition::new(name.into(), kind.into(), command.into());
        definition.id = format!("builtin-{}", definition.kind);
        definition
    })
    .collect()
}

pub fn executor_definitions_with_discovery(
    configured: &[ExecutorDefinition],
) -> Vec<(ExecutorDefinition, bool)> {
    let mut output = configured
        .iter()
        .cloned()
        .map(|definition| (definition, false))
        .collect::<Vec<_>>();
    for candidate in common_executor_definitions() {
        let duplicate = configured.iter().any(|definition| {
            definition.kind.eq_ignore_ascii_case(&candidate.kind)
                && definition.command.eq_ignore_ascii_case(&candidate.command)
                && definition.executable == candidate.executable
        });
        if !duplicate {
            output.push((candidate, true));
        }
    }
    output
}

pub fn opencode_version(command: &str) -> Option<String> {
    let exe = find_executable(command)?;
    let output = std::process::Command::new(&exe)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(ToOwned::to_owned);
    if let Some(line) = line {
        return Some(line);
    }
    Some(exe.display().to_string())
}
