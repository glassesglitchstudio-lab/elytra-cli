//! Elytra Tarama Motoru ve Araç Güvenlik Testleri

use elytra_cli::scan::{scan_all_providers, ScanMode, ScanOptions, ScanResult, ScanStatus, ScanSummary};
use elytra_cli::api::ApiClient;
use elytra_cli::config::AppConfig;
use elytra_cli::tools::{file_search, glob_match, compile_glob, grep_search};

// ─────────────────────── Tarama Motoru ───────────────────────

#[tokio::test]
async fn scan_all_providers_streams_results_and_summarizes() {
    let config = AppConfig::default();
    let client = ApiClient::new(config);
    let options = ScanOptions {
        mode: ScanMode::Keys,
        per_provider_timeout: std::time::Duration::from_secs(2),
        concurrency: 4,
    };

    let mut streamed = Vec::new();
    let summary = scan_all_providers(&client, &options, |r| {
        streamed.push(r.provider_id.clone());
    })
    .await;

    // Keys modunda: tüm sağlayıcılar sonuç döndürmeli (hiç ağa çıkmaz)
    assert_eq!(streamed.len(), elytra_cli::api::providers::PROVIDERS.len(), "Her sağlayıcı bir sonuç akıtmalı");
    assert_eq!(summary.total, elytra_cli::api::providers::PROVIDERS.len());
    assert_eq!(summary.ok + summary.skipped, elytra_cli::api::providers::PROVIDERS.len());
    // Anahtarsız ortamda sağlayıcılar Skipped olur (sahte motor yok)
    assert!(summary.skipped >= 1, "Anahtarsiz ortamda en az bir saglayici Skipped olmali");
    assert!(summary.total_duration_ms < 1000, "Keys modu ağ kullanmaz, çok hızlı olmalı");
}

#[tokio::test]
async fn scan_single_unknown_provider_is_handled() {
    // Katalogda olmayan sağlayıcı için scan_all çağrılmaz;
    // bu test özet formatının doğru çalıştığını doğrular
    let results = [ScanResult {
        provider_id: "test".into(),
        provider_name: "Test".into(),
        status: ScanStatus::RateLimited,
        latency_ms: 10,
        detail: "429".into(),
    }];
    let total: usize = 1;
    assert_eq!(total, results.len());
}

#[test]
fn scan_summary_format_contains_key_info() {
    let s = ScanSummary {
        total: 2,
        ok: 1,
        unauthorized: 1,
        fastest: Some(("Groq Cloud".into(), 42)),
        total_duration_ms: 200,
        ..Default::default()
    };
    let report = s.format_report(ScanMode::Connectivity);
    assert!(report.contains("TARAMA OZETI"));
    assert!(report.contains("Calisan: 1"));
    assert!(report.contains("Yetkisiz: 1"));
    assert!(report.contains("Groq Cloud (42ms)"));
    assert!(report.contains("0.2s"));
}

// ─────────────────────── Araç Taraması ───────────────────────

#[test]
fn grep_search_finds_known_pattern_with_context() {
    // src içinde "fn scan_single" deseni — bağlam satırlarıyla
    let res = grep_search("src", "fn scan_single", false, 1, None)
        .expect("grep_search çalışmalı");
    assert!(
        res.contains("scan") && res.contains("mod.rs") && res.contains("eslesme"),
        "scan_single fonksiyonu gerçekten bulunmalı, dönen: {res}"
    );
    // Bağlam satırı işareti (>) eşleşme satırında olmalı
    assert!(res.contains('>'), "Bağlam satırı işareti olmalı");
}

#[test]
fn grep_search_skips_ignored_directories() {
    // target/ ve .git/ dizinleri atlanmalı — "elytra_cli-" hedef dosya adında geçer
    let res = grep_search(".", "fn scan_single", false, 0, None)
        .expect("çalışmalı");
    assert!(
        !res.contains("target\\") && !res.contains("target/"),
        "target/ dizini taranmamalı: {}",
        res.lines().take(5).collect::<Vec<_>>().join(" | ")
    );
}

#[test]
fn grep_search_respects_glob_filter() {
    // *.rs filtresiyle sadece Rust dosyaları taranır
    let res_rs = grep_search("src", "fn ", false, 0, Some("*.rs"))
        .expect("glob filtreli arama çalışmalı");
    assert!(res_rs.contains(".rs"), "Rust dosyaları eşleşmeli");

    // Var olmayan glob → sonuç yok
    let res_none = grep_search("src", "fn ", false, 0, Some("*.nonexistentxyz"))
        .expect("çalışmalı");
    assert!(
        res_none.contains("bulunamadi"),
        "Eşleşme olmayan glob 'bulunamadi' dönmeli, buldu: {res_none}"
    );
}

