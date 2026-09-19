//! 应用级配置。放在系统应用数据目录，与项目目录分开：
//! - settings.json 用户偏好 + 供应商列表
//! - secrets.json  API Key（明文存放于用户私有目录，便于导出迁移；后续可换成系统钥匙串）

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::AppSettings;
use crate::store;

/// 应用配置目录，与 Tauri 的 `app_config_dir()` 保持一致。
/// 命令行工具（`--net-check` / `--asr-download`）用它，才能和界面共用同一份设置与模型。
pub fn default_config_dir() -> PathBuf {
    const ID: &str = "com.duanju.workbench";
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(ID);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(ID);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        return base.join(ID);
    }
    #[allow(unreachable_code)]
    PathBuf::from(ID)
}

#[derive(Clone)]
pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    fn secrets_path(&self) -> PathBuf {
        self.dir.join("secrets.json")
    }

    pub fn load_settings(&self) -> AppSettings {
        store::read_json_or_default::<AppSettings>(&self.settings_path()).unwrap_or_default()
    }

    pub fn save_settings(&self, s: &AppSettings) -> Result<()> {
        store::write_json(&self.settings_path(), s)
    }

    fn load_secrets(&self) -> BTreeMap<String, String> {
        store::read_json_or_default::<BTreeMap<String, String>>(&self.secrets_path())
            .unwrap_or_default()
    }

    pub fn get_secret(&self, key: &str) -> Option<String> {
        if key.is_empty() {
            return None;
        }
        self.load_secrets().get(key).cloned().filter(|v| !v.is_empty())
    }

    pub fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        let mut m = self.load_secrets();
        if value.is_empty() {
            m.remove(key);
        } else {
            m.insert(key.to_string(), value.to_string());
        }
        store::write_json(&self.secrets_path(), &m)
    }

    pub fn delete_secret(&self, key: &str) -> Result<()> {
        self.set_secret(key, "")
    }

    /// 只回传「是否已配置」，绝不把密钥内容送到前端。
    pub fn secret_status(&self) -> BTreeMap<String, bool> {
        self.load_secrets()
            .into_iter()
            .map(|(k, v)| (k, !v.is_empty()))
            .collect()
    }
}
