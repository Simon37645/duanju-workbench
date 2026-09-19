//! 工具入参 JSON Schema。
//!
//! 这些 schema 会原样发给模型，属于**稳定前缀**的一部分：
//! 顺序、字段名、描述文字的任何改动都会让服务端缓存失效一次。
//! 所以这里只写结构性说明，不要把易变内容（ID、数量）拼进来。

use serde_json::{json, Value};

fn obj(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
    })
}

fn s(desc: &str) -> Value {
    json!({ "type": "string", "description": desc })
}

fn arr(desc: &str, item: Value) -> Value {
    json!({ "type": "array", "description": desc, "items": item })
}

pub fn empty() -> Value {
    json!({ "type": "object", "properties": {}, "required": [] })
}

pub fn sections() -> Value {
    obj(
        json!({
            "sections": arr(
                "要读取的部分，留空则返回全景概览。可选：manifest, bible, chapters, style, shots, assets, prompts, takes, timeline, subtitles, checklist, progress",
                json!({ "type": "string" })
            )
        }),
        &[],
    )
}

pub fn checklist_add() -> Value {
    obj(
        json!({
            "panel": s("所属面板：script/style/storyboard/asset/prompt/video/edit/subtitle/checklist"),
            "text": s("检查项内容"),
            "note": s("备注，可选")
        }),
        &["panel", "text"],
    )
}

pub fn checklist_toggle() -> Value {
    obj(
        json!({
            "id": s("检查项 id"),
            "done": json!({ "type": "boolean", "description": "true 勾选，false 取消" }),
            "note": s("备注，可选")
        }),
        &["id"],
    )
}

pub fn bible_update() -> Value {
    obj(
        json!({
            "synopsis": s("故事梗概"),
            "sellingPoints": arr("卖点/爽点", json!({ "type": "string" })),
            "world": s("世界观与设定"),
            "tone": s("整体基调"),
            "audience": s("目标观众"),
            "notes": s("其他备注"),
            "characters": arr(
                "人物卡数组，按 name 匹配，已存在则更新",
                obj(
                    json!({
                        "name": s("姓名"),
                        "aliases": arr("别名", json!({ "type": "string" })),
                        "role": s("在剧中的角色定位，如 女主/反派"),
                        "age": s("年龄段"),
                        "appearance": s("外形描写"),
                        "personality": s("性格"),
                        "arc": s("人物弧光/变化")
                    }),
                    &["name"],
                )
            )
        }),
        &[],
    )
}

pub fn script_set_chapters() -> Value {
    obj(
        json!({
            "chapters": arr(
                "章节列表，按顺序。会覆盖现有章节列表（同序号章节保留 id 与正文）",
                obj(
                    json!({
                        "title": s("章节标题"),
                        "summary": s("本章大纲，2~4 句")
                    }),
                    &["title"]
                )
            )
        }),
        &["chapters"],
    )
}

pub fn script_append_chapter() -> Value {
    obj(
        json!({
            "title": s("章节标题"),
            "summary": s("本章大纲"),
            "content": s("章节正文（Markdown），可选")
        }),
        &[],
    )
}

pub fn chapter_ref(with_content: bool) -> Value {
    let mut props = serde_json::Map::new();
    props.insert("chapterId".into(), s("章节 id，如 ch_001"));
    props.insert(
        "index".into(),
        json!({ "type": "integer", "description": "章节序号，从 1 开始" }),
    );
    if with_content {
        props.insert("content".into(), s("章节正文（Markdown）"));
        props.insert("summary".into(), s("本章大纲"));
        props.insert(
            "status".into(),
            s("章节状态：empty/draft/written/locked"),
        );
    }
    obj(Value::Object(props), if with_content { &["content"] } else { &[] })
}

pub fn chapter_filter() -> Value {
    obj(
        json!({
            "chapterId": s("只看该章节，留空表示全部")
        }),
        &[],
    )
}

pub fn style_apply_preset() -> Value {
    obj(
        json!({
            "presetId": s("预设 id，先用 style_list_presets 拿")
        }),
        &["presetId"],
    )
}

pub fn style_set() -> Value {
    obj(
        json!({
            "name": s("风格名"),
            "prompt": s("风格提示词，会拼到所有生图提示词前面"),
            "negative": s("负面词"),
            "palette": arr("主色调", json!({ "type": "string" })),
            "lighting": s("光线方案"),
            "lens": s("镜头与焦段倾向"),
            "filmStock": s("质感/胶片感"),
            "aspectRatio": s("画幅，如 9:16"),
            "motionStyle": s("运动风格"),
            "notes": s("备注")
        }),
        &[],
    )
}