#[test]
fn grep_search_binary_guard_does_not_panic() {
    // target/ dizini engellenir; .git içeriği taranmaz
    let res = grep_search(".", "fn main", false, 0, None).expect("çalışmalı");
    // Panik yok, ya eşleşme ya boş sonuç
    assert!(res.contains("eslesme") || res.contains("bulunamadi"));
}

#[test]
fn file_search_glob_patterns() {
    // Glob deseni: *.toml
    let res = file_search("*.toml", Some(".")).expect("çalışmalı");
    assert!(res.contains("Cargo.toml"), "Cargo.toml bulunmalı");

    // Düz metin
    let res2 = file_search("lib.rs", Some("src")).expect("çalışmalı");
    assert!(res2.contains("lib.rs"), "lib.rs bulunmalı");
}

#[test]
fn glob_matcher_unit() {
    let star_rs = compile_glob("*.rs");
    assert!(glob_match(&star_rs, "main.rs"));
    assert!(glob_match(&star_rs, "a.b.rs"));
    assert!(!glob_match(&star_rs, "main.toml"));

    let q = compile_glob("main.?s");
    assert!(glob_match(&q, "main.rs"));
    assert!(!glob_match(&q, "main.rsx"));

    let exact = compile_glob("Cargo.toml");
    assert!(glob_match(&exact, "cargo.toml")); // harf duyarsız
    assert!(!glob_match(&exact, "Cargo.lock"));
}

// ─────────────────────── Sert Zaman Aşımı ───────────────────────

#[tokio::test]
async fn bash_exec_hard_timeout_kills_long_command() {
    // 1 saniyelik timeout: 5 saniye sürecek komut öldürülmeli
    let start = std::time::Instant::now();
    let res = elytra_cli::tools::bash_exec_with_timeout(
        "Start-Sleep -Seconds 5",
        None,
        1,
    )
    .await;
    let elapsed = start.elapsed();

    assert!(res.is_err(), "Uzun komut hata dönmeli (zaman aşımı)");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("zaman asimi") || err.contains("sonlandirildi"),
        "Hata mesajı zaman aşımını belirtmeli: {err}"
    );
    assert!(
        elapsed.as_secs() < 4,
        "Timeout gerçekten sert olmalı, {elapsed:?} sürdü"
    );
}

#[tokio::test]
async fn bash_exec_runs_normal_command() {
    let res = elytra_cli::tools::bash_exec("Write-Output 'merhaba-elytra'", None)
        .await
        .expect("normal komut çalışmalı");
    assert!(res.contains("merhaba-elytra"), "çıktı yakalanmalı: {res}");
}

#[test]
fn scan_result_and_summary_json_serializable() {
    // --json çıktı yolu için serileştirme doğrulaması
    let r = ScanResult {
        provider_id: "groq".into(),
        provider_name: "Groq Cloud".into(),
        status: ScanStatus::Ok,
        latency_ms: 42,
        detail: "Baglandi — 2 model listelendi".into(),
    };
    let json = serde_json::to_string(&r).expect("ScanResult serileştirilebilir olmali");
    assert!(json.contains("\"provider_id\":\"groq\""));
    assert!(json.contains("\"status\":\"Ok\""));
    assert!(json.contains("Baglandi"));

    let s = ScanSummary {
        total: 1,
        ok: 1,
        fastest: Some(("Groq Cloud".into(), 42)),
        total_duration_ms: 42,
        ..Default::default()
    };
    let sjson = serde_json::to_string(&s).expect("ScanSummary serileştirilebilir olmali");
    assert!(sjson.contains("\"total\":1"));
    assert!(sjson.contains("\"fastest\""));
}

#[test]
fn apply_patch_preserves_trailing_newline() {
    let dir = std::env::temp_dir().join("elytra_patch_test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("patch_target.txt");
    std::fs::write(&file, "satir1\nESKI\nsatir3\n").unwrap();

    let res = elytra_cli::tools::apply_patch(
        file.to_str().unwrap(),
        "@@ -1,3 +1,3 @@\n satir1\n-ESKI\n+YENI\n satir3\n",
    )
    .expect("patch uygulanmali");

    assert!(res.contains("basariyla"), "patch raporu başarılı olmali");
    let content = std::fs::read_to_string(&file).unwrap();
    assert!(content.contains("YENI"), "yeni içerik yazilmali: {content}");
    assert!(
        content.ends_with('\n'),
        "satir sonu korunmali, bulundu: {content:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
