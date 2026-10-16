//! Elytra Ajan Araçları (Tools) — Glitch Code ve Claude Code seviyesinde yerel araçlar
//!
//! Bellek tüketimi ve CPU yükü sıfıra yakın olacak şekilde optimize edilmiştir.
//! Arka planda kilitlenme veya donma olmaması için her işlemde zaman aşımı ve
//! çıktı limiti bulunur.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Model için Araç Tanımı (OpenAI Function Calling formatı)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Tüm araçların listesi (Model API'sine verilir)
pub fn get_available_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".into(),
            description: "Bir dosyanin icerigini satir numaralariyla okur. Satir araligi verilebilir.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Okunacak dosyanin goreceli veya mutlak yolu" },
                    "start_line": { "type": "integer", "description": "Opsiyonel baslangic satiri (1-tabanli)" },
                    "end_line": { "type": "integer", "description": "Opsiyonel bitis satiri (1-tabanli)" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".into(),
            description: "Yeni bir dosya olusturur veya var olan dosyayi gunceller. Ust dizinleri otomatik acar.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Yazilacak dosya yolu" },
                    "content": { "type": "string", "description": "Dosyaya yazilacak tam metin" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "edit_file".into(),
            description: "Bir dosyadaki belirli bir hedef metin blogunu yeni metinle degistirir.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Duzenlenecek dosya yolu" },
                    "target": { "type": "string", "description": "Degistirilecek mevcut blok (birebir eslesmeli)" },
                    "replacement": { "type": "string", "description": "Yerine konulacak yeni metin" }
                },
                "required": ["path", "target", "replacement"]
            }),
        },
        ToolDefinition {
            name: "list_dir".into(),
            description: "Belirtilen dizindeki dosya ve klasorleri listeler. .git, target, node_modules gibi agir klasorleri atlar.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Listelenecek dizin yolu (varsayilan: .)" },
                    "max_depth": { "type": "integer", "description": "Maksimum alt klasor derinligi (varsayilan: 2)" }
                }
            }),
        },
        ToolDefinition {
            name: "grep_search".into(),
            description: "Belirli bir dizin altindaki dosyalarda regex veya metin aramasi yapar. Binary ve 2MB uzeri dosyalari otomatik atlar, glob filtresi ve baglam satirlari destekler.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Aranacak dizin veya dosya" },
                    "query": { "type": "string", "description": "Aranacak metin veya regex" },
                    "case_insensitive": { "type": "boolean", "description": "Buyuk/kucuk harf duyarsiz arama" },
                    "context_lines": { "type": "integer", "description": "Eslesme basina gosterilecek cevresel satir sayisi (0-5)" },
                    "include_glob": { "type": "string", "description": "Sadece bu desene uyan dosyalari tara (orn: '*.rs', '*.toml')" }
                },
                "required": ["path", "query"]
            }),
        },
        ToolDefinition {
            name: "file_search".into(),
            description: "Dosya adina veya uzantisina gore hizli dosya aramasi yapar (orn: main.rs, *.toml).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Aranacak dosya adi veya uzantisi" },
                    "dir": { "type": "string", "description": "Opsiyonel baslangic dizini (varsayilan: .)" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "git_status".into(),
            description: "Git deposundaki aktif dali, degisen, eklenen veya silinen dosyalari listeler.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "Opsiyonel calisma dizini" }
                }
            }),
        },
        ToolDefinition {
            name: "git_diff".into(),
            description: "Dosyalardaki git degisikliklerini (diff) inceler.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Opsiyonel dosya yolu (bos ise tum repo diff)" }
                }
            }),
        },
        ToolDefinition {
            name: "diagnostics".into(),
            description: "Projede derleme veya linter hatalarini kontrol eder (cargo check, npm lint vb.).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "Opsiyonel calisma dizini" }
                }
            }),
        },
        ToolDefinition {
            name: "bash_exec".into(),
            description: "Sistem terminal komutu calistirir (cargo test, git, npm vb.). 30s zaman asimi korumalidir.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Calistirilacak terminal komutu" },
                    "cwd": { "type": "string", "description": "Opsiyonel calisma dizini" }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "web_fetch".into(),
            description: "Bir web URL'sinin icerigini ceker (HTTP GET).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Icerigi cekilecek gecerli URL" }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "web_search".into(),
            description: "Canli web aramasi yapar (DuckDuckGo motoru). Basliklar, linkler ve ozetler doner.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Aranacak kelime veya soru" },
                    "max_results": { "type": "integer", "description": "Maksimum sonuc sayisi (varsayilan: 5)" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "apply_patch".into(),
            description: "Bir dosyaya unified diff veya degisiklik yamasi (patch) uygular.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Yamalanacak dosya yolu" },
                    "patch": { "type": "string", "description": "Uygulanacak diff veya yama icerigi" }
                },
                "required": ["path", "patch"]
            }),
        },
        ToolDefinition {
            name: "task_tracker".into(),
            description: "Cok adimli gorev ve checklist yonetimi (add, list, complete, clear).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "description": "Islem: 'add', 'list', 'complete', 'clear'" },
                    "task": { "type": "string", "description": "Gorev metni ('add' icin zorunlu)" },
                    "id": { "type": "integer", "description": "Gorev ID'si ('complete' icin zorunlu)" }
                },
                "required": ["action"]
            }),
        },
        ToolDefinition {
            name: "repo_map".into(),
            description: "Akilli kod tabani mimarisi, proje tipi, temel dosyalar ve dil istatistiklerini cikarir.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "dir": { "type": "string", "description": "Incelenecek dizin (varsayilan: .)" }
                }
            }),
        },
    ]
}

