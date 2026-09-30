//! macOS 启动：**始终按路径**打开应用包。
//!
//! 为什么不用 `open -b <bundleid>`：重复 bundle id 是真实存在的（同一个应用的
//! 稳定版与测试版、外置卷上的副本、`~/Applications` 覆盖 `/Applications`），
//! LaunchServices 会自己挑一份注册过的，挑中的未必是用户点的那一份。按路径打开
//! 没有这个歧义。bundle id 只用于身份、排序与去重。
//!
//! 已知限制（不隐瞒，写进诊断报告）：
//! - `open` 返回成功只表示 LaunchServices 接受了请求，**不表示应用已经启动**；
//!   因此启动回执的 `pid` 为 `None`；
//! - 应用已经在运行时，`--args` 会被 LaunchServices 丢掉（改为投递「打开文稿」
//!   事件），这是 `open(1)` 的既有行为，无法在同步调用里察觉。

use std::path::{Path, PathBuf};

use crate::launch::LaunchError;

#[cfg(target_os = "macos")]
use crate::launch::{AppLauncher, LaunchReceipt};
#[cfg(target_os = "macos")]
use crate::launch_request::LaunchRequest;
#[cfg(target_os = "macos")]
use std::process::Command;

/// `open(1)` 的固定位置。用绝对路径而不是 PATH 查找，避免被环境污染。
pub const OPEN_BIN: &str = "/usr/bin/open";

/// 需要终端时使用的终端应用名。
pub const TERMINAL_APP: &str = "Terminal";

/// 一次启动要执行的计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlan {
    /// 用 `/usr/bin/open` 打开应用包（或把非应用包交给 Terminal.app）。
    Open { argv: Vec<String> },
    /// 直接 exec 非应用包的可执行文件（等同 Linux 的按 argv 直接执行）。
    Exec { program: PathBuf, args: Vec<String> },
}

/// 路径是否指向一个 `.app` 应用包目录。
pub fn is_bundle_path(path: &Path) -> bool {
    let is_app = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("app"))
        .unwrap_or(false);
    is_app && path.is_dir()
}

/// 生成启动计划。纯函数，便于在没有 macOS 的机器上验证参数拼装。
///
/// `program_is_bundle` 由调用方根据文件系统判断（[`is_bundle_path`]）。
pub fn launch_plan(
    program: &str,
    args: &[String],
    terminal: bool,
    program_is_bundle: bool,
) -> Result<LaunchPlan, LaunchError> {
    if program.trim().is_empty() {
        return Err(LaunchError::EmptyCommand);
    }

    if program_is_bundle {
        // GUI 应用包不经过终端；`Terminal=true` 在 macOS 上没有对应语义。
        let mut argv = vec![OPEN_BIN.to_string(), "-a".to_string(), program.to_string()];
        if !args.is_empty() {
            argv.push("--args".to_string());
            argv.extend(args.iter().cloned());
        }
        return Ok(LaunchPlan::Open { argv });
    }

    let path = PathBuf::from(program);
    if terminal {
        if !args.is_empty() {
            // 不静默丢弃参数：通过 Terminal.app 打开无法把参数传给目标程序，
            // 与其给用户一个「启动成功」的假象，不如如实报错。
            return Err(LaunchError::Spawn {
                program: program.to_string(),
                reason: "macOS 下通过 Terminal 打开非应用包时无法传递额外参数".to_string(),
            });
        }
        return Ok(LaunchPlan::Open {
            argv: vec![
                OPEN_BIN.to_string(),
                "-a".to_string(),
                TERMINAL_APP.to_string(),
                program.to_string(),
            ],
        });
    }

    Ok(LaunchPlan::Exec {
        program: path,
        args: args.to_vec(),
    })
}

#[cfg(target_os = "macos")]
pub struct MacosLauncher {
    path_env: String,
}

#[cfg(target_os = "macos")]
impl Default for MacosLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosLauncher {
    pub fn new() -> Self {
        Self {
            path_env: std::env::var("PATH").unwrap_or_default(),
        }
    }

    pub fn with_path(path_env: String) -> Self {
        Self { path_env }
    }

    /// 在 PATH 中查找可执行文件（只用于非应用包的程序名）。
    pub fn which(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            return Some(PathBuf::from(program));
        }
        self.path_env
            .split(':')
            .filter(|segment| !segment.is_empty())
            .map(|segment| Path::new(segment).join(program))
            .find(|candidate| candidate.is_file())
    }
}

#[cfg(target_os = "macos")]
impl AppLauncher for MacosLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError> {
        if request.program.trim().is_empty() {
            return Err(LaunchError::EmptyCommand);
        }

        let direct = Path::new(&request.program);
        let program_is_bundle = is_bundle_path(direct);
        // 非应用包且带路径分隔符时必须真实存在，否则给出明确的「找不到」。
        if !program_is_bundle && request.program.contains('/') && !direct.exists() {
            return Err(LaunchError::ProgramNotFound {
                program: request.program.clone(),
            });
        }
        if !program_is_bundle && !request.program.contains('/') && !request.terminal {
            if self.which(&request.program).is_none() {
                return Err(LaunchError::ProgramNotFound {
                    program: request.program.clone(),
                });
            }
        }

        let plan = launch_plan(
            &request.program,
            &request.args,
            request.terminal,
            program_is_bundle,
        )?;

        match plan {
            LaunchPlan::Open { argv } => {
                let (binary, rest) = argv.split_first().expect("计划至少包含可执行文件");
                let output = Command::new(binary)
                    .args(rest)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .output()
                    .map_err(|error| LaunchError::Spawn {
                        program: request.program.clone(),
                        reason: format!("无法执行 {OPEN_BIN}：{error}"),
                    })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let code = output
                        .status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "信号终止".to_string());
                    return Err(LaunchError::Spawn {
                        program: request.program.clone(),
                        reason: if stderr.is_empty() {
                            format!("open 退出码 {code}")
                        } else {
                            format!("open 退出码 {code}：{stderr}")
                        },
                    });
                }
                Ok(LaunchReceipt { pid: None, argv })
            }
            LaunchPlan::Exec { program, args } => {
                let mut argv = vec![program.to_string_lossy().into_owned()];
                argv.extend(args.iter().cloned());
                let mut command = Command::new(&program);
                command
                    .args(&args)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                if let Some(dir) = request.working_dir.as_ref().filter(|dir| dir.is_dir()) {
                    command.current_dir(dir);
                }
                // 子进程进入独立进程组：退出 Flashcast 不会连带影响它。
                {
                    use std::os::unix::process::CommandExt;
                    command.process_group(0);
                }
                let child = command.spawn().map_err(|error| LaunchError::Spawn {
                    program: request.program.clone(),
                    reason: error.to_string(),
                })?;
                let pid = child.id();
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
    }
}
