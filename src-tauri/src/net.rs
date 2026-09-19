//! 网络代理解析。
//!
//! reqwest 默认只认 `HTTP_PROXY` / `HTTPS_PROXY` 环境变量，
//! 而 v2rayN / Clash 这类工具通常只改 **系统代理**（WinINET 设置），
//! 结果就是浏览器能上网、应用里却连不上。这里把系统代理也读出来。
//!
//! 解析顺序（`proxy_mode = "auto"`）：
//!   1. 环境变量（HTTPS_PROXY / HTTP_PROXY / ALL_PROXY，大小写都试）
//!   2. Windows：注册表 `Internet Settings` 里的 ProxyEnable + ProxyServer
//!   3. macOS：`scutil --proxy`
//! 拿到之后统一交给 reqwest 的 `Proxy::all()`，并且永远跳过本地地址。

use crate::error::Result;
use crate::models::AppSettings;

/// 本地地址不走代理 —— 本机的 vLLM / Ollama / ComfyUI 不能被代理拦掉。
const NO_PROXY: &str = "localhost,127.0.0.1,::1,0.0.0.0,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";

fn env_proxy() -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let enabled = std::process::Command::new("reg")
        .args(["query", key, "/v", "ProxyEnable"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .filter(|s| s.contains("0x1"));
    if enabled.is_none() {
        return None;
    }
    let out = std::process::Command::new("reg")
        .args(["query", key, "/v", "ProxyServer"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let raw = text
        .lines()
        .find(|l| l.contains("ProxyServer"))?
        .split("REG_SZ")
        .nth(1)?
        .trim()
        .to_string();
    if raw.is_empty() {
        return None;
    }
    // 可能是 "127.0.0.1:10808"，也可能是 "http=1.2.3.4:8080;https=1.2.3.4:8080"
    let pick = raw
        .split(';')
        .find_map(|part| part.trim().strip_prefix("https="))
        .or_else(|| {
            raw.split(';')
                .find_map(|part| part.trim().strip_prefix("http="))
        })
        .map(|s| s.trim().to_string())
        .unwrap_or(raw);
    Some(ensure_scheme(&pick))
}

#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<String> {
    let out = std::process::Command::new("scutil").arg("--proxy").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let enabled = text.contains("HTTPSEnable : 1") || text.contains("HTTPEnable : 1");
    if !enabled {
        return None;
    }
    let host = text
        .lines()
        .find(|l| l.contains("HTTPSProxy"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .or_else(|| {
            text.lines()
                .find(|l| l.contains("HTTPProxy"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })?;
    let port = text
        .lines()
        .find(|l| l.contains("HTTPSPort"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "7890".into());
    Some(format!("http://{host}:{port}"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn macos_system_proxy() -> Option<String> {
    None
}

fn ensure_scheme(s: &str) -> String {
    let t = s.trim();
    if t.contains("://") {
        t.to_string()
    } else {
        format!("http://{t}")
    }
}

/// 按设置解析出最终要用的代理地址。None 表示直连。
pub fn resolve_proxy(settings: &AppSettings) -> Option<String> {
    match settings.proxy_mode.as_str() {
        "off" => None,
        "manual" => {
            let u = settings.proxy_url.trim();
            if u.is_empty() {
                None
            } else {
                Some(ensure_scheme(u))
            }
        }
        // auto（默认）
        _ => env_proxy()
            .map(|s| ensure_scheme(&s))
            .or_else(system_proxy),
    }
}

pub fn system_proxy() -> Option<String> {
    #[cfg(windows)]
    {
        windows_system_proxy()
    }
    #[cfg(not(windows))]
    {
        macos_system_proxy()
    }
}

/// 构造 reqwest 客户端，带上解析出来的代理。
pub fn build_client(timeout_sec: u64, proxy: Option<&str>) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(if timeout_sec == 0 {
            900
        } else {
            timeout_sec
        }))
        .user_agent(concat!("DuanjuWorkbench/", env!("CARGO_PKG_VERSION")));

    match proxy {
        Some(url) => {
            let p = reqwest::Proxy::all(url)
                .map_err(|e| crate::error::AppError::Config(format!("代理地址不合法：{url}（{e}）")))?;
            b = b.proxy(p.no_proxy(reqwest::NoProxy::from_string(NO_PROXY)));
        }
        None => {
            // 显式关掉，避免环境变量又被 reqwest 自己捡起来造成「设置说直连却走了代理」
            b = b.no_proxy();
        }
    }
    b.build().map_err(Into::into)
}

/// 诊断信息，给设置面板显示用。
pub fn diagnostics(settings: &AppSettings) -> serde_json::Value {
    serde_json::json!({
        "mode": settings.proxy_mode,
        "manualUrl": settings.proxy_url,
        "envProxy": env_proxy(),
        "systemProxy": system_proxy(),
        "effective": resolve_proxy(settings),
        "noProxy": NO_PROXY,
    })
}
