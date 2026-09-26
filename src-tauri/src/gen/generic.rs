//! 配置驱动的通用 HTTP 适配器。**接新接口时不用改代码，只改 provider.options。**
//!
//! `options` 结构（除 `submit.body` 外全部可选，按需填）：
//!
//! ```jsonc
//! {
//!   "submit": {
//!     "method": "POST",
//!     "path": "/v1/images/generations",          // 相对 baseUrl，也可写完整 URL
//!     "headers": { "X-Foo": "bar" },
//!     "body": {                                  // 模板：{{变量}} 会被替换
//!       "model": "{{model}}",
//!       "prompt": "{{prompt}}",
//!       "negative_prompt": "{{negative}}",
//!       "width": "{{width}}",                    // 整个值就是一个变量时保留数字类型
//!       "height": "{{height}}",
//!       "seed": "{{seed}}",                      // 无值时该字段会被整个删掉
//!       "image": "{{image1}}",                   // 参考图，data-url 形式
//!       "duration": "{{duration}}"
//!     }
//!   },
//!   "taskIdPath": "/data/0/task_id",             // 有此字段 = 异步任务，进入轮询
//!   "poll": {
//!     "method": "GET",
//!     "path": "/v1/tasks/{{taskId}}",
//!     "statusPath": "/data/status",
//!     "successValues": ["succeeded", "success"],
//!     "failureValues": ["failed", "error"],
//!     "progressPath": "/data/progress",          // 0~100 或 0~1 都认
//!     "intervalSec": 5,
//!     "timeoutSec": 900
//!   },
//!   "result": {
//!     "urlPath": ["/data/0/url", "/output/url"], // 候选路径，取第一个非空
//!     "b64Path": ["/data/0/b64_json"],
//!     "filePath": ["/data/0/path"],
//!     "headers": { }                             // 下载结果文件时带的头
//!   },
//!   "maxAttempts": 3,
//!   "downloadTimeoutSec": 600
//! }
//! ```
//!
//! 认证方式用 `authStyle` 选：`bearer`（默认，`Authorization: Bearer xxx`）、
//! `raw`（`Authorization: xxx`，有些平台不带 `Bearer` 前缀）、`none`。
//!
//! 图片变量 `{{image1}}` `{{image2}}` … 默认给 data-url。**如果接口要求公网 URL**
//! （例如 ComfyUI 工作流平台的 `first_frame` / `ref_image_0`），在 options 里配一段 `upload`，
//! 本地图片会先传上去，`{{image1}}` 换成上传后的引用值。另外始终可用
//! `{{image1data}}`（data-url）、`{{image1raw}}`（纯 base64）。
//!
//! 可用变量：
//! - 文本参数：`prompt` `negative` `model` `seed` `apiKey`
//! - 尺寸：`width` `height` `size`（'WxH'）、`aspect`（走 `aspectValues` 映射，
//!   例如 `{"768x1344":"768p_portrait"}`）；`width`/`height`/`size` 都可被 options 里的同名项覆盖
//! - 时长：`duration`（原值）、`durationInt`（取整，视频接口常用）
//! - 图片：`image1`…`image9`（配了 upload 就是上传后的引用，否则是 data-url）、
//!   `image1data`（强制 data-url）、`image1raw`（纯 base64）、`image`（=image1）

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value};

use super::upload::{self, UploadConfig};
use super::{ref_image_b64, ref_image_data_url, GenOutput, GenProvider, GenRequest};
use crate::error::{AppError, Result};
use crate::jobs::JobCtx;
use crate::models::ProviderConfig;

pub struct GenericHttpProvider {
    cfg: ProviderConfig,
    api_key: Option<String>,
    http: reqwest::Client,
    upload: Option<UploadConfig>,
    /// bearer（默认）/ raw（直接放进 Authorization）/ none
    auth_style: String,
}

impl GenericHttpProvider {
    pub fn new(cfg: ProviderConfig, api_key: Option<String>, http: reqwest::Client) -> Result<Self> {
        if cfg.options.get("submit").is_none() {
            return Err(AppError::Config(format!(
                "provider「{}」是 generic-http 适配器，但缺少 options.submit 配置",
                cfg.name
            )));
        }
        let auth_style = cfg
            .options
            .get("authStyle")
            .and_then(|v| v.as_str())
            .unwrap_or("bearer")
            .to_string();
        let upload = UploadConfig::from_options(&cfg.options);
        Ok(Self {
            cfg,
            api_key,
            http,
            upload,
            auth_style,
        })
    }