pub fn storyboard_write_shots() -> Value {
    obj(
        json!({
            "chapterId": s("所属章节 id"),
            "index": json!({ "type": "integer", "description": "章节序号，与 chapterId 二选一" }),
            "shots": arr(
                "镜头数组",
                obj(
                    json!({
                        "shotSize": s("景别，如 全景/中景/近景/特写"),
                        "camera": s("机位描述"),
                        "cameraMove": s("运镜，如 固定/推近/横移/环绕"),
                        "durationSec": json!({ "type": "number", "description": "时长（秒），建议 2~6" }),
                        "location": s("场景地点"),
                        "timeOfDay": s("时间，如 白天/黄昏/夜晚"),
                        "interior": json!({ "type": "boolean", "description": "是否内景" }),
                        "characters": arr("出场人物名字，用资产里的名字", json!({ "type": "string" })),
                        "props": arr("涉及道具名字", json!({ "type": "string" })),
                        "action": s("画面动作描述"),
                        "dialogue": s("台词，没有就留空"),
                        "narration": s("画外音"),
                        "sfx": s("音效"),
                        "bgm": s("背景音乐提示"),
                        "imagePrompt": s("该镜头的静态画面提示词，可选"),
                        "videoPrompt": s("该镜头的视频提示词草稿，可选")
                    }),
                    &["action"]
                )
            )
        }),
        &["shots"],
    )
}

pub fn storyboard_update_shot() -> Value {
    obj(
        json!({
            "shotId": s("镜头 id"),
            "patch": obj(
                json!({
                    "location": s("场景地点"),
                    "timeOfDay": s("时间"),
                    "shotSize": s("景别"),
                    "camera": s("机位"),
                    "cameraMove": s("运镜"),
                    "action": s("动作描述"),
                    "dialogue": s("台词"),
                    "narration": s("画外音"),
                    "imagePrompt": s("静态画面提示词"),
                    "videoPrompt": s("视频提示词"),
                    "durationSec": json!({ "type": "number" }),
                    "status": s("draft/ready/prompted/generated/locked")
                }),
                &[]
            )
        }),
        &["shotId", "patch"],
    )
}

pub fn asset_list() -> Value {
    obj(
        json!({
            "kind": s("过滤类型：character/scene/prop/costume/vehicle/effect/other，留空为全部")
        }),
        &[],
    )
}

pub fn asset_upsert() -> Value {
    obj(
        json!({
            "id": s("资产 id，留空则按 name 查找或新建"),
            "kind": s("类型：character/scene/prop/costume/vehicle/effect/other"),
            "name": s("资产名，后续分镜与提示词都按这个名字引用"),
            "aliases": arr("别名", json!({ "type": "string" })),
            "description": s("外形/特征描述"),
            "tags": arr("标签", json!({ "type": "string" })),
            "lockedTraits": arr(
                "跨图一致性锁定项，出图时会原样带上，如「左眉有疤」「银色细框眼镜」",
                json!({ "type": "string" })
            )
        }),
        &["name"],
    )
}

pub fn asset_plan_views() -> Value {
    obj(
        json!({
            "assetId": s("资产 id"),
            "views": arr(
                "要生成的视图列表",
                obj(
                    json!({
                        "kind": s("视图类型：front/side/back/threeQuarter/fullBody/closeUp/wide/medium/birdView/custom"),
                        "label": s("显示名，留空按类型自动命名"),
                        "prompt": s("该视图的提示词（风格词会自动拼在前面，这里只写这个视图本身）"),
                        "negative": s("该视图的负面词，可选"),
                        "seed": json!({ "type": "integer", "description": "固定随机种子，可选" })
                    }),
                    &["kind"]
                )
            )
        }),
        &["assetId", "views"],
    )
}

pub fn asset_generate_view() -> Value {
    obj(
        json!({
            "assetId": s("资产 id"),
            "viewId": s("视图 id，留空表示该资产所有待生成的视图"),
            "force": json!({ "type": "boolean", "description": "true 表示已出图的也重出" })
        }),
        &["assetId"],
    )
}

pub fn asset_generate_missing() -> Value {
    obj(
        json!({
            "assetId": s("只处理该资产，留空表示全部资产")
        }),
        &[],
    )
}

pub fn knowledge_read() -> Value {
    obj(json!({ "id": s("知识包 id，先用 knowledge_list 拿") }), &["id"])
}

pub fn ask_user() -> Value {
    obj(
        json!({
            "question": s("要问用户的问题，一次只问一件事"),
            "options": arr(
                "候选项，前端会渲染成按钮；不给就让用户自由作答",
                s("候选项文本")
            ),
            "why": s("为什么需要问（可选，帮用户快速判断）")
        }),
        &["question"],
    )
}

pub fn asset_view_image() -> Value {
    obj(
        json!({
            "assetId": s("资产 id"),
            "viewId": s("只看某个视图，留空则取该资产已生成的前几张")
        }),
        &["assetId"],
    )
}

