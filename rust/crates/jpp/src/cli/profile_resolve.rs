//! 能力画像的路径解析（B73；`21` 步 15d-0）。
//!
//! 路径只在宿主（CLI）这里解析，内核只收 `Profile` 值（`20` §2.2 第 2 条：内核不引用固定文件路径）。
//! 真机运行（`--backend live` 的首跑与续接）必须有画像：按 `--profile <文件>`，否则按
//! `--profiles-dir <目录>/<model>.json`，再否则按可执行文件旁的 `profiles/<model>.json`；
//! 解析不到报 `E-profile-missing` 并写明试过的路径，不回退代码兜底值。
//! 重放不发调用、不初始化真机客户端，不要求画像。固定观察在步 15d 之前可以无画像运行，
//! 提示由 `run_io` 无条件打印。
use crate::options::RunOptions;
use jpp::effects::Profile;
use std::path::{Path, PathBuf};

/// 解析出的画像：从哪个文件读的，读出来的值。
pub struct Resolved {
    pub path: PathBuf,
    pub profile: Profile,
}

/// 这一趟是否用注册后端发调用（首跑或续接）。重放不算：它不初始化后端。步 15g-0 起不限 `live`：
/// 注册表里的后端都按 B73 必带画像。
pub fn uses_live_backend(options: &RunOptions) -> bool {
    options.backend.spec().is_some() && options.replay.is_none()
}

/// 可执行文件旁的画像目录（发行物把 `profiles/` 放在 `jpp` 旁边）。
fn beside_executable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("profiles"))
}

fn load(path: &Path) -> Result<Resolved, String> {
    // 字节装载在 `jpp::store::ProfileLoader`（步 18-0）；路径与读文件留在 CLI（B73）。报文与 `Profile::load` 相同
    let bytes = std::fs::read(path)
        .map_err(|e| format!("{}: 读不到档案 {}：{e}", path.display(), path.display()))?;
    let profile =
        jpp::store::ProfileLoader::load(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Resolved {
        path: path.to_path_buf(),
        profile,
    })
}

/// 按 B73 解析这一趟的画像。`Ok(None)` 只出现在不用真机后端且没给 `--profile` 的时候。
pub fn resolve(options: &RunOptions) -> Result<Option<Resolved>, String> {
    if let Some(p) = &options.profile {
        return load(p).map(Some);
    }
    let Some(spec) = options
        .backend
        .spec()
        .filter(|_| uses_live_backend(options))
    else {
        return Ok(None);
    };
    let model = options.model.as_deref().unwrap_or(spec.default_model);
    let file = format!("{model}.json");
    let dir = match &options.profiles_dir {
        Some(d) => Some(d.clone()),
        None => beside_executable(),
    };
    let tried: Vec<PathBuf> = dir.into_iter().map(|d| d.join(&file)).collect();
    if let Some(found) = tried.iter().find(|p| p.is_file()) {
        return load(found).map(Some);
    }
    let tried_text = if tried.is_empty() {
        "（取不到可执行文件所在目录）".to_string()
    } else {
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("、")
    };
    Err(format!(
        // 依据：B73（地基/附注/2026-09-24-评估①裁定.md §二；21 步 15d-0）
        "E-profile-missing: --backend {} 需要模型 {model} 的能力画像（B73），不回退代码兜底值；试过：{tried_text}。\
         修法：--profile <画像文件>，或 --profiles-dir <目录>（目录里放 {file}；发行物附带 profiles/）",
        spec.name
    ))
}
