//! 轻量文件存储：所有项目数据都是普通 JSON / Markdown / 媒体文件。
//! 没有原生数据库依赖 —— 项目目录整个拷走就能在另一台机器（含 Apple Silicon）打开。

use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;

pub fn ensure_dir(p: &Path) -> Result<()> {
    if !p.exists() {
        fs::create_dir_all(p)?;
    }
    Ok(())
}

pub fn read_json_opt<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    Ok(read_json_opt::<T>(path)?.unwrap_or_default())
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(path, &bytes)
}

pub fn read_text_opt(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(fs::read_to_string(path)?))
}

pub fn read_text_or_empty(path: &Path) -> Result<String> {
    Ok(read_text_opt(path)?.unwrap_or_default())
}

pub fn write_text(path: &Path, s: &str) -> Result<()> {
    write_bytes_atomic(path, s.as_bytes())
}

/// 先写临时文件再替换，避免崩溃时留下半个文件。
pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = tmp_path(path);
    fs::write(&tmp, bytes)?;
    replace(&tmp, path)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Windows 下 rename 不能覆盖已有文件，失败时退化为「删除 + 重命名」。
fn replace(tmp: &Path, dest: &Path) -> Result<()> {
    match fs::rename(tmp, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            if dest.exists() {
                let _ = fs::remove_file(dest);
            }
            fs::rename(tmp, dest)?;
            Ok(())
        }
    }
}

pub fn remove_file_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// 内容指纹，用于判断 LLM 稳定前缀是否变化。
pub fn fingerprint(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex()[..16].to_string()
}

/// 中英混排的字数统计：CJK 按字算，拉丁按词算。
pub fn count_words(s: &str) -> u32 {
    let mut cjk = 0u32;
    let mut latin_words = 0u32;
    let mut in_word = false;
    for ch in s.chars() {
        let is_cjk = matches!(ch as u32,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x3040..=0x30FF);
        if is_cjk {
            cjk += 1;
            in_word = false;
        } else if ch.is_alphanumeric() {
            if !in_word {
                latin_words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    cjk + latin_words
}