    /// 按 authStyle 给请求带上认证头。
    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match (self.api_key.as_deref(), self.auth_style.as_str()) {
            (Some(k), "raw") => rb.header("Authorization", k),
            (Some(_), "none") => rb,
            (Some(k), _) => rb.bearer_auth(k),
            (None, _) => rb,
        }
    }

    fn opt(&self, key: &str) -> Option<&Value> {
        self.cfg.options.get(key)
    }

    async fn build_vars(&self, req: &GenRequest) -> Result<BTreeMap<String, Value>> {
        let mut vars: BTreeMap<String, Value> = BTreeMap::new();
        vars.insert("prompt".into(), Value::String(req.prompt.clone()));
        vars.insert("negative".into(), Value::String(req.negative.clone()));
        vars.insert("model".into(), Value::String(self.cfg.model.clone()));
        // width / height 可以被 provider 的 options 覆盖（有些接口只认固定尺寸）
        let ow = self.cfg.options.get("width").and_then(|v| v.as_u64());
        let oh = self.cfg.options.get("height").and_then(|v| v.as_u64());
        let w = ow.unwrap_or(req.width as u64);
        let h = oh.unwrap_or(req.height as u64);
        if w > 0 {
            vars.insert("width".into(), Value::from(w));
        }
        if h > 0 {
            vars.insert("height".into(), Value::from(h));
        }
        if req.width > 0 && req.height > 0 {
            // {{size}} 优先用 options.size，否则用 'WxH'
            let size = self
                .cfg
                .options
                .get("size")
                .and_then(|v| v.as_str())
                .map(|x| x.to_string())
                .unwrap_or_else(|| format!("{w}x{h}"));
            vars.insert("size".into(), Value::String(size.clone()));
            // {{aspect}} 走 options.aspectValues 映射，例如 {"768x1344":"768p_portrait"}
            if let Some(m) = self
                .cfg
                .options
                .get("aspectValues")
                .and_then(|v| v.as_object())
            {
                if let Some(found) = m
                    .get(&size)
                    .or_else(|| m.get(&format!("{w}x{h}")))
                    .and_then(|v| v.as_str())
                {
                    vars.insert("aspect".into(), Value::String(found.to_string()));
                }
            }
        }
        if let Some(s) = req.seed {
            vars.insert("seed".into(), Value::from(s));
        }
        if let Some(d) = req.duration_sec {
            vars.insert("duration".into(), Value::from(d));
            // 很多视频接口只收整数秒
            vars.insert("durationInt".into(), Value::from(d.round().max(1.0) as i64));
        }
        if let Some(k) = &self.api_key {
            vars.insert("apiKey".into(), Value::String(k.clone()));
        }
        for (i, p) in req.ref_images.iter().enumerate() {
            let n = i + 1;
            // image{n}data / image{n}raw 永远可用
            if let Ok(u) = ref_image_data_url(p) {
                vars.insert(format!("image{n}data"), Value::String(u.clone()));
                vars.insert(format!("image{n}"), Value::String(u));
            }
            if let Ok(b) = ref_image_b64(p) {
                vars.insert(format!("image{n}raw"), Value::String(b));
            }
            // 配了 upload 就把 image{n} 换成上传后的引用值
            if let Some(u) = &self.upload {
                let r = upload::upload_file(
                    u,
                    &self.http,
                    &self.cfg.base_url,
                    self.api_key.as_deref(),
                    &self.auth_style,
                    p,
                )
                .await?;
                vars.insert(format!("image{n}"), Value::String(r));
            }
            if i == 0 {
                if let Some(v) = vars.get("image1").cloned() {
                    vars.insert("image".into(), v);
                }
            }
        }
        if let Some(extra) = req.extra.as_object() {
            for (k, v) in extra {
                vars.insert(k.clone(), v.clone());
            }
        }
        Ok(vars)
    }

    fn url_for(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            crate::llm::join_url(&self.cfg.base_url, path)
        }
    }

    fn headers_from(&self, node: Option<&Value>) -> Vec<(String, String)> {
        let mut out = vec![];
        if let Some(obj) = node.and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    out.push((k.clone(), s.to_string()));
                }
            }
        }
        out
    }

    async fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value> {
        let m = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .map_err(|_| AppError::invalid(format!("非法 HTTP 方法：{method}")))?;
        let resp = crate::llm::send_with_retry(
            &self.http,
            || {
                let mut rb = self.http.request(m.clone(), url);
                rb = self.auth(rb);
                for (k, v) in headers {
                    rb = rb.header(k.as_str(), v.as_str());
                }
                if let Some(b) = &body {
                    rb = rb.json(b);
                }
                rb
            },
            3,
        )
        .await?;
        let text = resp.text().await?;
        serde_json::from_str::<Value>(&text).map_err(|_| {
            AppError::Provider(format!(
                "返回的不是 JSON：{}",
                crate::llm::truncate(&text, 600)
            ))
        })
    }
}