/// Gelen araç çağrısını çalıştır ve metin çıktısını dön
pub async fn execute_tool(name: &str, args: &Value) -> Result<String> {
    match name {
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'path' parametresi eksik"))?;
            let start = args.get("start_line").and_then(|v| v.as_u64()).map(|v| v as usize);
            let end = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize);
            read_file(path, start, end)
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'path' parametresi eksik"))?;
            let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'content' parametresi eksik"))?;
            write_file(path, content)
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'path' parametresi eksik"))?;
            let target = args.get("target").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'target' parametresi eksik"))?;
            let replacement = args.get("replacement").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'replacement' parametresi eksik"))?;
            edit_file(path, target, replacement)
        }
        "apply_patch" => {
            let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'path' parametresi eksik"))?;
            let patch = args.get("patch").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'patch' parametresi eksik"))?;
            apply_patch(path, patch)
        }
        "list_dir" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            list_dir(path, max_depth)
        }
        "grep_search" => {
            let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'path' parametresi eksik"))?;
            let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'query' parametresi eksik"))?;
            let case_insensitive = args.get("case_insensitive").and_then(|v| v.as_bool()).unwrap_or(false);
            let context_lines = args.get("context_lines").and_then(|v| v.as_u64()).unwrap_or(0).min(5) as usize;
            let include_glob = args.get("include_glob").and_then(|v| v.as_str());
            grep_search(path, query, case_insensitive, context_lines, include_glob)
        }
        "file_search" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'pattern' parametresi eksik"))?;
            let dir = args.get("dir").and_then(|v| v.as_str());
            file_search(pattern, dir)
        }
        "git_status" => {
            let cwd = args.get("cwd").and_then(|v| v.as_str());
            git_status(cwd)
        }
        "git_diff" => {
            let path = args.get("path").and_then(|v| v.as_str());
            let cwd = args.get("cwd").and_then(|v| v.as_str());
            git_diff(path, cwd)
        }
        "diagnostics" => {
            let cwd = args.get("cwd").and_then(|v| v.as_str());
            diagnostics(cwd).await
        }
        "bash_exec" => {
            let command = args.get("command").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'command' parametresi eksik"))?;
            let cwd = args.get("cwd").and_then(|v| v.as_str());
            bash_exec(command, cwd).await
        }
        "web_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'url' parametresi eksik"))?;
            web_fetch(url).await
        }
        "web_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'query' parametresi eksik"))?;
            let max_res = args.get("max_results").and_then(|v| v.as_u64()).map(|v| v as usize);
            web_search(query, max_res).await
        }
        "task_tracker" => {
            let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("'action' parametresi eksik"))?;
            let task = args.get("task").and_then(|v| v.as_str());
            let id = args.get("id").and_then(|v| v.as_u64()).map(|v| v as usize);
            task_tracker(action, task, id)
        }
        "repo_map" => {
            let dir = args.get("dir").and_then(|v| v.as_str());
            repo_map(dir)
        }
        _ => Err(anyhow!("Bilinmeyen arac: '{}'", name)),
    }
}

