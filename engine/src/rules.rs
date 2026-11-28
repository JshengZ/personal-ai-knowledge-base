use crate::types::*;
use anyhow::{bail, Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::{collections::HashMap, io::Read, path::{Path, PathBuf}, sync::Mutex};

pub fn normalized(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    let value = if let Some(unc)=value.strip_prefix("//?/UNC/") { format!("//{unc}") } else { value.strip_prefix("//?/").unwrap_or(&value).to_string() };
    let prefix=if value.starts_with("//") { "//" } else if value.starts_with('/') { "/" } else { "" };
    let value=format!("{prefix}{}",value.split('/').filter(|part|!part.is_empty() && *part!=".").collect::<Vec<_>>().join("/"));
    if cfg!(windows) { value.to_lowercase() } else { value }
}
pub fn contains(parent: &Path, child: &Path) -> bool {
    let parent = normalized(parent); let child = normalized(child);
    parent == child || child.starts_with(&(parent + "/"))
}
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let base = normalized(root); let full = normalized(path);
    if full == base { Some(".".into()) } else { full.strip_prefix(&(base + "/")).map(str::to_string) }
}
pub fn validate_paths(paths: &[String]) -> Result<()> {
    if paths.len() > 200 { bail!("单个集合最多保存 200 条路径规则"); }
    for value in paths {
        let text = value.replace('\\', "/");
        if text.trim().is_empty() || text.len() > 2048 || text.starts_with('/') || text.contains(':')
            || text.split('/').any(|part| part == "..") {
            bail!("路径规则需为集合内的相对路径，不能包含上级目录");
        }
    }
    Ok(())
}
pub fn validate(root: &Root) -> Result<()> {
    if root.exclusions.len() > 200 { bail!("排除规则最多 200 条"); }
    for pattern in &root.exclusions {
        if pattern.len() > 2048 { bail!("排除规则过长"); }
        GlobBuilder::new(pattern).case_insensitive(cfg!(windows)).build().context("排除规则格式无效")?;
    }
    validate_paths(&root.filters.excluded_paths)?;
    validate_paths(&root.filters.included_paths)
}
fn covers(paths: &[String], relative: &str) -> bool {
    paths.iter().any(|path| contains(Path::new(path), Path::new(relative)) || path == ".")
}
fn automatic_name(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    if [".build", ".dist", ".onefile-build"].iter().any(|suffix| name.ends_with(suffix)) {
        Some("编译产物目录（含 Nuitka）")
    } else if ["tmp","temp",".tmp",".temp",".zcode",".codex"].contains(&name.as_str()) || ["temp-","temp_","tmp-","tmp_"].iter().any(|prefix|name.starts_with(prefix)) || ["temp","tmp"].iter().any(|prefix|name.strip_prefix(prefix).is_some_and(|tail|!tail.is_empty()&&tail.chars().all(|character|character.is_ascii_digit()))) {Some("临时文件或 Agent 缓存目录")}
    else if name.ends_with(".app") { Some("应用程序包") }
    else if name.ends_with(".dist-info") || name.ends_with(".egg-info") { Some("Python 依赖元数据") }
    else if ["node_modules", ".venv", "venv", "target", ".next", ".nuxt", ".cache", ".local",
        "__pycache__", ".npm", ".pnpm-store", ".cargo", ".rustup", ".gradle", ".m2", ".terraform",
        "dist", "build", "coverage", "obj", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".tox", ".nox"].contains(&name.as_str()) {
        Some("依赖、缓存或构建输出目录")
    } else { None }
}
fn protected_name(name: &str) -> bool {
    [".git", "$recycle.bin", "system volume information", "appdata"].contains(&name.to_ascii_lowercase().as_str())
}
pub fn kind(path: &Path) -> &'static str {
    match path.extension().and_then(|v|v.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "png"|"jpg"|"jpeg"|"tif"|"tiff" => "images",
        "md"|"mdx"|"txt"|"rst"|"log"|"pdf"|"docx"|"pptx"|"xlsx"|"csv"|"tsv" => "documents",
        _ => "code",
    }
}