/// 用 JSON Pointer 列表依次尝试取值。
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

fn path_list(node: Option<&Value>) -> Vec<String> {
    match node {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

enum Skip {
    Yes,
}

/// 模板渲染：字符串整体就是一个变量时保留原始类型（数字/布尔），
/// 变量缺失时返回 Skip，让调用方把这个字段删掉 —— 这样可选参数不会以空值发出去。
fn render(value: &Value, vars: &BTreeMap<String, Value>) -> std::result::Result<Value, Skip> {
    match value {
        Value::String(s) => {
            let t = s.trim();
            if let Some(inner) = t.strip_prefix("{{").and_then(|x| x.strip_suffix("}}")) {
                return match vars.get(inner.trim()) {
                    Some(Value::String(x)) if x.is_empty() => Err(Skip::Yes),
                    Some(v) => Ok(v.clone()),
                    None => Err(Skip::Yes),
                };
            }
            let mut out = s.clone();
            let mut missing = false;
            for (k, v) in vars {
                let token = format!("{{{{{k}}}}}");
                if out.contains(&token) {
                    let rep = match v {
                        Value::String(x) => x.clone(),
                        other => other.to_string(),
                    };
                    out = out.replace(&token, &rep);
                }
            }
            // 还有没被替换掉的变量 -> 认为缺值
            if out.contains("{{") {
                missing = true;
            }
            if missing && out.trim().is_empty() {
                Err(Skip::Yes)
            } else {
                Ok(Value::String(out))
            }
        }
        Value::Array(a) => {
            let mut out = vec![];
            for x in a {
                if let Ok(v) = render(x, vars) {
                    out.push(v);
                }
            }
            Ok(Value::Array(out))
        }
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, v) in o {
                if let Ok(rv) = render(v, vars) {
                    m.insert(k.clone(), rv);
                }
            }
            Ok(Value::Object(m))
        }
        other => Ok(other.clone()),
    }
}

#[async_trait]
impl GenProvider for GenericHttpProvider {
    fn adapter(&self) -> &'static str {
        "generic-http"
    }

    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn generate(&self, ctx: &JobCtx, req: GenRequest) -> Result<GenOutput> {
        let vars = self.build_vars(&req).await?;
        let submit = self
            .opt("submit")
            .ok_or_else(|| AppError::Config("缺少 options.submit".into()))?;

        let method = submit
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("POST");
        let path = submit.get("path").and_then(|p| p.as_str()).unwrap_or("/");
        let url = self.url_for(path);
        let headers = self.headers_from(submit.get("headers"));
        let body = match submit.get("body") {
            Some(tpl) => Some(render(tpl, &vars).unwrap_or(Value::Object(Map::new()))),
            None => None,
        };

        ctx.progress(0.05, "提交生成请求…");
        let mut resp = self.send(method, &url, &headers, body).await?;

        // 异步任务：轮询直到完成
        if let Some(task_id_paths) = self.opt("taskIdPath").map(|v| path_list(Some(v))) {
            if !task_id_paths.is_empty() {
                let task_id = pick(&resp, &task_id_paths)
                    .map(|v| match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .ok_or_else(|| {
                        AppError::Provider(format!(
                            "提交成功但没在 {} 找到任务 ID：{}",
                            task_id_paths.join(", "),
                            crate::llm::truncate(&resp.to_string(), 400)
                        ))
                    })?;
                resp = self.poll(ctx, &task_id).await?;
            }
        }

        ctx.progress(0.9, "下载结果…");
        self.fetch_result(ctx, &resp).await
    }
}