pub fn file_view_image() -> Value {
    obj(
        json!({
            "path": s("单个图片文件路径"),
            "paths": arr("多个图片文件路径（最多 4 张）", json!({ "type": "string" }))
        }),
        &[],
    )
}

pub fn shot_filter() -> Value {
    obj(
        json!({
            "shotId": s("只看该镜头，留空表示全部")
        }),
        &[],
    )
}

fn asset_ref_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "description": desc,
        "properties": {
            "assetId": s("资产 id"),
            "viewId": s("视图 id，留空表示用该资产已生成的第一张图")
        },
        "required": ["assetId"]
    })
}

pub fn prompt_upsert() -> Value {
    obj(
        json!({
            "shotId": s("镜头 id"),
            "prompt": s("视频提示词，描述运动过程而不是静态画面"),
            "negative": s("负面词"),
            "motion": s("主体动作描述"),
            "cameraMove": s("运镜"),
            "durationSec": json!({ "type": "number", "description": "时长（秒）" }),
            "firstFrame": asset_ref_schema("首帧图"),
            "lastFrame": asset_ref_schema("尾帧图，可选"),
            "refs": arr("其它参考图", asset_ref_schema("参考资产")),
            "modelHint": s("指定模型，可选"),
            "seed": json!({ "type": "integer" })
        }),
        &["shotId", "prompt"],
    )
}

pub fn prompt_bind_assets() -> Value {
    obj(
        json!({
            "shotId": s("镜头 id"),
            "refs": arr("参考资产列表", asset_ref_schema("参考资产")),
            "firstFrame": asset_ref_schema("首帧图，可选")
        }),
        &["shotId", "refs"],
    )
}

pub fn video_generate() -> Value {
    obj(
        json!({
            "shotId": s("镜头 id，与 shotIds 二选一"),
            "shotIds": arr("批量镜头 id", json!({ "type": "string" })),
            "providerId": s("指定 provider，留空用当前启用的"),
            "model": s("指定模型，留空用 provider 默认")
        }),
        &[],
    )
}

pub fn video_generate_batch() -> Value {
    obj(
        json!({
            "chapterId": s("只处理该章节，留空表示全部"),
            "onlyMissing": json!({ "type": "boolean", "description": "只生成还没出片的镜头，默认 true" })
        }),
        &[],
    )
}

pub fn edit_append_clip() -> Value {
    obj(
        json!({
            "trackId": s("轨道 id，默认 tr_video"),
            "takeId": s("生成记录 id，与 source 二选一"),
            "source": s("视频文件绝对路径"),
            "startSec": json!({ "type": "number", "description": "时间线上的起点（秒），留空接在末尾" }),
            "inSec": json!({ "type": "number", "description": "源内入点，默认 0" }),
            "outSec": json!({ "type": "number", "description": "源内出点" }),
            "durationSec": json!({ "type": "number", "description": "时长，若未给 outSec 用它" })
        }),
        &[],
    )
}

pub fn edit_update_clip() -> Value {
    obj(
        json!({
            "clipId": s("片段 id"),
            "startSec": json!({ "type": "number" }),
            "inSec": json!({ "type": "number" }),
            "outSec": json!({ "type": "number" }),
            "speed": json!({ "type": "number" }),
            "volume": json!({ "type": "number" }),
            "enabled": json!({ "type": "boolean" })
        }),
        &["clipId"],
    )
}

pub fn clip_ref() -> Value {
    obj(json!({ "clipId": s("片段 id") }), &["clipId"])
}

pub fn edit_render() -> Value {
    obj(
        json!({
            "burnSubtitles": json!({ "type": "boolean", "description": "是否把字幕烧进画面，默认 true" })
        }),
        &[],
    )
}

pub fn subtitle_transcribe() -> Value {
    obj(
        json!({
            "file": s("要转写的视频/音频路径，留空用时间线第一个片段"),
            "language": s("语言代码，默认 zh；auto 为自动检测"),
            "modelId": s("whisper 模型 id，留空用设置里的"),
            "useGpu": json!({ "type": "boolean", "description": "是否用显卡加速" }),
            "maxLineChars": json!({ "type": "integer", "description": "单条字幕最大字数，默认 18" }),
            "name": s("输出字幕名，可选")
        }),
        &[],
    )
}

pub fn subtitle_update_cues() -> Value {
    obj(
        json!({
            "subtitleId": s("字幕文档 id"),
            "cues": arr(
                "字幕条目",
                obj(
                    json!({
                        "index": json!({ "type": "integer" }),
                        "start": json!({ "type": "number", "description": "开始秒" }),
                        "end": json!({ "type": "number", "description": "结束秒" }),
                        "text": s("文本")
                    }),
                    &["start", "end", "text"]
                )
            )
        }),
        &["subtitleId", "cues"],
    )
}
