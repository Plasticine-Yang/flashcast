//! Linux 软件启动：按 argv 直接执行，绝不经过 `sh -c`。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::launch::{AppLauncher, LaunchError, LaunchReceipt};
use crate::launch_request::LaunchRequest;

/// 需要终端时按顺序尝试的终端模拟器。全部使用 `-e <argv...>` 约定。
const TERMINAL_EMULATORS: [&str; 8] = [
    "x-terminal-emulator",
    "kgx",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
    "alacritty",
    "kitty",
    "foot",
];

pub struct LinuxLauncher {
    path_env: String,
}

impl Default for LinuxLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxLauncher {
    pub fn new() -> Self {
        Self {
            path_env: std::env::var("PATH").unwrap_or_default(),
        }
    }

    pub fn with_path(path_env: String) -> Self {
        Self { path_env }
    }

    /// 在 PATH 中查找可执行文件。
    pub fn which(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            let path = PathBuf::from(program);
            return is_executable(&path).then_some(path);
        }
        self.path_env
            .split(':')
            .filter(|segment| !segment.is_empty())
            .map(|segment| Path::new(segment).join(program))
            .find(|candidate| is_executable(candidate))
    }

    /// 找到可用的终端模拟器。
    pub fn find_terminal(&self) -> Option<PathBuf> {
        TERMINAL_EMULATORS
            .iter()
            .find_map(|terminal| self.which(terminal))
    }
}

impl AppLauncher for LinuxLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError> {
        if request.program.trim().is_empty() {
            return Err(LaunchError::EmptyCommand);
        }

        let mut program_path = self.which(&request.program).ok_or_else(|| {
            LaunchError::ProgramNotFound {
                program: request.program.clone(),
            }
        })?;
        let mut args = request.args.clone();

        if request.terminal {
            let terminal = self
                .find_terminal()
                .ok_or_else(|| LaunchError::Spawn {
                    program: request.program.clone(),
                    reason: "该软件需要终端，但未找到可用的终端模拟器".to_string(),
                })?;
            // `Terminal=true` 的条目统一用 `-e <argv...>` 交给终端模拟器执行，
            // 仍然按 argv 传递，不拼接 shell 命令。
            let mut wrapped = vec![
                "-e".to_string(),
                program_path.to_string_lossy().into_owned(),
            ];
            wrapped.append(&mut args);
            args = wrapped;
            program_path = terminal;
        }

        let argv = {
            let mut argv = vec![program_path.to_string_lossy().into_owned()];
            argv.extend(args.iter().cloned());
            argv
        };

        let mut command = Command::new(&program_path);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = request.working_dir.as_ref().filter(|d| d.is_dir()) {
            command.current_dir(dir);
        }
        // 让子进程进入独立进程组：退出 Flashcast 或终端信号不会连带影响它。
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().map_err(|error| LaunchError::Spawn {
            program: request.program.clone(),
            reason: error.to_string(),
        })?;
        let pid = child.id();

        // 单独线程回收子进程，避免长时间运行后积累僵尸进程。
        std::thread::Builder::new()
            .name(format!("flashcast-reap-{pid}"))
            .spawn(move || {
                let mut child = child;
                let _ = child.wait();
            })
            .map_err(|error| LaunchError::Spawn {
                program: request.program.clone(),
                reason: format!("无法创建进程回收线程：{error}"),
            })?;

        Ok(LaunchReceipt {
            pid: Some(pid),
            argv,
        })
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
