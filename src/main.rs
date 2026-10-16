//! Elytra — Terminal AI Asistani & Otonom Kod Ajani
//!
//! Sifirdan Rust ile yazildi. Glitch Code ve Claude Code seviyesinde yetenekleri
//! minimum bellek (10-15 MB RAM) ve sifir islemci gecikmesiyle sunar.
//! Sifir emoji — saf vektor estetik tasarimi ve yuksek performans.

use clap::{Parser, Subcommand};

use elytra_cli::agent;
use elytra_cli::api;
use elytra_cli::config;
use elytra_cli::scan;
use elytra_cli::tools;
use elytra_cli::ui;
use elytra_cli::util;

use agent::{AgentEvent, AgentSession};
use config::AppConfig;
use util::truncate_chars;

#[derive(Parser, Debug)]
#[command(
    name = "elytra",
    version,
    about = "Elytra — Iridesan Hiz Motoruyla Guclendirilmis Terminal AI Asistani & Otonom Ajan",
    long_about = "Elytra, Glitch Code'un tum yeteneklerini (ReAct ajan dongusu, \
                  dosya okuma/yazma/duzenleme, guvenli terminal komutlari, coklu model saglayicisi destegi) \
                  sifir bellek yuku ve milisaniyelik yanit hiziyla sunan yerel Rust CLI aracidir."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Tek seferlik baslangic sorusu (orn: elytra "Merhaba")
    #[arg(trailing_var_arg = true)]
    prompt: Vec<String>,

    /// Kullanilacak model (orn: groq, gemini-2.0-flash, gpt-4o, deepseek-chat, qwen2.5-coder)
    #[arg(short, long)]
    model: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Otonom ReAct Ajan modunda gorev coz (kod analizi, duzenleme, komut calistirma)
    Agent {
        /// Cozulecek gorev aciklamasi
        #[arg(trailing_var_arg = true, required = true)]
        task: Vec<String>,

        /// Kullanilacak model
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Model saglayicilarini ve ag baglantilarini tara (orn: elytra test, elytra test all, elytra test groq)
    Test {
        /// Test edilecek saglayici kimligi (bos birakilirsa tum saglayicilar paralel taranir)
        provider: Option<String>,
        /// Tam dogrulama modu: baglanti + minimal chat completion (kota harcar)
        #[arg(long)]
        full: bool,
        /// Sonuclari JSON olarak yazdir (makine okunabilir)
        #[arg(long)]
        json: bool,
        /// Sadece anahtar kontrolu (aga hic cikmaz)
        #[arg(long)]
        keys: bool,
        /// Saglayici basina zaman asimi (milisaniye, varsayilan: 6000)
        #[arg(long, default_value = "6000")]
        timeout: u64,
        /// Es zamanli calisacak maksimum tarama sayisi (varsayilan: 8)
        #[arg(long, default_value = "8")]
        concurrency: usize,
    },
    /// Desteklenen saglayici ve modelleri listele
    Models,
    /// Ajanin kullanabilecegi dahili araclari listele
    Tools,
    /// Yapilandirma ayarlarini yonet (~/.config/elytra/config.toml)
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Mevcut ayarlari ve anahtarlari goster
    Show,
    /// Yapilandirma anahtari ata (orn: elytra config set groq_api_key gsk_...)
    Set {
        key: String,
        value: String,
    },
    /// Yapilandirma dosyasinin konumunu goster
    Path,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Agent { task, model }) => {
            let task_str = task.join(" ");
            run_cli_agent(&task_str, model.as_deref()).await?;
        }
        Some(Commands::Test { provider, full, keys, timeout, concurrency, json }) => {
            run_cli_test(provider.as_deref(), full, keys, timeout, concurrency, json).await?;
        }
        Some(Commands::Models) => {
            print_models();
        }
        Some(Commands::Tools) => {
            print_tools();
        }
        Some(Commands::Config { action }) => {
            handle_config(action)?;
        }
        None => {
            // Komut verilmemisse: Tek seferlik soru mu yoksa interaktif TUI mi?
            if !cli.prompt.is_empty() {
                let prompt = cli.prompt.join(" ");
                println!("{}", api::ask_once(&prompt, cli.model.as_deref()).await?);
            } else {
                // Varsayilan interaktif OpenCode-tarzi TUI
                ui::run(cli.model.as_deref()).await?;
            }
        }
    }

    Ok(())
}