// ─────────────────────── Araç Gerçeklemeleri ───────────────────────

/// Dosya oku (satır aralığı destekli, binary/büyük dosya korumalı)
pub fn read_file(path: &str, start_line: Option<usize>, end_line: Option<usize>) -> Result<String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(anyhow!("Dosya bulunamadi: {}", path));
    }
    let content = crate::util::read_text_file_safe(p, 5_000_000)
        .map_err(|e| anyhow!("Dosya okunamadi ({path}): {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(total_lines).min(total_lines);

    if start > total_lines {
        return Ok(format!("Dosya {total_lines} satir, baslangic satiri {start} dosya boyutunu asiyor."));
    }

    let mut out = String::new();
    out.push_str(&format!("[{path}] (Satir {start}-{end} / Toplam {total_lines} satir):\n"));

    for idx in start..=end {
        let line_content = lines.get(idx - 1).unwrap_or(&"");
        out.push_str(&format!("{:4}: {}\n", idx, line_content));
    }

    Ok(out)
}

/// Dosya yaz (üst dizinleri otomatik oluşturur)
pub fn write_file(path: &str, content: &str) -> Result<String> {
    let p = Path::new(path);
    if let Some(parent) = p.parent()
        && !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    fs::write(p, content)?;
    Ok(format!("Basariyla yazildi: {} ({} bayt)", path, content.len()))
}

/// Dosyada hedef bloğu değiştir
pub fn edit_file(path: &str, target: &str, replacement: &str) -> Result<String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(anyhow!("Dosya bulunamadi: {}", path));
    }
    let content = fs::read_to_string(p)?;

    if !content.contains(target) {
        return Err(anyhow!(
            "Hedef metin dosyada bulunamadi. Lutfen tam eslestiginden emin olun:\n{}",
            target
        ));
    }

    let count = content.matches(target).count();
    if count > 1 {
        return Err(anyhow!(
            "Hedef metin dosyada {} kez geciyor. Lutfen daha belirgin bir baglam blogu secin.",
            count
        ));
    }

    let new_content = content.replace(target, replacement);
    fs::write(p, new_content)?;
    Ok(format!("Dosya basariyla guncellendi: {}", path))
}

/// Dizin listeleme (ağır klasörleri filtreler)
pub fn list_dir(path: &str, max_depth: usize) -> Result<String> {
    let root = Path::new(path);
    if !root.exists() {
        return Err(anyhow!("Dizin bulunamadi: {}", path));
    }

    let mut entries = Vec::new();
    const MAX_ENTRIES: usize = 500;
    for entry in walkdir::WalkDir::new(root)
        .max_depth(max_depth)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !is_scan_ignored(std::ffi::OsStr::new(name.as_ref()))
        })
        .filter_map(|e| e.ok())
    {
        if entries.len() >= MAX_ENTRIES {
            entries.push("... (Maksimum 500 girdi gosterildi)".into());
            break;
        }
        let rel_path = entry.path().strip_prefix(root).unwrap_or(entry.path());
        if rel_path.as_os_str().is_empty() {
            continue;
        }
        let kind = if entry.file_type().is_dir() { "DIR " } else { "FILE" };
        let size_str = if entry.file_type().is_file() {
            format!("({} B)", entry.metadata().map(|m| m.len()).unwrap_or(0))
        } else {
            String::new()
        };
        entries.push(format!("{} {:<35} {}", kind, rel_path.display(), size_str));
    }

    if entries.is_empty() {
        Ok(format!("Dizin bos veya tum icerikler filtrelendi: {}", path))
    } else {
        Ok(entries.join("\n"))
    }
}

