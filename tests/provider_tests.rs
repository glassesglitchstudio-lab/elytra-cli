//! Elytra Model Sağlayıcıları ve İstemci Test Paketi

use elytra_cli::api::{ApiClient, PROVIDERS};
use elytra_cli::config::AppConfig;

#[tokio::test]
async fn test_provider_catalog_completeness() {
    // 15 sağlayıcının tamamının tanımlı olduğunu doğrula
    assert!(PROVIDERS.len() >= 15, "En az 15 sağlayıcı kayıtlı olmalıdır");

    let expected_ids = [
        "openrouter", "nous", "groq", "google", "cerebras", "deepseek",
        "mistral", "zai", "moonshot", "xai", "openai", "anthropic",
        "together", "fireworks", "nvidia",
    ];

    for expected in expected_ids {
        let exists = PROVIDERS.iter().any(|p| p.id == expected);
        assert!(exists, "Sağlayıcı '{}' katalogda bulunamadı!", expected);
    }

    // Katalogda sahte/yerel motor bulunmamalı
    for banned in ["demo", "offline", "local", "ollama", "perplexity", "sambanova", "cohere"] {
        let exists = PROVIDERS.iter().any(|p| p.id == banned);
        assert!(!exists, "Sahte sağlayıcı '{}' katalogda BULUNAMAZ!", banned);
    }
}

#[tokio::test]
async fn test_missing_key_reports_honest_error() {
    let config = AppConfig::default(); // tüm anahtarlar boş
    let client = ApiClient::new(config);

    // Gerçek bir sağlayıcı için anahtar yoksa dürüstçe hata dönmeli
    let res = client.test_provider("openai").await;
    assert!(!res.success, "Anahtarsız sağlayıcı başarılı olmamalı");
    assert!(res.message.to_lowercase().contains("anahtar"),
        "Hata mesajı anahtar gerektiğini belirtmelidir: {}", res.message);
}

#[tokio::test]
async fn test_selection_model_resolution() {
    use elytra_cli::api::Selection;

    let mut sel = Selection::default();

    // 1. Sağlayıcı ID ile ayar
    assert!(sel.set_provider_by_id("groq"));
    assert_eq!(sel.provider().id, "groq");

    // 2. Önekli model seçimi
    assert!(sel.set_by_name("openrouter/deepseek/deepseek-r1"));
    assert_eq!(sel.provider().id, "openrouter");
    assert_eq!(sel.model_id(), "deepseek/deepseek-r1");

    // 3. Özel model seçimi (katalogda olmayan herhangi bir model adı)
    assert!(sel.set_by_name("custom-model-x-99"));
    assert_eq!(sel.model_id(), "custom-model-x-99");
}

#[tokio::test]
async fn test_no_emojis_in_provider_metadata() {
    // Kullanıcı kuralı: UI ve katalogda emoji olmamalı
    let emoji_patterns = ["😀", "⚡", "📁", "💾", "⚙", "⚠️", "👉", "💡", "🚀", "🛠", "🪽"];

    for p in PROVIDERS {
        for em in &emoji_patterns {
            assert!(
                !p.name.contains(em),
                "Sağlayıcı adı '{}' emoji '{}' içeremez!",
                p.name, em
            );
            assert!(
                !p.hint.contains(em),
                "Sağlayıcı açıklaması '{}' emoji '{}' içeremez!",
                p.hint, em
            );
        }
    }
}

#[tokio::test]
async fn test_tool_catalog_fifteen() {
    let tools = elytra_cli::tools::get_available_tools();
    assert_eq!(tools.len(), 15, "15 dahili araç tanımlı olmalıdır");

    let expected_names = [
        "read_file", "write_file", "edit_file", "apply_patch", "list_dir",
        "grep_search", "file_search", "git_status", "git_diff", "diagnostics",
        "bash_exec", "web_fetch", "web_search", "task_tracker", "repo_map"
    ];

    for name in expected_names {
        assert!(tools.iter().any(|t| t.name == name), "Araç '{}' bulunamadı!", name);
    }
}

#[tokio::test]
async fn test_real_file_and_repo_tools() {
    // 1. Gerçek dosya okuma
    let read_res = elytra_cli::tools::read_file("Cargo.toml", Some(1), Some(6))
        .expect("Cargo.toml okunabilmelidir");
    assert!(read_res.contains("elytra-cli"), "Cargo.toml paket adını içermelidir");

    // 2. Gerçek dizin listeleme
    let list_res = elytra_cli::tools::list_dir("src", 2)
        .expect("src dizini listelenebilmelidir");
    assert!(list_res.contains("main.rs"), "src dizini main.rs içermelidir");

    // 3. Gerçek repo haritası
    let map_res = elytra_cli::tools::repo_map(Some("."))
        .expect("Repo haritası üretilebilmelidir");
    assert!(map_res.contains("Rust (Cargo)"), "Proje tipi Rust olarak tespit edilmelidir");
    assert!(map_res.contains(".rs"), "Dil dağılımında .rs bulunmalıdır");
}

#[tokio::test]
async fn test_task_tracker_lifecycle() {
    // 1. Görev ekle
    let add_res = elytra_cli::tools::task_tracker("add", Some("Test görevini tamamla"), None)
        .expect("Görev eklenebilmelidir");
    assert!(add_res.contains("Test görevini tamamla"));

    // 2. Görevleri listele
    let list_res = elytra_cli::tools::task_tracker("list", None, None)
        .expect("Görevler listelenebilmelidir");
    assert!(list_res.contains("Test görevini tamamla"));

    // 3. Görevi tamamla
    let comp_res = elytra_cli::tools::task_tracker("complete", None, Some(1))
        .expect("Görev tamamlanabilmelidir");
    assert!(comp_res.contains("tamamlandi"));

    // 4. Temizle
    let _ = elytra_cli::tools::task_tracker("clear", None, None);
}

#[tokio::test]
async fn test_extract_tool_calls_from_text() {
    use elytra_cli::api::client::extract_tool_calls_from_text;

    // XML etiketi
    let text_xml = "Dosyayı inceliyorum.\n<tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"Cargo.toml\"}}</tool_call>";
    let (clean, calls) = extract_tool_calls_from_text(text_xml);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "read_file");
    assert!(!clean.contains("<tool_call>"));

    // Markdown JSON bloğu
    let text_json = "```json\n{\"name\": \"list_dir\", \"arguments\": {\"path\": \"src\"}}\n```";
    let (_, calls_json) = extract_tool_calls_from_text(text_json);
    assert_eq!(calls_json.len(), 1);
    assert_eq!(calls_json[0].function.name, "list_dir");
}


