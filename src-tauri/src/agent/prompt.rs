//! 缓存友好的提示词装配。
//!
//! ## 为什么这样分层
//!
//! 服务端前缀缓存（Anthropic 显式断点 / OpenAI 兼容端点的自动前缀缓存）都是
//! **按前缀逐字节匹配**的：前面任何一个字符变了，从那里往后全部重新计费。
//! 所以真正决定命中率的不是"断点打在哪"，而是"前缀里有没有会变的东西"。
//!
//! 本工作台的做法是把提示词切成两层：
//!
//! * **冻结前缀**（system 分层 + 会话首条上下文快照）
//!   在会话创建时构建一次，之后整场对话逐字节不变。变化频率不同的内容各自成段：
//!   `L0 核心指令` → `L1 面板职责` → `L2 项目圣经（含风格）` → `L3 资产索引`。
//!   这样即使用户改了资产，L0~L2 的缓存依然有效，只有 L3 往后失效。
//!
//! * **增量尾部**（工具结果、新对话轮次）
//!   永远只追加在消息列表末尾，绝不回头改历史消息。
//!
//! 于是「面板状态变了」这件事不会污染缓存 —— agent 需要新数据时通过
//! `project_snapshot` 之类的工具去读，结果落在尾部，不影响前面的前缀。
//!
//! 用户手动点「刷新上下文」会重建前缀，那时才付出一次缓存失效的代价。

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::models::*;
use crate::project::Project;
use crate::store::fingerprint;

