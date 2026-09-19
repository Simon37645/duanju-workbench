//! 通用文件上传。
//!
//! 有些生图 / 生视频接口不接受 base64，只收**公网可访问的 URL**（第三方平台 的 ComfyUI 工作流就是
//! 这样：`first_frame`、`ref_image_0` 都写明"图片 URL"）。本地生成的资产图必须先传上去。
//!
//! 这里实现两种模式，用 provider 的 `options.upload` 配置选择：
//!
//! **simple** —— 一次 multipart POST：
//! ```jsonc
//! "upload": {
//!   "mode": "simple",
//!   "path": "/api/v1/upload",
//!   "field": "file",
//!   "headers": { },
//!   "urlPath": "/data/url"        // 返回体里取 URL 的 JSON Pointer
//! }
//! ```
//!
//! **chunked** —— 第三方平台 用的三段式（先换 token，再声明分片，最后逐片 PUT）：
//! ```jsonc
//! "upload": {
//!   "mode": "chunked",
//!   "beforePath": "/api/v1/file/before_upload",  // POST {md5, file_size}
//!                                                // -> data.data.{token, host, quickly_upload}
//!   "initPath": "/api/v1/file",                  // POST {md5, total_size, chunks} + FileToken 头
//!   "chunkPath": "/api/v1/file",                 // PUT multipart: FileToken/md5/chunk/chunks/chunk_size/file
//!   "field": "file",
//!   "chunkSize": 10485760,
//!   "refTemplate": "{{md5}}"                     // 传完之后 {{imageN}} 用什么值
//! }
//! ```
//!
//! `refTemplate` 里的 `{{md5}}` / `{{url}}` / `{{path}}` 会被替换成上传结果。如果平台是用
//! md5 去引用文件，就写 `{{md5}}`；如果返回了 URL，就写 `{{url}}`。

use std::path::Path;

use serde_json::Value;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Default)]
pub struct UploadConfig {
    pub mode: String,
    pub field: String,
    pub before_path: String,
    pub init_path: String,
    pub chunk_path: String,
    pub simple_path: String,
    pub url_path: Vec<String>,
    pub ref_template: String,
    pub chunk_size: usize,
    pub headers: Vec<(String, String)>,
}

impl UploadConfig {
    pub fn from_options(options: &Value) -> Option<Self> {
        let u = options.get("upload")?;
        if u.is_null() || u.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return None;
        }
        let s = |k: &str, d: &str| {
            u.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or(d)
                .to_string()
        };
        let mut headers = vec![];
        if let Some(obj) = u.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(v) = v.as_str() {
                    headers.push((k.clone(), v.to_string()));
                }
            }
        }
        let url_path = match u.get("urlPath") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect(),
            _ => vec!["/data/url".into(), "/data/file_url".into(), "/url".into()],
        };
        Some(Self {
            mode: s("mode", "simple"),
            field: s("field", "file"),
            before_path: s("beforePath", "/api/v1/file/before_upload"),
            init_path: s("initPath", "/api/v1/file"),
            chunk_path: s("chunkPath", "/api/v1/file"),
            simple_path: s("path", "/api/v1/upload"),
            url_path,
            ref_template: s("refTemplate", "{{url}}"),
            chunk_size: u
                .get("chunkSize")
                .and_then(|v| v.as_u64())
                .unwrap_or(10 * 1024 * 1024) as usize,
            headers,
        })
    }
}

