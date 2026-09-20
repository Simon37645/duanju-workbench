// Windows 下发布版不弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 命令行自检：不启动界面
    //   cargo run -- --pipeline-check   整条生产链跑一遍
    //   cargo run -- --net-check        代理与模型仓库连通性
    //   cargo run -- --pi-check [项目]   pi 引擎真实对话链路（会消耗少量额度）
    //   cargo run -- --asr-download ggml-tiny   预下载 whisper 模型
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--pipeline-check") {
        let code =
            tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_pipeline_check());
        std::process::exit(code);
    }
    if let Some(i) = args.iter().position(|a| a == "--pi-check") {
        let path = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .cloned();
        let code =
            tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_pi_check(path));
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--net-check") {
        let code = tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_net_check());
        std::process::exit(code);
    }
    if let Some(i) = args.iter().position(|a| a == "--import-skill") {
        let paths: Vec<String> = args[i + 1..].to_vec();
        if paths.is_empty() {
            eprintln!("用法：cargo run -- --import-skill <zip 或 目录 或 md>…");
            std::process::exit(2);
        }
        let code =
            tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_import_skills(&paths));
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--skills") {
        let code = tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_list_skills());
        std::process::exit(code);
    }
    if let Some(i) = args.iter().position(|a| a == "--asr-download") {
        let model = args.get(i + 1).cloned().unwrap_or_else(|| "ggml-tiny".into());
        let code =
            tauri::async_runtime::block_on(duanju_workbench_lib::selftest::run_asr_download(&model));
        std::process::exit(code);
    }

    duanju_workbench_lib::run()
}