/// Regex / Metin aramasi (gelistirilmis: binary korumali, glob filtresi, baglam satirlari)
pub fn grep_search(
    path: &str,
    query: &str,
    case_insensitive: bool,
    context_lines: usize,
    include_glob: Option<&str>,
) -> Result<String> {
    let root = Path::new(path);
    let mut pattern_builder = regex::RegexBuilder::new(query);
    pattern_builder.case_insensitive(case_insensitive);
    let re = pattern_builder.build()
        .map_err(|e| anyhow!("Gecersiz arama deseni/regex: {e}"))?;

    let glob = include_glob.map(compile_glob);

    let max_matches = 100;
    let max_file_bytes: u64 = 2_000_000; // 2 MB uzerini tarama
    let mut matches: Vec<String> = Vec::new();
    let mut files_scanned = 0usize;
    let mut files_skipped = 0usize;

    'outer: for entry in walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !is_scan_ignored(e.file_name()))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let entry_path = entry.path();
        if let Some(g) = &glob {
            let fname = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !glob_match(g, &fname) {
                continue;
            }
        }
        // Binary ve devasa dosyalari atla (ajan korumasi)
        let content = match crate::util::read_text_file_safe(entry_path, max_file_bytes) {
            Ok(c) => c,
            Err(_) => {
                files_skipped += 1;
                continue;
            }
        };
        files_scanned += 1;

        let lines: Vec<&str> = content.lines().collect();
        for (line_idx, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                if context_lines > 0 {
                    let start = line_idx.saturating_sub(context_lines);
                    let end = (line_idx + context_lines).min(lines.len().saturating_sub(1));
                    for (offset, ctx_line) in lines[start..=end].iter().enumerate() {
                        let ctx = start + offset;
                        let marker = if ctx == line_idx { ">" } else { " " };
                        matches.push(format!(
                            "{}{}:{}: {}",
                            marker,
                            entry_path.display(),
                            ctx + 1,
                            ctx_line.trim_end()
                        ));
                    }
                    matches.push("-".repeat(40));
                } else {
                    matches.push(format!(
                        "{}:{}: {}",
                        entry_path.display(),
                        line_idx + 1,
                        line.trim()
                    ));
                }
                if matches.len() >= max_matches {
                    matches.push(format!("... (Maksimum {max_matches} eslesmeye ulasildi)"));
                    break 'outer;
                }
            }
        }
    }

    let skipped_note = if files_skipped > 0 {
        format!(" ({files_skipped} binary/buyuk dosya atlandi)")
    } else {
        String::new()
    };

    if matches.is_empty() {
        Ok(format!(
            "'{}' deseni icin eslesme bulunamadi ({} dosya tarandi){}.",
            query, files_scanned, skipped_note
        ))
    } else {
        Ok(format!(
            "◆ {} eslesme ({} dosya tarandi){}:\n{}",
            matches.len(),
            files_scanned,
            skipped_note,
            matches.join("\n")
        ))
    }
}

/// Dizin gezinmesinde atlanacak agir/gurultulu klasorler
pub fn is_scan_ignored(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    matches!(
        name.as_ref(),
        ".git" | "target" | "node_modules" | "dist" | ".turbo" | ".husky"
            | ".venv" | "venv" | "__pycache__" | ".next" | ".nuxt" | "out"
    )
}

/// Basit glob token'lari (* ve ? destekler)
#[derive(Debug, Clone)]
pub enum GlobToken {
    Literal(String),
    Star,
    Question,
}

