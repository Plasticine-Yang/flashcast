//! 启动计划（纯逻辑）：把 argv[0] 映射到三种真实启动方式之一。
//!
//! 研究笔记 §1.9 的结论：
//!
//! * `.exe` 用 `Command::new(path)`（`CreateProcessW` 语义），**不得预先加引号** ——
//!   `std::process::Command` 会按 `CommandLineToArgvW` 规则自己加引号，
//!   预先加引号反而会让 `CreateProcessW` 找不到文件；
//! * `.lnk` / `.url` 必须走 `ShellExecuteW`：`.lnk` 不是可执行文件，
//!   `CreateProcessW` 会直接拒绝；由 Shell 解析目标与参数，也顺带解决了
//!   `.lnk` 里参数串的分词问题；
//! * AUMID 用 `explorer.exe shell:AppsFolder\<AUMID>`，不需要 COM 与 CLSID。

use std::path::{Path, PathBuf};

use crate::windows::uwp::aumid_from_program;

/// 一次启动应当怎样执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlan {
    /// 直接 `CreateProcessW`：程序路径已归一化为 `OsString`，不预先加引号。
    Direct {
        program: String,
        args: Vec<String>,
        working_dir: Option<PathBuf>,
    },
    /// 交给 Shell：适用于 `.lnk`、`.url` 与 Shell 命名空间目标。
    ShellExecute {
        file: String,
        args: Vec<String>,
        working_dir: Option<PathBuf>,
    },
    /// 打包应用：`explorer.exe shell:AppsFolder\<AUMID>`。
    AppsFolder { aumid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("该软件没有可执行的启动命令")]
    EmptyCommand,
}

/// 需要交给 Shell 的扩展名（大小写不敏感）。
const SHELL_TARGET_EXTENSIONS: [&str; 2] = ["lnk", "url"];

/// 该路径是否必须交给 `ShellExecuteW`。
pub fn needs_shell_execute(program: &str) -> bool {
    let lower = program.trim().to_ascii_lowercase();
    SHELL_TARGET_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(&format!(".{extension}")))
}

/// 由 `program` 与 `args` 生成启动计划。
///
/// `terminal` 在 Windows 上不需要特殊处理：控制台子系统的程序由系统自行分配控制台，
/// 因此这里忽略它，但保留参数以对齐跨平台签名。
pub fn plan(
    program: &str,
    args: &[String],
    working_dir: Option<&Path>,
    _terminal: bool,
) -> Result<LaunchPlan, PlanError> {
    let trimmed = program.trim();
    if trimmed.is_empty() {
        return Err(PlanError::EmptyCommand);
    }
    if let Some(aumid) = aumid_from_program(trimmed) {
        return Ok(LaunchPlan::AppsFolder {
            aumid: aumid.to_string(),
        });
    }
    let working_dir = working_dir.map(Path::to_path_buf);
    if needs_shell_execute(trimmed) {
        return Ok(LaunchPlan::ShellExecute {
            file: trimmed.to_string(),
            args: args.to_vec(),
            working_dir,
        });
    }
    Ok(LaunchPlan::Direct {
        // 原样保留：绝不预先加引号（见模块文档）。
        program: trimmed.to_string(),
        args: args.to_vec(),
        working_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn exe_paths_are_never_pre_quoted() {
        let path = r"C:\Program Files\示例 应用\app.exe";
        let planned = plan(path, &args(&["--profile", "a b"]), None, false).expect("计划可生成");
        assert_eq!(
            planned,
            LaunchPlan::Direct {
                program: path.to_string(),
                args: args(&["--profile", "a b"]),
                working_dir: None,
            }
        );
        assert!(
            !planned_program(&planned).starts_with('"'),
            "路径本身不得带引号：{}",
            planned_program(&planned)
        );
    }

    fn planned_program(plan: &LaunchPlan) -> &str {
        match plan {
            LaunchPlan::Direct { program, .. } => program,
            LaunchPlan::ShellExecute { file, .. } => file,
            LaunchPlan::AppsFolder { aumid } => aumid,
        }
    }

    #[test]
    fn shortcuts_and_urls_go_through_the_shell() {
        let lnk = r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Word.lnk";
        let planned = plan(lnk, &[], Some(Path::new(r"C:\Docs")), false).expect("计划可生成");
        assert_eq!(
            planned,
            LaunchPlan::ShellExecute {
                file: lnk.to_string(),
                args: Vec::new(),
                working_dir: Some(PathBuf::from(r"C:\Docs")),
            }
        );
        assert!(needs_shell_execute("Bookmark.URL"));
        assert!(!needs_shell_execute(r"C:\app.exe"));
    }

    #[test]
    fn aumid_programs_launch_through_apps_folder() {
        let planned = plan(
            "aumid:Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
            &[],
            None,
            false,
        )
        .expect("计划可生成");
        assert_eq!(
            planned,
            LaunchPlan::AppsFolder {
                aumid: "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_string()
            }
        );
    }

    #[test]
    fn empty_program_is_rejected() {
        assert_eq!(plan("   ", &[], None, false), Err(PlanError::EmptyCommand));
        assert_eq!(plan("", &[], None, false), Err(PlanError::EmptyCommand));
    }
}