pub struct Rules<'a> {
    root: &'a Root,
    home: &'a Path,
    resources: &'a Path,
    excludes: GlobSet,
    markers: Mutex<HashMap<PathBuf, Option<String>>>,
    links: Mutex<HashMap<PathBuf, bool>>,
}
impl<'a> Rules<'a> {
    pub fn new(root: &'a Root, home: &'a Path, resources: &'a Path) -> Result<Self> {
        validate(root)?;
        let mut builder = GlobSetBuilder::new();
        for pattern in &root.exclusions {
            builder.add(GlobBuilder::new(pattern).case_insensitive(cfg!(windows)).build()?);
        }
        Ok(Self { root, home, resources, excludes: builder.build()?, markers: Mutex::new(HashMap::new()),links:Mutex::new(HashMap::new()) })
    }
    fn marker(&self, directory: &Path) -> Option<String> {
        let mut markers = self.markers.lock().unwrap();
        markers.entry(directory.to_path_buf()).or_insert_with(|| {
            // Two independent signatures avoid classifying arbitrary output folders.
            if directory.join("__constants.c").is_file()
                && (directory.join("__helpers.c").is_file() || directory.join("build_definitions.h").is_file()) {
                Some("检测到 Nuitka 生成文件".into())
            } else if directory.join("_internal").is_dir() && std::fs::read_dir(directory).ok().is_some_and(|items|
                items.take(256).flatten().any(|item| item.path().extension().is_some_and(|extension| extension.eq_ignore_ascii_case("exe")))) {
                Some("检测到应用打包目录".into())
            } else { None }
        }).clone()
    }
    pub fn skip(&self, path: &Path, directory: bool) -> Option<String> {
        self.check(path,directory,false)
    }
    // Listing metadata must not reopen every indexed file. Source reads and
    // scans still perform the full content and canonical-path checks.
    pub fn skip_indexed(&self,path:&Path)->Option<String> {self.check(path,false,true)}
    fn check(&self,path:&Path,directory:bool,indexed:bool)->Option<String> {
        if contains(self.home, path) || (!self.resources.as_os_str().is_empty() && contains(self.resources, path)) {
            return Some("知识库自身的数据或运行资源".into());
        }
        if !indexed && std::fs::symlink_metadata(path).is_ok_and(|value|value.file_type().is_symlink()) {
            return Some("符号链接或目录联接".into());
        }
        let Some(relative) = relative(Path::new(&self.root.path), path) else { return Some("资料已离开集合目录".into()); };
        if covers(&self.root.filters.excluded_paths, &relative)
            || self.excludes.is_match(&relative)
            || directory && self.excludes.is_match(format!("{relative}/")) {
            return Some("自定义排除规则".into());
        }
        let included = covers(&self.root.filters.included_paths, &relative);
        // An explicitly included descendant needs its ancestor directories walked.
        let walk_to_included = directory && self.root.filters.included_paths.iter().any(|item|
            contains(Path::new(&relative), Path::new(item)) || relative == ".");
        let root = Path::new(&self.root.path);
        let mut current = if directory { Some(path) } else { path.parent() };
        while let Some(parent) = current {
            let linked=if indexed {*self.links.lock().unwrap().entry(parent.into()).or_insert_with(||std::fs::symlink_metadata(parent).is_ok_and(|value|value.file_type().is_symlink()))}
                else {std::fs::symlink_metadata(parent).is_ok_and(|value|value.file_type().is_symlink())};
            if linked {
                return Some("符号链接或目录联接".into());
            }
            if let Some(parent_relative) = self::relative(root, parent) {
                if self.excludes.is_match(&parent_relative) || self.excludes.is_match(format!("{parent_relative}/")) {
                    return Some("自定义排除规则".into());
                }
            }
            let name = parent.file_name().and_then(|name|name.to_str()).unwrap_or_default();
            if protected_name(name) { return Some("系统或 Git 内部目录".into()); }
            if self.root.filters.automatic && !included && !walk_to_included {
                if let Some(reason) = automatic_name(name) { return Some(reason.into()); }
                if let Some(reason) = self.marker(parent) { return Some(reason); }
            }
            if normalized(parent) == normalized(root) { break; }
            if !contains(root, parent) { return Some("资料已离开集合目录".into()); }
            current = parent.parent();
        }
        if directory { return None; }
        if !crate::indexer::supported(path) { return Some("不支持的文件类型（安装包、压缩包或媒体等）".into()); }
        let enabled = match kind(path) { "images"=>self.root.filters.images, "documents"=>self.root.filters.documents, _=>self.root.filters.code };
        if !enabled { return Some("此文件类型已关闭索引".into()); }
        if !indexed && !["pdf","docx","pptx","xlsx","png","jpg","jpeg","tif","tiff"].contains(
            &path.extension().and_then(|value|value.to_str()).unwrap_or_default().to_ascii_lowercase().as_str()) {
            if let Ok(mut file) = std::fs::File::open(path) {
                let mut bytes = [0u8;1024];
                if let Ok(count) = file.read(&mut bytes) {
                    let sample = &bytes[..count];
                    if sample.starts_with(b"\x7fELF") || sample.contains(&0) && !sample.starts_with(&[0xff,0xfe]) && !sample.starts_with(&[0xfe,0xff]) {
                        return Some("文件内容为二进制，无法按文本索引".into());
                    }
                }
            }
        }
        None
    }
}

#[derive(Default, Serialize)]
pub struct ScanSummary {
    pub eligible_files: u64,
    pub skipped_files: u64,
    pub skipped_directories: u64,
    pub reasons: HashMap<String,u64>,
    pub examples: Vec<serde_json::Value>,
    pub scanned_at: String,
}
impl ScanSummary {
    pub fn record(&mut self, root: &Root, path: &Path, directory: bool, reason: String) {
        if directory { self.skipped_directories += 1; } else { self.skipped_files += 1; }
        *self.reasons.entry(reason.clone()).or_default() += 1;
        if self.examples.len() < 20 {
            self.examples.push(serde_json::json!({"path":relative(Path::new(&root.path),path).unwrap_or_default(),"directory":directory,"reason":reason}));
        }
    }
}
