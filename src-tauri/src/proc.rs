//! 子进程工具。
//!
//! Windows 上 `std::process::Command` 默认会**弹一个控制台黑框**，
//! 对 GUI 程序来说这非常显眼 —— 打开一次设置面板就可能闪好几个。
//! 所有外部命令一律通过 `hidden()` 创建，统一带上 CREATE_NO_WINDOW。

use std::ffi::OsStr;

/// Windows: CREATE_NO_WINDOW；其他平台无此概念，原样返回。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hidden(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 跑一条命令并拿到 stdout 文本；失败或非 UTF-8 时返回 None。
/// 用于探测类调用（nvidia-smi、reg query 之类），调用方不关心退出码。
pub fn output_text(program: impl AsRef<OsStr>, args: &[&str]) -> Option<String> {
    let out = hidden(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}