/// Basit glob deseni derleyici
pub fn compile_glob(pattern: &str) -> Vec<GlobToken> {
    let mut tokens = Vec::new();
    let mut literal = String::new();
    for c in pattern.chars() {
        match c {
            '*' => {
                if !literal.is_empty() {
                    tokens.push(GlobToken::Literal(std::mem::take(&mut literal)));
                }
                tokens.push(GlobToken::Star);
            }
            '?' => {
                if !literal.is_empty() {
                    tokens.push(GlobToken::Literal(std::mem::take(&mut literal)));
                }
                tokens.push(GlobToken::Question);
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        tokens.push(GlobToken::Literal(literal));
    }
    tokens
}

/// Glob eslestirme (token listesi ile, dosya adi uzerinde, kucuk harfe duyarsiz)
pub fn glob_match(tokens: &[GlobToken], name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let chars: Vec<char> = name_lower.chars().collect();

    fn helper(tokens: &[GlobToken], chars: &[char]) -> bool {
        match tokens.first() {
            None => chars.is_empty(),
            Some(GlobToken::Star) => {
                // * sifir veya daha fazla karakter tuketebilir
                for skip in 0..=chars.len() {
                    if helper(&tokens[1..], &chars[skip..]) {
                        return true;
                    }
                }
                false
            }
            Some(GlobToken::Question) => {
                if chars.is_empty() {
                    false
                } else {
                    helper(&tokens[1..], &chars[1..])
                }
            }
            Some(GlobToken::Literal(lit)) => {
                let lit_lower = lit.to_lowercase();
                let lit_chars: Vec<char> = lit_lower.chars().collect();
                if chars.len() < lit_chars.len() {
                    return false;
                }
                if chars[..lit_chars.len()] == lit_chars[..] {
                    helper(&tokens[1..], &chars[lit_chars.len()..])
                } else {
                    false
                }
            }
        }
    }

    helper(tokens, &chars)
}

/// Dosya adı araması (Glob destekli: *.rs, main.rs, Cargo.*)
pub fn file_search(pattern: &str, dir: Option<&str>) -> Result<String> {
    let root = Path::new(dir.unwrap_or("."));
    if !root.exists() {
        return Err(anyhow!("Dizin bulunamadi: {:?}", root));
    }
    let pat_lower = pattern.to_lowercase();
    let is_glob = pattern.contains('*') || pattern.contains('?');
    let glob_tokens = if is_glob {
        Some(compile_glob(pattern))
    } else {
        None
    };

    let mut matches: Vec<String> = Vec::new();
    const MAX_RESULTS: usize = 60;

    for entry in walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !is_scan_ignored(e.file_name()))
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let hit = if let Some(tokens) = &glob_tokens {
            glob_match(tokens, &name)
        } else {
            name.contains(&pat_lower)
        };
        if hit {
            let size = entry
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            matches.push(format!("  {} ({} B)", entry.path().display(), size));
            if matches.len() >= MAX_RESULTS {
                matches.push(format!("  ... (Maksimum {MAX_RESULTS} dosya gosterildi)"));
                break;
            }
        }
    }

    if matches.is_empty() {
        Ok(format!("'{}' deseniyle eslesen dosya bulunamadi.", pattern))
    } else {
        Ok(format!(
            "◆ '{}' icin {} dosya bulundu:\n{}",
            pattern,
            matches.len(),
            matches.join("\n")
        ))
    }
}

