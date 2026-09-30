//! Windows 软件启动：`.exe` 走 `Command`（`CreateProcessW`），`.lnk` 走 `ShellExecuteW`，
//! AUMID 走 `explorer.exe shell:AppsFolder\…`。
//!
//! 三处必须做对的地方（研究笔记 §1.9）：
//!
//! 1. **不预加引号**：`std::process::Command` 会按 `CommandLineToArgvW` 规则自己
//!    构造命令行；把路径预先包成 `"..."` 会让 `CreateProcessW` 找不到文件；
//! 2. **`ShellExecuteW` 的返回值不是 `Result`**：返回值 `<= 32` 是错误码；
//! 3. **失效条目必须报错**：文件不存在时立刻返回
//!    [`LaunchError::ProgramNotFound`]，绝不因为「Shell 没报错」就当成成功。

use std::path::Path;
use std::process::{Command, Stdio};

use crate::launch::{AppLauncher, LaunchError, LaunchReceipt};
use crate::launch_request::LaunchRequest;
use crate::windows::launch_plan::{plan, LaunchPlan, PlanError};
use crate::windows::uwp::apps_folder_argument;

/// `ERROR_ELEVATION_REQUIRED`（WinError 740）：目标要求提升权限。
const ERROR_ELEVATION_REQUIRED: i32 = 740;

const ELEVATION_HINT: &str =
    "该程序要求以管理员身份运行（ERROR_ELEVATION_REQUIRED）；请右键选择「以管理员身份运行」";

pub struct WindowsLauncher;

impl Default for WindowsLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsLauncher {
    pub fn new() -> Self {
        Self
    }
}

impl AppLauncher for WindowsLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError> {
        let planned = plan(
            &request.program,
            &request.args,
            request.working_dir.as_deref(),
            request.terminal,
        )
        .map_err(|PlanError::EmptyCommand| LaunchError::EmptyCommand)?;

        match planned {
            LaunchPlan::Direct {
                program,
                args,
                working_dir,
            } => spawn_direct(&program, &args, working_dir.as_deref()),
            LaunchPlan::ShellExecute {
                file,
                args,
                working_dir,
            } => shell_execute(
                &file,
                &args,
                working_dir
                    .as_deref()
                    .or(request.working_dir.as_deref()),
            ),
            LaunchPlan::AppsFolder { aumid } => spawn_apps_folder(&aumid),
        }
    }
}

/// `.exe`：直接用 `CreateProcessW` 语义启动。
fn spawn_direct(
    program: &str,
    args: &[String],
    working_dir: Option<&Path>,
) -> Result<LaunchReceipt, LaunchError> {
    // 绝对路径不存在时必须立刻报错，而不是把 CreateProcessW 的错误当成「已启动」。
    let path = Path::new(program);
    if path.is_absolute() && !path.exists() {
        return Err(LaunchError::ProgramNotFound {
            program: program.to_string(),
        });
    }

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = working_dir.filter(|dir| dir.is_dir()) {
        command.current_dir(dir);
    }

    let child = command.spawn().map_err(|error| map_spawn_error(program, &error))?;
    let pid = child.id();
    let argv = {
        let mut argv = vec![program.to_string()];
        argv.extend(args.iter().cloned());
        argv
    };

    // 单独线程回收子进程，避免长时间运行后积累僵尸句柄。
    std::thread::Builder::new()
        .name(format!("flashcast-reap-{pid}"))
        .spawn(move || {
            let mut child = child;
            let _ = child.wait();
        })
        .map_err(|error| LaunchError::Spawn {
            program: program.to_string(),
            reason: format!("无法创建进程回收线程：{error}"),
        })?;

    Ok(LaunchReceipt {
        pid: Some(pid),
        argv,
    })
}

/// `.lnk` / `.url`：交给 Shell 解析并启动。
fn shell_execute(
    file: &str,
    args: &[String],
    working_dir: Option<&Path>,
) -> Result<LaunchReceipt, LaunchError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if Path::new(file).is_absolute() && !Path::new(file).exists() {
        return Err(LaunchError::ProgramNotFound {
            program: file.to_string(),
        });
    }

    let wide = |value: &str| -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() };
    let file_wide = wide(file);
    // 目录条目（`.lnk`）不带参数：目标与参数都由 Shell 从快捷方式里取。
    // 这里仅在显式给出参数时把它们原样拼成 lpParameters。
    let parameters = (!args.is_empty()).then(|| args.join(" "));
    let parameters_wide = parameters.as_deref().map(wide);
    let dir_wide = working_dir
        .filter(|dir| dir.is_dir())
        .map(|dir| dir.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>());

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(file_wide.as_ptr()),
            parameters_wide
                .as_deref()
                .map(|value| PCWSTR(value.as_ptr()))
                .unwrap_or(PCWSTR::null()),
            dir_wide
                .as_deref()
                .map(|value| PCWSTR(value.as_ptr()))
                .unwrap_or(PCWSTR::null()),
            SW_SHOWNORMAL,
        )
    };

    // 返回值 <= 32 是错误码，不是句柄。
    let code = result.0 as isize;
    if code <= 32 {
        let code = code as i32;
        if code == ERROR_ELEVATION_REQUIRED {
            return Err(LaunchError::Spawn {
                program: file.to_string(),
                reason: ELEVATION_HINT.to_string(),
            });
        }
        if code == 2 {
            return Err(LaunchError::ProgramNotFound {
                program: file.to_string(),
            });
        }
        return Err(LaunchError::Spawn {
            program: file.to_string(),
            reason: format!("ShellExecuteW 返回错误码 {code}"),
        });
    }

    let mut argv = vec![file.to_string()];
    argv.extend(args.iter().cloned());
    Ok(LaunchReceipt {
        // ShellExecute 不返回 pid：新建进程的 id 只能由被启动的程序自己报告。
        pid: None,
        argv,
    })
}

/// AUMID：`explorer.exe shell:AppsFolder\<AUMID>`，对打包应用与带 AUMID 的 Win32 应用都有效。
fn spawn_apps_folder(aumid: &str) -> Result<LaunchReceipt, LaunchError> {
    let argument = apps_folder_argument(aumid);
    let mut command = Command::new("explorer.exe");
    command
        .arg(&argument)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command.spawn().map_err(|error| LaunchError::Spawn {
        program: "explorer.exe".to_string(),
        reason: format!("无法通过 shell:AppsFolder 启动打包应用 {aumid}：{error}"),
    })?;
    let pid = child.id();
    std::thread::Builder::new()
        .name(format!("flashcast-reap-{pid}"))
        .spawn(move || {
            let mut child = child;
            let _ = child.wait();
        })
        .ok();

    Ok(LaunchReceipt {
        // explorer.exe 只是代理：被启动应用的真实 pid 无法从它得到，
        // 与其报一个误导性的 explorer pid，不如如实报告 None。
        pid: None,
        argv: vec!["explorer.exe".to_string(), argument],
    })
}

fn map_spawn_error(program: &str, error: &std::io::Error) -> LaunchError {
    if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) {
        return LaunchError::Spawn {
            program: program.to_string(),
            reason: ELEVATION_HINT.to_string(),
        };
    }
    match error.kind() {
        std::io::ErrorKind::NotFound => LaunchError::ProgramNotFound {
            program: program.to_string(),
        },
        _ => LaunchError::Spawn {
            program: program.to_string(),
            reason: error.to_string(),
        },
    }
}
