//! 项目工作区：目录布局 + 加载 / 保存。
//!
//! 目录结构（人类可读、可 git、可手工编辑）：
//! ```text
//! <项目>/
//!   project.json              项目清单
//!   bible.json                项目圣经（设定 / 人物 / 卖点）
//!   script/index.json         章节索引
//!   script/chapters/*.md      章节正文（Markdown）
//!   style/style.json          风格圣经
//!   storyboard/index.json     镜头表
//!   assets/index.json         资产清单
//!   assets/files/<资产>/...    人物三视图 / 场景 / 物件图片
//!   prompts/video_prompts.json 视频提示词与资产配对
//!   video/takes.json          生成结果清单
//!   video/files/*.mp4         生成的视频
//!   edit/timeline.json        时间线
//!   subtitles/                字幕文档
//!   checklist/checklist.json  核对清单
//!   .workbench/               可删的运行时数据（任务队列、会话、缓存）
//! ```

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::*;
use crate::store;

pub const MARKER: &str = "project.json";

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
}

impl ProjectPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn manifest(&self) -> PathBuf {
        self.root.join("project.json")
    }
    pub fn bible(&self) -> PathBuf {
        self.root.join("bible.json")
    }
    pub fn script_index(&self) -> PathBuf {
        self.root.join("script").join("index.json")
    }
    pub fn chapters_dir(&self) -> PathBuf {
        self.root.join("script").join("chapters")
    }
    pub fn chapter(&self, id: &str) -> PathBuf {
        self.chapters_dir().join(format!("{id}.md"))
    }
    pub fn style(&self) -> PathBuf {
        self.root.join("style").join("style.json")
    }
    pub fn storyboard(&self) -> PathBuf {
        self.root.join("storyboard").join("index.json")
    }
    pub fn assets_index(&self) -> PathBuf {
        self.root.join("assets").join("index.json")
    }
    pub fn asset_dir(&self, asset_id: &str) -> PathBuf {
        self.root.join("assets").join("files").join(asset_id)
    }
    pub fn asset_file(&self, asset_id: &str, view_id: &str, ext: &str) -> PathBuf {
        self.asset_dir(asset_id).join(format!("{view_id}.{ext}"))
    }
    pub fn prompts(&self) -> PathBuf {
        self.root.join("prompts").join("video_prompts.json")
    }
    pub fn takes(&self) -> PathBuf {
        self.root.join("video").join("takes.json")
    }
    pub fn video_dir(&self) -> PathBuf {
        self.root.join("video").join("files")
    }
    pub fn timeline(&self) -> PathBuf {
        self.root.join("edit").join("timeline.json")
    }
    pub fn render_dir(&self) -> PathBuf {
        self.root.join("edit").join("renders")
    }
    pub fn subtitles(&self) -> PathBuf {
        self.root.join("subtitles").join("index.json")
    }
    pub fn subtitle_dir(&self) -> PathBuf {
        self.root.join("subtitles").join("files")
    }
    pub fn checklist(&self) -> PathBuf {
        self.root.join("checklist").join("checklist.json")
    }
    pub fn workbench(&self) -> PathBuf {
        self.root.join(".workbench")
    }
    pub fn jobs_file(&self) -> PathBuf {
        self.workbench().join("jobs.json")
    }
    pub fn sessions_file(&self) -> PathBuf {
        self.workbench().join("agent_sessions.json")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.workbench().join("cache")
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.workbench().join("thumbs")
    }
    pub fn tmp_dir(&self) -> PathBuf {
        self.workbench().join("tmp")
    }

    pub fn is_project(&self) -> bool {
        self.manifest().exists()
    }

    pub fn ensure_layout(&self) -> Result<()> {
        for d in [
            self.chapters_dir(),
            self.root.join("style"),
            self.root.join("storyboard"),
            self.root.join("assets").join("files"),
            self.root.join("prompts"),
            self.video_dir(),
            self.root.join("edit"),
            self.subtitle_dir(),
            self.root.join("checklist"),
            self.workbench(),
            self.cache_dir(),
            self.thumbs_dir(),
            self.tmp_dir(),
        ] {
            store::ensure_dir(&d)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub paths: ProjectPaths,
    pub manifest: Manifest,
    pub bible: Bible,
    pub script: ScriptIndex,
    pub style: StyleState,
    pub shots: Vec<Shot>,
    pub assets: Vec<Asset>,
    pub prompts: Vec<VideoPrompt>,
    pub takes: Vec<VideoTake>,
    pub timeline: Timeline,
    pub subtitles: Vec<SubtitleDoc>,
    pub checklist: Checklist,
}

impl Project {
    pub fn root(&self) -> &Path {
        &self.paths.root
    }

    pub fn root_string(&self) -> String {
        self.paths.root.to_string_lossy().to_string()
    }

    /// 新建项目。`parent` 下会创建以项目名命名的子目录。
    pub fn create(parent: &Path, name: &str) -> Result<Self> {
        let safe: String = name
            .chars()
            .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
            .collect();
        let safe = safe.trim().trim_matches('.').to_string();
        if safe.is_empty() {
            return Err(AppError::invalid("项目名不能为空"));
        }
        let root = parent.join(&safe);
        let paths = ProjectPaths::new(root);
        if paths.is_project() {
            return Err(AppError::invalid(format!("目录已存在项目：{}", paths.root.display())));
        }
        paths.ensure_layout()?;

        let p = Self {
            paths,
            manifest: Manifest::new(&safe),
            bible: Bible::default(),
            script: ScriptIndex::default(),
            style: StyleState::default(),
            shots: vec![],
            assets: vec![],
            prompts: vec![],
            takes: vec![],
            timeline: Timeline::default(),
            subtitles: vec![],
            // 自定义检查项只在建项目时种一次，之后用户删了就是删了
            checklist: Checklist {
                items: crate::progress::default_items(),
            },
        };
        p.save_all()?;
        Ok(p)
    }

    pub fn open(root: &Path) -> Result<Self> {
        let paths = ProjectPaths::new(root.to_path_buf());
        if !paths.is_project() {
            return Err(AppError::NotFound(format!(
                "该目录不是工作台项目（缺少 {MARKER}）：{}",
                root.display()
            )));
        }
        paths.ensure_layout()?;
        let manifest = store::read_json_opt::<Manifest>(&paths.manifest())?
            .ok_or_else(|| AppError::NotFound("project.json 无法解析".into()))?;
        Ok(Self {
            manifest,
            bible: store::read_json_or_default(&paths.bible())?,
            script: store::read_json_or_default(&paths.script_index())?,
            style: store::read_json_or_default(&paths.style())?,
            shots: store::read_json_or_default(&paths.storyboard())?,
            assets: store::read_json_or_default(&paths.assets_index())?,
            prompts: store::read_json_or_default(&paths.prompts())?,
            takes: store::read_json_or_default(&paths.takes())?,
            timeline: store::read_json_opt::<Timeline>(&paths.timeline())?
                .unwrap_or_default(),
            subtitles: store::read_json_or_default(&paths.subtitles())?,
            checklist: store::read_json_or_default(&paths.checklist())?,
            paths,
        })
    }

    /* ------------------------------------------------------------ 读写 */

    pub fn save_all(&self) -> Result<()> {
        self.paths.ensure_layout()?;
        self.save_manifest()?;
        self.save_bible()?;
        self.save_script_index()?;
        self.save_style()?;
        self.save_shots()?;
        self.save_assets()?;
        self.save_prompts()?;
        self.save_takes()?;
        self.save_timeline()?;
        self.save_subtitles()?;
        self.save_checklist()?;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.manifest.updated_at = now_iso();
    }

    pub fn save_manifest(&self) -> Result<()> {
        store::write_json(&self.paths.manifest(), &self.manifest)
    }
    pub fn save_bible(&self) -> Result<()> {
        store::write_json(&self.paths.bible(), &self.bible)
    }
    pub fn save_script_index(&self) -> Result<()> {
        store::write_json(&self.paths.script_index(), &self.script)
    }
    pub fn save_style(&self) -> Result<()> {
        store::write_json(&self.paths.style(), &self.style)
    }
    pub fn save_shots(&self) -> Result<()> {
        store::write_json(&self.paths.storyboard(), &self.shots)
    }
    pub fn save_assets(&self) -> Result<()> {
        store::write_json(&self.paths.assets_index(), &self.assets)
    }
    pub fn save_prompts(&self) -> Result<()> {
        store::write_json(&self.paths.prompts(), &self.prompts)
    }
    pub fn save_takes(&self) -> Result<()> {
        store::write_json(&self.paths.takes(), &self.takes)
    }
    pub fn save_timeline(&self) -> Result<()> {
        store::write_json(&self.paths.timeline(), &self.timeline)
    }
    pub fn save_subtitles(&self) -> Result<()> {
        store::write_json(&self.paths.subtitles(), &self.subtitles)
    }
    pub fn save_checklist(&self) -> Result<()> {
        store::write_json(&self.paths.checklist(), &self.checklist)
    }

    /* ------------------------------------------------------------ 章节 */

    pub fn load_chapter(&self, chapter_id: &str) -> Result<Chapter> {
        let meta = self
            .script
            .chapters
            .iter()
            .find(|c| c.id == chapter_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("章节不存在：{chapter_id}")))?;
        let content = store::read_text_or_empty(&self.paths.chapter(chapter_id))?;
        Ok(Chapter { meta, content })
    }

    pub fn save_chapter_content(&self, chapter_id: &str, content: &str) -> Result<ChapterMeta> {
        store::write_text(&self.paths.chapter(chapter_id), content)?;
        let mut meta = self
            .script
            .chapters
            .iter()
            .find(|c| c.id == chapter_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("章节不存在：{chapter_id}")))?;
        meta.word_count = store::count_words(content);
        if !content.trim().is_empty() && matches!(meta.status, ChapterStatus::Empty) {
            meta.status = ChapterStatus::Draft;
        }
        meta.updated_at = now_iso();
        Ok(meta)
    }

    pub fn next_chapter_index(&self) -> u32 {
        self.script.chapters.iter().map(|c| c.index).max().unwrap_or(0) + 1
    }

    /// 给章节重新编号，保证 index 连续且与数组顺序一致。
    pub fn reindex_chapters(&mut self) {
        self.script.chapters.sort_by_key(|c| c.index);
        for (i, c) in self.script.chapters.iter_mut().enumerate() {
            c.index = i as u32 + 1;
        }
    }

    pub fn next_shot_index(&self, chapter_id: &str) -> u32 {
        self.shots
            .iter()
            .filter(|s| s.chapter_id == chapter_id)
            .map(|s| s.index)
            .max()
            .unwrap_or(0)
            + 1
    }

    /* ---------------------------------------------------------- 常用查询 */

    pub fn asset_by_name(&self, name: &str) -> Option<&Asset> {
        let n = name.trim();
        self.assets
            .iter()
            .find(|a| a.name == n || a.aliases.iter().any(|x| x == n))
    }

    /// 供 agent / 提示词拼装使用的、剥离了易变字段的资产摘要。
    pub fn asset_digest_lines(&self) -> Vec<String> {
        self.assets
            .iter()
            .map(|a| {
                let views: Vec<String> = a
                    .views
                    .iter()
                    .filter(|v| v.status == AssetStatus::Done)
                    .map(|v| v.label.clone())
                    .collect();
                let traits = if a.locked_traits.is_empty() {
                    String::new()
                } else {
                    format!("｜固定特征: {}", a.locked_traits.join("、"))
                };
                format!(
                    "- [{}] {}（{:?}）{}{}",
                    a.id,
                    a.name,
                    a.kind,
                    if views.is_empty() {
                        String::new()
                    } else {
                        format!("｜已有视图: {}", views.join("/"))
                    },
                    traits
                )
            })
            .collect()
    }
}