/// 有些平台认证失败时照样返回 HTTP 200，只在 body 里放 `code`/`msg`
/// （第三方平台 就是这样：`{"code":"AuthorizeFailed","msg":"认证失败; 登录超时"}`）。
/// 不先查这个的话，报出来的错会是"没找到 token"，完全看不出真正原因。
fn check_envelope(v: &Value, what: &str) -> Result<()> {
    let Some(code) = v.get("code").and_then(|c| c.as_str()) else {
        return Ok(());
    };
    let ok = code.is_empty()
        || code.eq_ignore_ascii_case("success")
        || code.eq_ignore_ascii_case("ok")
        || code == "0"
        || code == "200";
    if ok {
        return Ok(());
    }
    let msg = v
        .get("msg")
        .or_else(|| v.get("message"))
        .or_else(|| v.pointer("/error/message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    Err(AppError::Provider(format!(
        "{what}被拒绝：{code} {msg}
（检查 provider 的 API Key / Token 是否正确；
         若提示认证失败但密钥没问题，把这个平台的 authStyle 在 bearer 与 raw 之间换一下）"
    )))
}

fn join(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{}{}", base.trim_end_matches('/'), path)
    }
}

fn pick(v: &Value, paths: &[String]) -> Option<Value> {
    for p in paths {
        if let Some(found) = v.pointer(p) {
            if !found.is_null() {
                return Some(found.clone());
            }
        }
    }
    None
}

fn as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn apply_template(tpl: &str, vars: &[(&str, &str)]) -> String {
    let mut out = tpl.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// 上传一个文件，返回可以放进请求体的引用值。
pub async fn upload_file(
    cfg: &UploadConfig,
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    auth_style: &str,
    path: &Path,
) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let md5 = format!("{:x}", md5::compute(&bytes));
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{md5}.png"));

    if cfg.mode == "chunked" {
        upload_chunked(cfg, http, base_url, api_key, auth_style, &bytes, &md5, &file_name).await
    } else {
        upload_simple(cfg, http, base_url, api_key, auth_style, &bytes, &md5, &file_name).await
    }
}

fn with_auth(
    rb: reqwest::RequestBuilder,
    api_key: Option<&str>,
    auth_style: &str,
) -> reqwest::RequestBuilder {
    match (api_key, auth_style) {
        (Some(k), "raw") => rb.header("Authorization", k),
        (Some(_), "none") => rb,
        (Some(k), _) => rb.bearer_auth(k),
        (None, _) => rb,
    }
}

async fn upload_simple(
    cfg: &UploadConfig,
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    auth_style: &str,
    bytes: &[u8],
    md5: &str,
    file_name: &str,
) -> Result<String> {
    let url = join(base_url, &cfg.simple_path);
    let make_form = || {
        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(file_name.to_string())
            .mime_str("application/octet-stream")
            .expect("mime 常量不会失败");
        reqwest::multipart::Form::new().part(cfg.field.clone(), part)
    };

    let resp = crate::llm::send_with_retry(
        http,
        || {
            let mut rb = http.post(&url).multipart(make_form());
            for (hk, hv) in &cfg.headers {
                rb = rb.header(hk.as_str(), hv.as_str());
            }
            with_auth(rb, api_key, auth_style)
        },
        3,
    )
    .await?;
    let text = resp.text().await?;
    let v: Value = serde_json::from_str(&text).map_err(|_| {
        AppError::Provider(format!(
            "上传返回的不是 JSON：{}",
            crate::llm::truncate(&text, 400)
        ))
    })?;
    check_envelope(&v, "上传")?;
    let found = pick(&v, &cfg.url_path).ok_or_else(|| {
        AppError::Provider(format!(
            "上传成功但没在 {} 里找到文件地址：{}",
            cfg.url_path.join(", "),
            crate::llm::truncate(&text, 400)
        ))
    })?;
    Ok(apply_template(
        &cfg.ref_template,
        &[("url", &as_str(&found)), ("md5", md5), ("path", &as_str(&found))],
    ))
}