pub const LAYER_CORE: &str = "L0 核心指令";
pub const LAYER_PANEL: &str = "L1 面板职责";
pub const LAYER_BIBLE: &str = "L2 项目圣经";
pub const LAYER_ASSETS: &str = "L3 资产索引";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    pub text: String,
    /// 是否挂 Anthropic 显式缓存断点
    pub cache: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerReport {
    pub name: String,
    pub chars: u64,
    pub est_tokens: u64,
    pub hash: String,
    pub breakpoint: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefixReport {
    pub fingerprint: String,
    /// 与上次请求相比，冻结前缀是否没变（没变 = 这一轮能吃到缓存）
    pub stable: bool,
    pub layers: Vec<LayerReport>,
    pub est_tokens: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenPrefix {
    pub layers: Vec<Layer>,
    /// 会话首条用户消息里的上下文快照，同样冻结
    pub context: String,
    pub fingerprint: String,
    pub frozen_at: String,
}

impl FrozenPrefix {
    pub fn layer_texts(&self) -> Vec<crate::llm::SystemBlock> {
        self.layers
            .iter()
            .map(|l| crate::llm::SystemBlock {
                name: l.name.clone(),
                text: l.text.clone(),
                cache_breakpoint: l.cache,
            })
            .collect()
    }

    pub fn report(&self, stable: bool) -> PrefixReport {
        let layers: Vec<LayerReport> = self
            .layers
            .iter()
            .map(|l| LayerReport {
                name: l.name.clone(),
                chars: l.text.chars().count() as u64,
                est_tokens: est_tokens(&l.text),
                hash: fingerprint(&l.text),
                breakpoint: l.cache,
            })
            .collect();
        let total: u64 = layers.iter().map(|l| l.est_tokens).sum::<u64>()
            + est_tokens(&self.context);
        PrefixReport {
            fingerprint: self.fingerprint.clone(),
            stable,
            layers,
            est_tokens: total,
            note: if stable {
                "冻结前缀未变化，本轮可复用服务端缓存".into()
            } else {
                "冻结前缀已变化，本轮会重新写入缓存".into()
            },
        }
    }
}

pub fn est_tokens(s: &str) -> u64 {
    // 粗略估算：中文约 1 字 0.9 token，英文约 4 字符 1 token。仅用于 UI 展示。
    let mut cjk = 0u64;
    let mut other = 0u64;
    for ch in s.chars() {
        if matches!(ch as u32, 0x3000..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other / 4
}

pub fn build_frozen_prefix(project: &Project, panel: PanelId) -> Result<FrozenPrefix> {
    let layers = vec![
        Layer {
            name: LAYER_CORE.into(),
            text: core_prompt(),
            cache: true,
        },
        Layer {
            name: LAYER_PANEL.into(),
            text: panel_prompt(panel).to_string(),
            cache: true,
        },
        Layer {
            name: LAYER_BIBLE.into(),
            text: bible_text(project),
            cache: true,
        },
        Layer {
            name: LAYER_ASSETS.into(),
            text: assets_text(project),
            cache: true,
        },
    ];
    let context = context_snapshot(project, panel);
    let joined = format!(
        "{}\n\n===CONTEXT===\n{}",
        layers
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        context
    );
    Ok(FrozenPrefix {
        fingerprint: fingerprint(&joined),
        layers,
        context,
        frozen_at: now_iso(),
    })
}

/* ================================================================ 各层内容 */

fn core_prompt() -> String {
    r#"你是「短剧工作台」里的制作协作 agent。工作台把一部短剧的生产拆成九个面板：
剧本 → 风格 → 分镜 → 资产 → 视频提示词 → 生视频 → 剪辑 → 字幕 → Checklist。
用户正在某个面板里工作，你只在这个面板的职责范围内协作。

工作准则：
1. 只用中文回复，直接给结果，不要寒暄、不要复述用户已经说过的话。
2. 一切对项目数据的修改都必须通过工具完成。不要在回复里写"我已经改好了"却没有真正调用工具。
3. 动手之前先想清楚要改哪些数据；能一次批量改完的，不要拆成很多次工具调用。
4. 需要了解现状时，先用只读工具（如 project_snapshot、storyboard_list_shots）确认，不要凭记忆猜。
5. 工具报错时如实说明错误内容，不要假装成功；必要时换个思路重试一次。
6. 不要臆造 ID。所有 ID 都必须来自工具返回的数据。
7. 内容要具体、可直接投产：写提示词就写完整可用的提示词，写分镜就写清景别、运镜、时长、动作、台词。
8. 涉及花钱的操作（生图、生视频）要先说明将要生成什么、大概几张/几条，再调用工具。
9. 单次回复保持精炼：先说结论和做了什么，需要展开时再用要点列表。

上下文说明：你的系统提示里包含一份**项目圣经**与**资产索引**的冻结快照，
它们是本场对话开始时拍下的。如果用户说数据已经变了，或你需要最新状态，
请调用对应工具重新读取——工具返回的才是当前真实数据。"#
        .to_string()
}

fn panel_prompt(panel: PanelId) -> &'static str {
    match panel {
        PanelId::Script => {
            r#"当前面板：**剧本**。
职责：把故事拆成章节，产出每章的大纲与正文。
规范：
- 章节数是用户指定的；追加章节时保持与已有章节的节奏、人称、语气一致。
- 每章正文按短剧节奏写：开场 3 秒抓人，中间有小反转，结尾留钩子。
- 写正文用 Markdown，可包含场景标题、动作描写与台词（台词用「角色名：内容」格式）。
- 写完一章后顺手更新该章 summary 与出场人物，方便后面分镜和资产面板使用。
常用工具：script_set_chapters、script_append_chapter、script_write_chapter、bible_update。"#
        }
        PanelId::Style => {
            r#"当前面板：**风格**。
职责：定义整部剧的画面风格圣经，供后续所有生图、生视频复用。
规范：
- 风格描述要包含：影像质感、色彩与色调、光线、镜头与焦段倾向、胶片/颗粒感、常见构图。
- 如果用户选了预设，可以在此基础上按剧情微调，并把最终结果写回 style_set。
- negative（负面词）要写清要避免什么，例如畸变、多指、塑料感、过曝、文字水印。
- 风格一旦定稿就尽量别频繁改：它是下游所有资产的共享前缀，改了会导致资产需要重出。
常用工具：style_list_presets、style_apply_preset、style_set。"#
        }
        PanelId::Storyboard => {
            r#"当前面板：**分镜**。
职责：把剧本拆成可拍摄的镜头表。
规范：
- 每个镜头必须写清：景别、机位与运镜、时长（秒）、场景地点、时间、出场人物、涉及道具、画面动作。
- 有台词就填 dialogue，没有就留空；画外音填 narration。
- 单镜头时长控制在 2~6 秒；短剧节奏快，避免长镜头。
- 人物、道具字段请使用资产索引里已有的名字，这样下游能自动配对图片。
- 一次把一整章的镜头写完，用 storyboard_write_shots 覆盖式写入。
常用工具：storyboard_list_shots、storyboard_write_shots、storyboard_append_shots、storyboard_update_shot。"#
        }
        PanelId::Asset => {
            r#"当前面板：**资产**。
职责：把剧本与分镜里出现的所有视觉元素整理成资产，并规划/生成参考图。
规范：
- 资产分四类为主要：人物（character）、场景（scene）、道具（prop）、服装（costume）。
- 人物至少要有正面、侧面、背面三视图；场景要有多个角度（全景/中景/局部）。
- 人物的 lockedTraits 用来锁定跨图一致性（例如"左眉有疤""戴银色细框眼镜"），一旦确定不要随意删。
- 生成图片前先把 prompt 写好：风格统一带上面板 2 定下的风格词，再写该资产自身的特征。
- 生成图片会消耗额度，动手前先说明要出哪几张。
常用工具：asset_list、asset_upsert、asset_plan_views、asset_generate_view、asset_generate_missing。"#
        }
        PanelId::Prompt => {
            r#"当前面板：**视频提示词**。
职责：为每个分镜写视频生成提示词，并把需要的图片资产配对上去。
规范：
- 提示词要描述**运动**而不是静态画面：主体动作、镜头运动、变化过程、结束状态。
- 明确首帧（firstFrame）与尾帧（lastFrame）要引用哪张资产图，能做首尾帧控制的模型效果会好很多。
- 时长与分镜保持一致；运镜字段与分镜的 cameraMove 对齐。
- 配对资产时优先复用已有的三视图/场景图，不要重复生成新图。
常用工具：prompt_list、prompt_upsert、prompt_bind_assets、prompt_autofill_from_shots。"#
        }
        PanelId::Video => {
            r#"当前面板：**生视频**。
职责：根据视频提示词与配对好的资产图，调用视频模型出片。
规范：
- 生成前先检查这个镜头是否已经有提示词与首帧图；缺什么先补什么。
- 一次不要提交太多任务，按章节或按场次分批，方便用户中途检查效果。
- 同一条提示词可以出多条 take 供挑选，seed 不同结果不同。
- 失败的镜头要说清失败原因（提示词被拒、超时、额度不足等）。
常用工具：video_list_takes、video_generate、video_generate_batch、prompt_upsert。"#
        }
        PanelId::Edit => {
            r#"当前面板：**剪辑**。
职责：把生成的视频按镜头顺序铺到时间线上，做拼接、裁切、音量调整并导出。
规范：
- 主视频轨按镜头顺序排列；每个片段记录来源 take，便于回溯重出。
- 调整时以"秒"为单位，注意片段之间的衔接不要出现黑帧。
- 导出前先确认时间线时长与镜头总时长一致。
常用工具：edit_get_timeline、edit_build_from_takes、edit_append_clip、edit_update_clip、edit_remove_clip、edit_render。"#
        }
        PanelId::Subtitle => {
            r#"当前面板：**字幕**。
职责：用本地 whisper 模型把视频/音频转写成字幕，并按需校对。
规范：
- 转写走本地推理，不上传素材；是否启用显卡加速由用户在设置里控制。
- 中文短剧建议用 large-v3-turbo 及以上模型；对白密集时开启词级时间戳。
- 转写完成后检查断句：单条字幕不超过约 18 个汉字，阅读时长不少于 1 秒。
常用工具：subtitle_list、subtitle_transcribe、subtitle_update_cues。"#
        }
        PanelId::Checklist => {
            r#"当前面板：**Checklist**。
职责：核对每个面板的工作是否完成，把没完成的项指出来。
规范：
- 面板完成度由数据自动判定（自动项），你不要去改自动项。
- 你可以新增自定义检查项，或勾选/取消非自动项。
- 汇报时按面板分组，先说没完成的，再说下一步建议动作。
常用工具：checklist_report、checklist_add、checklist_toggle。"#
        }
    }
}

fn bible_text(project: &Project) -> String {
    let m = &project.manifest;
    let b = &project.bible;
    let s = &project.style;
    let mut out = String::from("# 项目圣经（会话开始时冻结）\n\n## 项目\n");
    out.push_str(&format!(
        "- 名称：{}\n- 题材：{}\n- 一句话卖点：{}\n- 计划集数/章节数：{}\n- 画幅：{}\n",
        nz(&m.name),
        nz(&m.genre),
        nz(&m.logline),
        if m.episode_count_hint == 0 {
            "未定".to_string()
        } else {
            m.episode_count_hint.to_string()
        },
        nz(&m.aspect_ratio),
    ));
    out.push_str("\n## 故事\n");
    out.push_str(&format!("- 梗概：{}\n", nz(&b.synopsis)));
    if !b.selling_points.is_empty() {
        out.push_str(&format!("- 卖点：{}\n", b.selling_points.join("；")));
    }
    out.push_str(&format!("- 世界观：{}\n", nz(&b.world)));
    out.push_str(&format!("- 基调：{}\n", nz(&b.tone)));
    out.push_str(&format!("- 目标观众：{}\n", nz(&b.audience)));
    if !b.notes.is_empty() {
        out.push_str(&format!("- 备注：{}\n", b.notes));
    }

    out.push_str("\n## 人物\n");
    if b.characters.is_empty() {
        out.push_str("（尚未建立人物卡）\n");
    } else {
        for c in &b.characters {
            out.push_str(&format!(
                "- {}（{}，{}）\n  - 外形：{}\n  - 性格：{}\n  - 弧光：{}\n",
                c.name,
                nz(&c.role),
                nz(&c.age),
                nz(&c.appearance),
                nz(&c.personality),
                nz(&c.arc)
            ));
            if !c.notes.is_empty() {
                out.push_str(&format!("  - 备注：{}\n", c.notes));
            }
        }
    }

    out.push_str("\n## 画面风格圣经\n");
    out.push_str(&format!("- 风格名：{}\n", nz(&s.spec.name)));
    out.push_str(&format!("- 风格提示词：{}\n", nz(&s.spec.prompt)));
    out.push_str(&format!("- 负面词：{}\n", nz(&s.spec.negative)));
    if !s.spec.palette.is_empty() {
        out.push_str(&format!("- 色调：{}\n", s.spec.palette.join("、")));
    }
    out.push_str(&format!("- 光线：{}\n", nz(&s.spec.lighting)));
    out.push_str(&format!("- 镜头/焦段：{}\n", nz(&s.spec.lens)));
    out.push_str(&format!("- 质感：{}\n", nz(&s.spec.film_stock)));
    out.push_str(&format!("- 画幅：{}\n", nz(&s.spec.aspect_ratio)));
    out.push_str(&format!("- 运动风格：{}\n", nz(&s.spec.motion_style)));
    out
}

fn assets_text(project: &Project) -> String {
    let mut out = String::from("# 资产索引（会话开始时冻结，共 ");
    out.push_str(&project.assets.len().to_string());
    out.push_str(" 项）\n");
    if project.assets.is_empty() {
        out.push_str("（尚未建立任何资产）\n");
        return out;
    }
    for kind in [
        AssetKind::Character,
        AssetKind::Scene,
        AssetKind::Prop,
        AssetKind::Costume,
        AssetKind::Vehicle,
        AssetKind::Effect,
        AssetKind::Other,
    ] {
        let items: Vec<&Asset> = project.assets.iter().filter(|a| a.kind == kind).collect();
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {}\n", kind_label(kind)));
        for a in items {
            out.push_str(&format!("- [{}] {}", a.id, a.name));
            if !a.aliases.is_empty() {
                out.push_str(&format!("（别名：{}）", a.aliases.join("、")));
            }
            out.push('\n');
            if !a.description.is_empty() {
                out.push_str(&format!("  - 描述：{}\n", a.description));
            }
            if !a.locked_traits.is_empty() {
                out.push_str(&format!("  - 固定特征：{}\n", a.locked_traits.join("、")));
            }
            let views: Vec<String> = a
                .views
                .iter()
                .map(|v| {
                    format!(
                        "{}/{}",
                        v.label,
                        match v.status {
                            AssetStatus::Done => "已出图",
                            AssetStatus::Failed => "失败",
                            AssetStatus::Running => "生成中",
                            AssetStatus::Queued => "排队中",
                            AssetStatus::Planned => "待生成",
                        }
                    )
                })
                .collect();
            if !views.is_empty() {
                out.push_str(&format!("  - 视图：{}\n", views.join("，")));
            }
        }
    }
    out
}

/// 会话首条消息里的上下文快照。与 system 一起冻结，保证整场对话前缀不变。
pub fn context_snapshot(project: &Project, panel: PanelId) -> String {
    let mut out = String::from("【上下文快照】以下是本场对话开始时的工作台状态。\n");
    out.push_str(&format!("- 当前面板：{}\n", panel.as_str()));
    out.push_str(&format!("- 项目目录：{}\n", project.root_string()));
    out.push_str(&format!(
        "- 项目规模：章节 {}，镜头 {}，资产 {}，视频提示词 {}，已生成视频 {}，时间线 {} 轨\n",
        project.script.chapters.len(),
        project.shots.len(),
        project.assets.len(),
        project.prompts.len(),
        project.takes.len(),
        project.timeline.tracks.len(),
    ));

    if !project.script.chapters.is_empty() {
        out.push_str("\n章节现状：\n");
        for c in &project.script.chapters {
            let shots = project
                .shots
                .iter()
                .filter(|s| s.chapter_id == c.id)
                .count();
            out.push_str(&format!(
                "- 第{}章 {}｜{} 字｜镜头 {}｜状态 {:?}\n",
                c.index,
                c.title,
                c.word_count,
                shots,
                c.status
            ));
        }
    }

    match panel {
        PanelId::Storyboard => {
            if let Some(last) = project.script.chapters.last() {
                out.push_str(&format!(
                    "\n分镜面板提示：最后一章是「{}」（{}），它已有 {} 个镜头。\n",
                    last.title,
                    last.id,
                    project
                        .shots
                        .iter()
                        .filter(|s| s.chapter_id == last.id)
                        .count()
                ));
            }
        }
        PanelId::Prompt | PanelId::Video => {
            let no_prompt = project
                .shots
                .iter()
                .filter(|s| !project.prompts.iter().any(|p| p.shot_id == s.id))
                .count();
            out.push_str(&format!(
                "\n视频阶段提示：{} 个镜头还没有对应的视频提示词。\n",
                no_prompt
            ));
        }
        PanelId::Edit => {
            out.push_str(&format!(
                "\n时间线：{} 秒 / {} fps / {}x{}\n",
                project.timeline.duration_sec,
                project.timeline.fps,
                project.timeline.width,
                project.timeline.height
            ));
        }
        PanelId::Subtitle => {
            out.push_str(&format!(
                "\n已生成 {} 份字幕文档。\n",
                project.subtitles.len()
            ));
        }
        PanelId::Asset => {
            let missing: Vec<String> = project
                .assets
                .iter()
                .filter(|a| a.views.is_empty())
                .map(|a| a.name.clone())
                .collect();
            if !missing.is_empty() {
                out.push_str(&format!(
                    "\n还没规划任何视图的资产：{}\n",
                    missing.join("、")
                ));
            }
        }
        _ => {}
    }
    out
}

pub fn kind_label(k: AssetKind) -> &'static str {
    match k {
        AssetKind::Character => "人物",
        AssetKind::Scene => "场景",
        AssetKind::Prop => "道具",
        AssetKind::Costume => "服装",
        AssetKind::Vehicle => "载具",
        AssetKind::Effect => "特效",
        AssetKind::Other => "其他",
    }
}

pub fn view_kind_label(k: ViewKind) -> &'static str {
    match k {
        ViewKind::Front => "正面",
        ViewKind::Side => "侧面",
        ViewKind::Back => "背面",
        ViewKind::ThreeQuarter => "四分之三侧",
        ViewKind::FullBody => "全身",
        ViewKind::CloseUp => "特写",
        ViewKind::Wide => "全景",
        ViewKind::Medium => "中景",
        ViewKind::BirdView => "俯视",
        ViewKind::Custom => "自定义",
    }
}

fn nz(s: &str) -> String {
    if s.trim().is_empty() {
        "（未填写）".to_string()
    } else {
        s.to_string()
    }
}