impl GenericHttpProvider {
    async fn poll(&self, ctx: &JobCtx, task_id: &str) -> Result<Value> {
        let poll = self
            .opt("poll")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let method = poll.get("method").and_then(|m| m.as_str()).unwrap_or("GET");
        let path = poll
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("/tasks/{{taskId}}");
        let interval = poll
            .get("intervalSec")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .max(1);
        let timeout = poll
            .get("timeoutSec")
            .and_then(|v| v.as_u64())
            .unwrap_or(900)
            .max(10);
        let success: Vec<String> = path_list(poll.get("successValues"))
            .into_iter()
            .chain(["succeeded", "success", "completed", "done"].map(String::from))
            .collect();
        let failure: Vec<String> = path_list(poll.get("failureValues"))
            .into_iter()
            .chain(["failed", "failure", "error", "canceled"].map(String::from))
            .collect();
        let status_paths = path_list(poll.get("statusPath"));
        let progress_paths = path_list(poll.get("progressPath"));
        let headers = self.headers_from(poll.get("headers"));

        let url = self.url_for(&path.replace("{{taskId}}", task_id));

        let started = std::time::Instant::now();
        loop {
            ctx.check_cancel()?;
            if started.elapsed() > Duration::from_secs(timeout) {
                return Err(AppError::Provider(format!(
                    "任务 {task_id} 轮询超时（{timeout} 秒）"
                )));
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
            ctx.check_cancel()?;

            let v = self.send(method, &url, &headers, None).await?;
            if let Some(p) = pick(&v, &progress_paths) {
                if let Some(f) = p.as_f64() {
                    let frac = if f > 1.5 { f / 100.0 } else { f };
                    ctx.progress(0.1 + (frac as f32) * 0.75, "生成中…");
                }
            }
            let status = pick(&v, &status_paths)
                .map(|s| match s {
                    Value::String(x) => x,
                    o => o.to_string(),
                })
                .unwrap_or_default()
                .to_ascii_lowercase();
            if failure.iter().any(|f| status.contains(&f.to_ascii_lowercase())) {
                return Err(AppError::Provider(format!(
                    "任务失败（状态 {status}）：{}",
                    crate::llm::truncate(&v.to_string(), 500)
                )));
            }
            if success.iter().any(|s| status.contains(&s.to_ascii_lowercase())) || status.is_empty() {
                return Ok(v);
            }
        }
    }

    async fn fetch_result(&self, ctx: &JobCtx, resp: &Value) -> Result<GenOutput> {
        let result = self.opt("result").cloned().unwrap_or(Value::Null);
        let url_paths = path_list(result.get("urlPath"));
        let b64_paths = path_list(result.get("b64Path"));
        let file_paths = path_list(result.get("filePath"));
        let dl_headers = self.headers_from(result.get("headers"));

        if let Some(p) = pick(resp, &file_paths) {
            if let Some(s) = p.as_str() {
                return Ok(GenOutput::File(PathBuf::from(s)));
            }
        }
        if let Some(p) = pick(resp, &b64_paths) {
            if let Some(s) = p.as_str() {
                use base64::Engine;
                let clean = s.split(',').last().unwrap_or(s);
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(clean.trim())
                    .map_err(|e| AppError::Provider(format!("base64 解码失败: {e}")))?;
                return Ok(GenOutput::Bytes(bytes));
            }
        }
        if let Some(p) = pick(resp, &url_paths) {
            let url = match p {
                Value::String(s) => s,
                Value::Array(a) => a
                    .first()
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                o => o.to_string(),
            };
            if url.is_empty() {
                return Err(AppError::Provider("结果 URL 为空".into()));
            }
            let resp = crate::llm::send_with_retry(
                &self.http,
                || {
                    let mut rb = self.http.get(&url);
                    for (k, v) in &dl_headers {
                        rb = rb.header(k.as_str(), v.as_str());
                    }
                    rb
                },
                3,
            )
            .await?;
            let total = resp.content_length().unwrap_or(0);
            let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
            use futures_util::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut got: u64 = 0;
            while let Some(chunk) = stream.next().await {
                ctx.check_cancel()?;
                let c = chunk.map_err(AppError::from)?;
                got += c.len() as u64;
                buf.extend_from_slice(&c);
                if total > 0 {
                    ctx.progress(0.9 + (got as f32 / total as f32) * 0.1, "下载结果…");
                }
            }
            return Ok(GenOutput::Bytes(buf));
        }
        Err(AppError::Provider(format!(
            "无法从返回结果里取到文件。请检查 options.result 配置。原始返回：{}",
            crate::llm::truncate(&resp.to_string(), 700)
        )))
    }
}