/// CLI Saglayici Taramasi (paralel, iki kademeli)
async fn run_cli_test(
    provider: Option<&str>,
    full: bool,
    keys_only: bool,
    timeout_ms: u64,
    concurrency: usize,
    json_output: bool,
) -> anyhow::Result<()> {
    let config = AppConfig::load();
    let client = api::ApiClient::new(config);

    let mode = if keys_only {
        scan::ScanMode::Keys
    } else if full {
        scan::ScanMode::Full
    } else {
        scan::ScanMode::Connectivity
    };

    let options = scan::ScanOptions {
        mode,
        per_provider_timeout: std::time::Duration::from_millis(timeout_ms.max(500)),
        concurrency: concurrency.max(1),
    };

    if !json_output {
        println!("\x1b[38;2;64;224;160m◆ ELYTRA SAGLAYICI TARAMASI\x1b[0m [{}]\n", mode.label());
    }

    // Tek sağlayıcı modu
    if let Some(p) = provider
        && p.to_lowercase() != "all" {
            let prov = api::PROVIDERS
                .iter()
                .find(|pv| pv.id.eq_ignore_ascii_case(p));

            let result = match prov {
                Some(pv) => scan::scan_single(&client, pv, &options).await,
                None => {
                    // Katalogda yoksa eski dogrudan endpoint denemesi
                    let legacy = client.test_provider(p).await;
                    scan::ScanResult {
                        provider_id: legacy.provider_id,
                        provider_name: legacy.provider_name,
                        status: if legacy.success {
                            scan::ScanStatus::Ok
                        } else if legacy.latency_ms == 0 {
                            scan::ScanStatus::NoKey
                        } else {
                            scan::ScanStatus::Unreachable
                        },
                        latency_ms: legacy.latency_ms,
                        detail: legacy.message,
                    }
                }
            };
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", result.report_line());
                println!("\n\x1b[38;2;110;125;115mIpucu: API anahtari eklemek icin: elytra config set <saglayici>_api_key <anahtar>\x1b[0m");
            }
            return Ok(());
        }

    // Paralel tam tarama — sonuclar hazir oldukca akar (JSON modunda sessiz toplanir)
    let mut results_json: Vec<scan::ScanResult> = Vec::new();
    let summary = scan::scan_all_providers(&client, &options, |r| {
        if json_output {
            results_json.push(r.clone());
        } else {
            println!("{}", r.report_line());
        }
    })
    .await;

    if json_output {
        let payload = serde_json::json!({
            "mode": mode.label(),
            "summary": summary,
            "results": results_json,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("\n{}", summary.format_report(mode));
        println!("\n\x1b[38;2;110;125;115mIpucu: Tam dogrulama icin 'elytra test all --full' (kota harcar). Anahtar kontrolu icin '--keys'. Makine icin '--json'.\x1b[0m");
    }
    Ok(())
}

/// CLI Otonom Ajan Modu
async fn run_cli_agent(task: &str, model_override: Option<&str>) -> anyhow::Result<()> {
    let config = AppConfig::load();
    let mut selection = api::Selection::default();

    if let Some(m) = model_override {
        selection.set_by_name(m);
    } else if let Some(ref def_m) = config.default_model {
        selection.set_by_name(def_m);
    }

    println!("\x1b[38;2;64;224;160m◆ ELYTRA AJAN MODU\x1b[0m");
    println!("Model: \x1b[38;2;80;200;220m{}\x1b[0m", selection.full_id());
    println!("Gorev: \x1b[38;2;230;220;160m{}\x1b[0m\n", task);

    let mut session = AgentSession::new(
        selection.provider().id,
        &selection.model_id(),
        config,
    );

    let _ = session
        .run(task, |event| match event {
            AgentEvent::Thought(th) => {
                println!("\x1b[38;2;110;125;115m[Dusunce]\x1b[0m {}", th);
            }
            AgentEvent::ToolExecuting { name, args } => {
                println!(
                    "\x1b[38;2;160;110;230m◈ Arac Calistiriliyor:\x1b[0m \x1b[1m{}\x1b[0m({})",
                    name, args
                );
            }
            AgentEvent::ToolFinished { name, result } => {
                let preview = truncate_chars(&result, 150);
                println!("\x1b[38;2;120;230;130m✓ {} Bitti:\x1b[0m {}", name, preview);
            }
            AgentEvent::FinalAnswer(ans) => {
                println!("\n\x1b[38;2;64;224;160m◆ Elytra Yaniti:\x1b[0m\n{}", ans);
            }
            AgentEvent::Error(err) => {
                println!("\x1b[38;2;230;90;90m[X] Hata:\x1b[0m {}", err);
            }
            AgentEvent::StepLimitReached => {
                println!("\x1b[38;2;230;160;120m[!] Maksimum adim limitine ulasildi.\x1b[0m");
            }
            AgentEvent::Retrying { attempt, reason } => {
                println!(
                    "\x1b[38;2;230;160;120m[!] Gecici hata — yeniden deneniyor ({}. deneme): {}\x1b[0m",
                    attempt, reason
                );
            }
        })
        .await?;

    Ok(())
}

/// Modelleri listele
fn print_models() {
    println!("\x1b[38;2;64;224;160m◆ ELYTRA MODEL VE SAGLAYICI KATALOGU ({} Saglayici)\x1b[0m\n", elytra_cli::api::providers::PROVIDERS.len());
    for p in api::PROVIDERS {
        let tier_label = if p.is_free_tier { "\x1b[32m[Ucretsiz Katman]\x1b[0m" } else { "\x1b[34m[Bulut/API]\x1b[0m" };
        println!("\x1b[1m{} ({})\x1b[0m {} — {}", p.name, p.id, tier_label, p.hint);
        for m in p.models {
            let free_str = if m.free { "\x1b[32m[Ucretsiz]\x1b[0m" } else { "[API Key]" };
            println!("  • {:<36} {:<10} Baglam: {}K", format!("{}/{}", p.id, m.id), free_str, m.context / 1000);
        }
        println!();
    }
}

/// Araçları listele
fn print_tools() {
    println!("\x1b[38;2;64;224;160m◆ ELYTRA DAHILI AJAN ARACLARI\x1b[0m\n");
    for t in tools::get_available_tools() {
        println!("◈ \x1b[1m{}\x1b[0m: {}", t.name, t.description);
    }
}

/// Yapılandırma yönetimi
fn handle_config(action: Option<ConfigAction>) -> anyhow::Result<()> {
    match action.unwrap_or(ConfigAction::Show) {
        ConfigAction::Path => {
            if let Some(p) = AppConfig::global_config_path() {
                println!("Yapilandirma dosya yolu: {}", p.display());
            } else {
                println!("Yapilandirma dizini tespit edilemedi.");
            }
        }
        ConfigAction::Show => {
            let cfg = AppConfig::load();
            println!("\x1b[38;2;64;224;160m◆ ELYTRA YAPILANDIRMASI\x1b[0m");
            println!("Varsayilan Saglayici : {:?}", cfg.default_provider);
            println!("Varsayilan Model     : {:?}", cfg.default_model);
            println!("Groq API Key         : {}", if cfg.groq_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Gemini API Key       : {}", if cfg.gemini_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("OpenRouter API Key   : {}", if cfg.openrouter_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Cerebras API Key     : {}", if cfg.cerebras_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("SambaNova API Key    : {}", if cfg.sambanova_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Mistral API Key      : {}", if cfg.mistral_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("DeepSeek API Key     : {}", if cfg.deepseek_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("OpenAI API Key       : {}", if cfg.openai_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Anthropic API Key    : {}", if cfg.anthropic_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Together API Key     : {}", if cfg.together_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Perplexity API Key   : {}", if cfg.perplexity_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Cohere API Key       : {}", if cfg.cohere_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Nous API Key         : {}", if cfg.nous_api_key.is_some() { "Tanimli" } else { "Yok" });
            println!("Ozel Base URL        : {:?}", cfg.custom_base_url);
        }
        ConfigAction::Set { key, value } => {
            let mut cfg = AppConfig::load();
            match key.to_lowercase().as_str() {
                "default_provider" => cfg.default_provider = Some(value.clone()),
                "default_model" => cfg.default_model = Some(value.clone()),
                "groq_api_key" | "groq" => cfg.groq_api_key = Some(value.clone()),
                "gemini_api_key" | "gemini" | "google" => cfg.gemini_api_key = Some(value.clone()),
                "openrouter_api_key" | "openrouter" => cfg.openrouter_api_key = Some(value.clone()),
                "cerebras_api_key" | "cerebras" => cfg.cerebras_api_key = Some(value.clone()),
                "sambanova_api_key" | "sambanova" => cfg.sambanova_api_key = Some(value.clone()),
                "mistral_api_key" | "mistral" => cfg.mistral_api_key = Some(value.clone()),
                "deepseek_api_key" | "deepseek" => cfg.deepseek_api_key = Some(value.clone()),
                "openai_api_key" | "openai" => cfg.openai_api_key = Some(value.clone()),
                "anthropic_api_key" | "anthropic" => cfg.anthropic_api_key = Some(value.clone()),
                "together_api_key" | "together" => cfg.together_api_key = Some(value.clone()),
                "perplexity_api_key" | "perplexity" => cfg.perplexity_api_key = Some(value.clone()),
                "cohere_api_key" | "cohere" => cfg.cohere_api_key = Some(value.clone()),
                "nous_api_key" | "nous" => cfg.nous_api_key = Some(value.clone()),
                "grok_api_key" | "grok" | "xai" => cfg.grok_api_key = Some(value.clone()),
                "custom_base_url" | "base_url" => cfg.custom_base_url = Some(value.clone()),
                _ => {
                    println!("\x1b[31mBilinmeyen ayar anahtari: {}\x1b[0m", key);
                    return Ok(());
                }
            }
            cfg.save_global()?;
            println!("\x1b[32m✓ '{}' ayari basariyla kaydedildi.\x1b[0m", key);
        }
    }
    Ok(())
}