/// Git durumu
pub fn git_status(cwd: Option<&str>) -> Result<String> {
    let current_dir = cwd.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut cmd = Command::new("git");
    cmd.args(["status", "--short", "-b"]).current_dir(current_dir);
    let out = cmd.output().map_err(|e| anyhow!("Git calistirilamadi: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        Ok("Git durumu temiz (Degisiklik yok)".into())
    } else {
        Ok(text)
    }
}

/// Git diff
pub fn git_diff(path: Option<&str>, cwd: Option<&str>) -> Result<String> {
    let current_dir = cwd.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut cmd = Command::new("git");
    cmd.arg("diff");
    if let Some(p) = path {
        cmd.arg(p);
    }
    cmd.current_dir(current_dir);
    let out = cmd.output().map_err(|e| anyhow!("Git diff calistirilamadi: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        Ok("Herhangi bir git degisikligi (diff) yok".into())
    } else {
        Ok(crate::util::truncate_chars_with_note(&text, 10_000, "\n... (Diff 10.000 karaktere kirpildi)"))
    }
}

/// Proje derleyici / linter kontrolleri (Diagnostics)
pub async fn diagnostics(cwd: Option<&str>) -> Result<String> {
    let current_dir = cwd.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    if current_dir.join("Cargo.toml").exists() {
        // cargo check ilk çalışmada uzun sürebilir — 120s koruma
        return bash_exec_with_timeout("cargo check --message-format=short", cwd, 120).await;
    }
    if current_dir.join("package.json").exists() {
        return bash_exec_with_timeout("npm run lint", cwd, 120).await;
    }
    Ok("Desteklenen proje tipi bulunamadi (Cargo.toml veya package.json yok)".into())
}

/// Güvenli terminal komutu çalıştırma (30s SERT timeout — child process öldürülür)
pub async fn bash_exec(cmd_str: &str, cwd: Option<&str>) -> Result<String> {
    bash_exec_with_timeout(cmd_str, cwd, 30).await
}

/// Zaman aşımı süresi ayarlanabilir terminal komutu çalıştırma
pub async fn bash_exec_with_timeout(cmd_str: &str, cwd: Option<&str>, timeout_secs: u64) -> Result<String> {
    let current_dir = cwd.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let cmd = cmd_str.to_string();

    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::process::Stdio;

        #[cfg(target_os = "windows")]
        let mut command = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", &cmd]);
            c
        };

        #[cfg(not(target_os = "windows"))]
        let mut command = {
            let mut c = Command::new("sh");
            c.args(["-c", &cmd]);
            c
        };

        command.current_dir(current_dir);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| anyhow!("Komut yurutme hatasi: {e}"))?;

        // Sert zaman aşımı: bekleme döngüsü + child kill
        let timeout = Duration::from_secs(timeout_secs.max(1));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        anyhow::bail!(
                            "Komut zaman asimina ugradi ({timeout_secs}s) ve sonlandirildi: {cmd}"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("Komut bekleme hatasi: {e}");
                }
            }
        };

        let output = child
            .wait_with_output()
            .map_err(|e| anyhow!("Komut ciktisi okunamadi: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = status.code().unwrap_or(-1);

        let mut res = String::new();
        if code != 0 {
            res.push_str(&format!("[Cikis Kodu: {code}]\n"));
        }
        if !stdout.is_empty() {
            res.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push('\n');
            }
            res.push_str(&format!("[STDERR]\n{}", stderr));
        }
        if res.is_empty() {
            res = "(Komut cikti uretmeden basariyla tamamlandi)".into();
        }
        Ok(res)
    })
    .await?
}

/// Web içeriği çekme
pub async fn web_fetch(url: &str) -> Result<String> {
    let client = shared_http_client(Duration::from_secs(12));

    let res = client.get(url).send().await
        .map_err(|e| anyhow!("URL istegi basarisiz ({url}): {e}"))?;

    let text = res.text().await
        .map_err(|e| anyhow!("Icerik okunamadi: {e}"))?;

    if text.len() > 10_000 {
        Ok(crate::util::truncate_chars_with_note(&text, 10_000, "\n... (Icerik 10.000 karaktere kirpildi)"))
    } else {
        Ok(text)
    }
}

/// Dosyaya patch / diff uygula (satır sonu korumalı)
pub fn apply_patch(path: &str, patch: &str) -> Result<String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(anyhow!("Dosya bulunamadi: {}", path));
    }
    let content = fs::read_to_string(p)?;
    // Orijinal satır sonu karakterini koru (CRLF dosyalarda bozulma olmasın)
    let uses_crlf = content.contains("\r\n");
    let trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    let mut modified = false;
    let mut target_chunk = Vec::new();
    let mut replacement_chunk = Vec::new();
    let mut in_hunk = false;

    for line in patch.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
            if !target_chunk.is_empty() {
                let target_str = target_chunk.join("\n");
                let repl_str = replacement_chunk.join("\n");
                let cur = lines.join("\n");
                if cur.contains(&target_str) {
                    let updated = cur.replacen(&target_str, &repl_str, 1);
                    lines = updated.lines().map(|s| s.to_string()).collect();
                    modified = true;
                }
                target_chunk.clear();
                replacement_chunk.clear();
            }
            continue;
        }

        if in_hunk {
            if let Some(rest) = line.strip_prefix('-') {
                target_chunk.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix('+') {
                replacement_chunk.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix(' ') {
                target_chunk.push(rest.to_string());
                replacement_chunk.push(rest.to_string());
            }
        }
    }

    if !target_chunk.is_empty() {
        let target_str = target_chunk.join("\n");
        let repl_str = replacement_chunk.join("\n");
        let cur = lines.join("\n");
        if cur.contains(&target_str) {
            let updated = cur.replacen(&target_str, &repl_str, 1);
            lines = updated.lines().map(|s| s.to_string()).collect();
            modified = true;
        }
    }

    if modified {
        let mut out = lines.join("\n");
        if uses_crlf {
            out = out.replace('\n', "\r\n");
        }
        if trailing_newline {
            out.push('\n');
        }
        fs::write(p, out)?;
        Ok(format!("Patch basariyla uygulandi: {}", path))
    } else {
        Err(anyhow!("Patch uygulanamadi: diff kaliplari hedef dosyada eslesmedi."))
    }
}