#[allow(clippy::too_many_arguments)]
async fn upload_chunked(
    cfg: &UploadConfig,
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    auth_style: &str,
    bytes: &[u8],
    md5: &str,
    file_name: &str,
) -> Result<String> {
    // 1) 预上传：拿 token 与上传主机，顺带按 md5 去重
    let before_url = join(base_url, &cfg.before_path);
    let body = serde_json::json!({ "md5": md5, "file_size": bytes.len() });
    let resp = crate::llm::send_with_retry(
        http,
        || {
            let mut rb = http.post(&before_url).json(&body);
            for (hk, hv) in &cfg.headers {
                rb = rb.header(hk.as_str(), hv.as_str());
            }
            with_auth(rb, api_key, auth_style)
        },
        3,
    )
    .await?;
    let v: Value = serde_json::from_str(&resp.text().await?)?;
    check_envelope(&v, "预上传")?;
    let token = v
        .pointer("/data/data/token")
        .or_else(|| v.pointer("/data/token"))
        .map(as_str)
        .ok_or_else(|| {
            AppError::Provider(format!(
                "预上传没返回 token：{}",
                crate::llm::truncate(&v.to_string(), 400)
            ))
        })?;
    let host = v
        .pointer("/data/data/host")
        .or_else(|| v.pointer("/data/host"))
        .map(as_str)
        .unwrap_or_else(|| base_url.to_string());
    let quick = v
        .pointer("/data/data/quickly_upload")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);

    // 秒传：服务端已有同 md5 的文件，直接返回引用
    if quick {
        return Ok(apply_template(
            &cfg.ref_template,
            &[("url", md5), ("md5", md5), ("path", md5)],
        ));
    }

    // 2) 声明分片，拿断点
    let chunks: Vec<&[u8]> = bytes.chunks(cfg.chunk_size).collect();
    let total = chunks.len().max(1);
    let init_url = join(&host, &cfg.init_path);
    let init_body =
        serde_json::json!({ "md5": md5, "total_size": bytes.len(), "chunks": total });
    let resp = crate::llm::send_with_retry(
        http,
        || {
            let mut rb = http
                .post(&init_url)
                .header("FileToken", token.as_str())
                .json(&init_body);
            for (hk, hv) in &cfg.headers {
                rb = rb.header(hk.as_str(), hv.as_str());
            }
            with_auth(rb, api_key, auth_style)
        },
        3,
    )
    .await?;
    let init_v: Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or(Value::Null);
    let start_seq = init_v
        .pointer("/data/data/next_seq")
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as usize;

    // 3) 逐片 PUT（断点之后的部分）
    let chunk_url = join(&host, &cfg.chunk_path);
    for (seq, chunk) in chunks.iter().enumerate() {
        if seq < start_seq {
            continue;
        }
        let make_form = || {
            let part = reqwest::multipart::Part::bytes(chunk.to_vec())
                .file_name(file_name.to_string())
                .mime_str("application/octet-stream")
                .expect("mime 常量不会失败");
            reqwest::multipart::Form::new()
                .text("md5", md5.to_string())
                .text("chunk", seq.to_string())
                .text("chunks", total.to_string())
                .text("chunk_size", chunk.len().to_string())
                .part(cfg.field.clone(), part)
        };
        let resp = crate::llm::send_with_retry(
            http,
            || {
                let mut rb = http
                    .put(&chunk_url)
                    .header("FileToken", token.as_str())
                    .multipart(make_form());
                for (hk, hv) in &cfg.headers {
                    rb = rb.header(hk.as_str(), hv.as_str());
                }
                with_auth(rb, api_key, auth_style)
            },
            3,
        )
        .await?;
        let text = resp.text().await.unwrap_or_default();
        if let Ok(j) = serde_json::from_str::<Value>(&text) {
            if let Some(code) = j.get("code").and_then(|c| c.as_str()) {
                if !code.eq_ignore_ascii_case("success") && !code.is_empty() {
                    return Err(AppError::Provider(format!(
                        "第 {} 片上传失败：{}",
                        seq,
                        crate::llm::truncate(&text, 300)
                    )));
                }
            }
        }
    }

    // 平台用 md5 作为文件标识；如果它同时给了 URL，模板里写 {{url}} 也行
    Ok(apply_template(
        &cfg.ref_template,
        &[("url", md5), ("md5", md5), ("path", md5)],
    ))
}
