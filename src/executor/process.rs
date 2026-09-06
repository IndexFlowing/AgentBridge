use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::executor::ExecutorError;

/// 在 PATH 上定位可执行文件（具备 Windows PATHEXT 识别与绝对路径支持）
pub fn find_executable(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() || command.contains('\0') {
        return None;
    }
    let path = Path::new(command);
    if path.is_absolute() {
        return existing_file(path);
    }
    if command.contains("..") {
        return None;
    }
    if path.components().count() > 1 {
        return existing_file(path);
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for candidate in candidates_in_dir(&dir, command) {
            if let Some(found) = existing_file(&candidate) {
                return Some(found);
            }
        }
    }
    None
}

fn existing_file(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // Windows 下：如果已有扩展名且存在，返回；如果无扩展名，绝不直接返回无后缀文件，优先查 PATHEXT！
        if has_extension(path) && path.is_file() {
            return Some(path.to_path_buf());
        }
        for ext in pathext() {
            let mut p = path.as_os_str().to_os_string();
            p.push(&ext);
            let with_ext = PathBuf::from(p);
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
        // 如果实在没有可执行后缀，但文件存在（兜底）
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    #[cfg(not(windows))]
    {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    None
}

fn candidates_in_dir(dir: &Path, command: &str) -> Vec<PathBuf> {
    let base = dir.join(command);
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        if has_extension(&base) {
            out.push(base);
        } else {
            // 👈 核心修复：Windows 下必须优先探测 .exe / .cmd / .bat，防止误命中 npm 生成的无后缀脚本！
            for ext in pathext() {
                let mut p = base.as_os_str().to_os_string();
                p.push(&ext);
                out.push(PathBuf::from(p));
            }
            out.push(base);
        }
    }

    #[cfg(not(windows))]
    {
        out.push(base);
    }

    out
}

#[cfg(windows)]
fn has_extension(path: &Path) -> bool {
    path.extension().is_some()
}

#[cfg(windows)]
fn pathext() -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_default();
    let mut extensions = raw
        .split(';')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    for ext in [".com", ".exe", ".bat", ".cmd"] {
        if !extensions.iter().any(|item| item == ext) {
            extensions.push(ext.into());
        }
    }
    extensions
}

/// 跨平台杀死整棵进程树
pub fn kill_process_tree(pid: u32) -> Result<(), ExecutorError> {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
    #[cfg(unix)]
    {
        // 优先向独立进程组发送 SIGTERM
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // 保底针对主进程发信号
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
}

/// 检测进程是否依然存活
pub fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.split(',')
                    .any(|col| col.trim_matches('"') == pid.to_string())
                    || s.contains(&pid.to_string())
            }
            Err(_) => false,
        }
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}