/// Canli Web Aramasi (DuckDuckGo motoru, sifir harici anahtar gerektirir)
pub async fn web_search(query: &str, max_results: Option<usize>) -> Result<String> {
    let limit = max_results.unwrap_or(5).clamp(1, 10);
    let client = shared_http_client(Duration::from_secs(12));

    let url = format!("https://html.duckduckgo.com/html/?q={}", url_encode(query));
    let res = client.get(&url).send().await
        .map_err(|e| anyhow!("Arama istegi basarisiz: {e}"))?;

    let html = res.text().await.unwrap_or_default();
    let mut results = Vec::new();

    let re_snippet = regex::Regex::new(r#"<a class="result__snippet"[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#)?;
    let re_html_tag = regex::Regex::new(r#"<[^>]+>"#)?;

    for cap in re_snippet.captures_iter(&html) {
        if results.len() >= limit {
            break;
        }
        let raw_link = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let raw_snippet = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        let clean_snippet = re_html_tag.replace_all(raw_snippet, "")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#x27;", "'");

        let link = if let Some(idx) = raw_link.find("uddg=") {
            let encoded = &raw_link[idx + 5..];
            let end_idx = encoded.find('&').unwrap_or(encoded.len());
            url_decode(&encoded[..end_idx])
        } else {
            raw_link.to_string()
        };

        results.push(format!("{}. [Link: {}]\n   {}", results.len() + 1, link, clean_snippet.trim()));
    }

    if results.is_empty() {
        let instant_url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1", url_encode(query));
        if let Ok(instant_res) = client.get(&instant_url).send().await
            && let Ok(json_val) = instant_res.json::<Value>().await
                && let Some(abs) = json_val.get("AbstractText").and_then(|v| v.as_str())
                    && !abs.is_empty() {
                        let source_url = json_val.get("AbstractURL").and_then(|v| v.as_str()).unwrap_or("");
                        return Ok(format!("1. [Ozet: {}]\n   {}", source_url, abs));
                    }
        return Ok(format!("'{}' icin web aramasinda sonuc bulunamadi.", query));
    }

    Ok(format!("◆ Web Arama Sonuclari ('{}'):\n\n{}", query, results.join("\n\n")))
}

/// Görev ve Checklist Takipçisi (Glitch Code task seviyesinde)
pub fn task_tracker(action: &str, task: Option<&str>, id: Option<usize>) -> Result<String> {
    let task_file = Path::new(".elytra_tasks.json");
    let mut tasks: Vec<Value> = if task_file.exists() {
        let content = fs::read_to_string(task_file).unwrap_or_else(|_| "[]".into());
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    match action {
        "add" => {
            let text = task.ok_or_else(|| anyhow!("'task' parametresi eksik"))?;
            let next_id = tasks.len() + 1;
            tasks.push(json!({
                "id": next_id,
                "task": text,
                "completed": false,
                "created_at": chrono::Utc::now().to_rfc3339()
            }));
            let _ = fs::write(task_file, serde_json::to_string_pretty(&tasks)?);
            Ok(format!("Gorev #{next_id} eklendi: {text}"))
        }
        "complete" => {
            let task_id = id.ok_or_else(|| anyhow!("'id' parametresi eksik"))?;
            let mut found = false;
            for t in &mut tasks {
                if t.get("id").and_then(|v| v.as_u64()) == Some(task_id as u64) {
                    t["completed"] = json!(true);
                    found = true;
                    break;
                }
            }
            if found {
                let _ = fs::write(task_file, serde_json::to_string_pretty(&tasks)?);
                Ok(format!("Gorev #{task_id} tamamlandi olarak isaretlendi!"))
            } else {
                Err(anyhow!("Gorev #{task_id} bulunamadi."))
            }
        }
        "list" => {
            if tasks.is_empty() {
                return Ok("Kayitli herhangi bir gorev yok.".into());
            }
            let mut out = String::from("▌ ELYTRA GOREV LISTESI\n");
            for t in &tasks {
                let id = t.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let text = t.get("task").and_then(|v| v.as_str()).unwrap_or("");
                let completed = t.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);
                let mark = if completed { "[✓]" } else { "[ ]" };
                out.push_str(&format!("  {} #{} : {}\n", mark, id, text));
            }
            Ok(out)
        }
        "clear" => {
            let _ = fs::remove_file(task_file);
            Ok("Tum gorevler temizlendi.".into())
        }
        _ => Err(anyhow!("Gecersiz gorev islemi: '{}'. Desteklenenler: add, list, complete, clear", action)),
    }
}

/// Akıllı Repo / Kod Tabanı Haritası
pub fn repo_map(dir: Option<&str>) -> Result<String> {
    let root = Path::new(dir.unwrap_or("."));
    if !root.exists() {
        return Err(anyhow!("Dizin bulunamadi: {:?}", root));
    }

    let mut ext_counts: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
    let mut key_files = Vec::new();
    let mut total_files = 0;
    let mut total_lines = 0;

    let important_names = [
        "Cargo.toml", "package.json", "tsconfig.json", "README.md",
        "Makefile", "pyproject.toml", "CMakeLists.txt", "Dockerfile"
    ];

    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "target" | "node_modules" | "dist" | ".cargo" | ".turbo")
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total_files += 1;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if important_names.contains(&file_name.as_str()) {
                let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
                key_files.push(rel.display().to_string());
            }

            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                let ext_key = format!(".{}", ext.to_lowercase());
                let lines_count = match crate::util::read_text_file_safe(entry.path(), 2_000_000) {
                    Ok(content) => content.lines().count(),
                    Err(_) => 0, // binary veya devasa dosyalari atla
                };
                total_lines += lines_count;
                let stat = ext_counts.entry(ext_key).or_insert((0, 0));
                stat.0 += 1;
                stat.1 += lines_count;
            }
        }
    }

    let project_type = if root.join("Cargo.toml").exists() {
        "Rust (Cargo)"
    } else if root.join("package.json").exists() {
        "Node.js / TypeScript"
    } else if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        "Python"
    } else if root.join("go.mod").exists() {
        "Go"
    } else {
        "Genel Kod Deposu"
    };

    let mut out = format!(
        "◆ REPO HARITASI ({})\n  Proje Tipi   : {}\n  Toplam Dosya : {}\n  Toplam Satir : ~{}\n\n",
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf()).display(),
        project_type,
        total_files,
        total_lines
    );

    if !key_files.is_empty() {
        out.push_str("  Temel Yapilandirma Dosyalari:\n");
        for k in key_files {
            out.push_str(&format!("    • {}\n", k));
        }
        out.push('\n');
    }

    out.push_str("  Dil & Dosya Turu Dagilimi:\n");
    let mut sorted_exts: Vec<_> = ext_counts.into_iter().collect();
    sorted_exts.sort_by_key(|e| std::cmp::Reverse(e.1.1));

    for (ext, (count, lines)) in sorted_exts.iter().take(8) {
        out.push_str(&format!("    • {:<8} : {:>3} dosya ({:>6} satir)\n", ext, count, lines));
    }

    Ok(out)
}

/// Paylaşılan HTTP istemcisi (bağlantı havuzu yeniden kullanımı)
fn shared_http_client(timeout: Duration) -> reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT_12S: OnceLock<reqwest::Client> = OnceLock::new();
    let _ = timeout; // tek istemci 12s timeout ile paylaşılır
    CLIENT_12S
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .timeout(Duration::from_secs(12))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn url_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                bytes.push(b);
            }
        } else if c == '+' {
            bytes.push(b' ');
        } else {
            bytes.push(c as u8);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

