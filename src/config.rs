//! Elytra Yapılandırma Yöneticisi (~/.config/elytra/config.toml & .elytra.toml)
//!
//! Hem küresel hem de proje bazlı yapılandırmayı destekler.
//! API anahtarları, varsayılan modeller ve ortam değişkenleri ile tam uyumludur.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Varsayılan sağlayıcı (örn: "groq", "openrouter", "google", "openai", "deepseek")
    pub default_provider: Option<String>,
    /// Varsayılan model (örn: "gemini-2.0-flash", "llama-3.3-70b-versatile", "gpt-4o")
    pub default_model: Option<String>,

    // API Anahtarları
    pub nous_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub grok_api_key: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub groq_api_key: Option<String>,
    pub cerebras_api_key: Option<String>,
    pub sambanova_api_key: Option<String>,
    pub mistral_api_key: Option<String>,
    pub together_api_key: Option<String>,
    pub perplexity_api_key: Option<String>,
    pub cohere_api_key: Option<String>,

    /// Özel OpenAI-uyumlu endpoint (vLLM, LMStudio veya özel sunucu için)
    pub custom_base_url: Option<String>,
    pub custom_api_key: Option<String>,

    /// Model parametreleri
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,

    /// Ajan sürdürülebilirlik ayarları
    /// Maksimum ReAct adım sayısı (varsayılan: 12)
    pub agent_max_steps: Option<usize>,
    /// Geçici hatalarda maksimum yeniden deneme (varsayılan: 3)
    pub agent_max_retries: Option<u32>,
}

impl AppConfig {
    /// Öncelik sırasıyla ayarları yükler:
    /// 1. Mevcut dizindeki `.elytra.toml` (proje özelinde)
    /// 2. Kullanıcı dizinindeki `~/.config/elytra/config.toml`
    /// 3. Ortam değişkenleri (OPENAI_API_KEY, GROQ_API_KEY, GEMINI_API_KEY vb.)
    pub fn load() -> Self {
        let mut config = Self::load_from_files().unwrap_or_default();
        config.apply_env_overrides();
        config
    }

    fn load_from_files() -> Option<Self> {
        // 1. Proje dizininde .elytra.toml var mı?
        let local_path = Path::new(".elytra.toml");
        if local_path.exists()
            && let Ok(content) = fs::read_to_string(local_path)
                && let Ok(cfg) = toml::from_str::<AppConfig>(&content) {
                    return Some(cfg);
                }

        // 2. Global config dizini (~/.config/elytra/config.toml)
        let global_path = Self::global_config_path()?;
        if global_path.exists()
            && let Ok(content) = fs::read_to_string(&global_path)
                && let Ok(cfg) = toml::from_str::<AppConfig>(&content) {
                    return Some(cfg);
                }

        None
    }

    /// Ortam değişkenlerinden anahtarları tamamla
    fn apply_env_overrides(&mut self) {
        if self.nous_api_key.is_none() {
            self.nous_api_key = std::env::var("NOUS_API_KEY").ok();
        }
        if self.openai_api_key.is_none() {
            self.openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        }
        if self.anthropic_api_key.is_none() {
            self.anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        }
        if self.gemini_api_key.is_none() {
            self.gemini_api_key = std::env::var("GEMINI_API_KEY").ok().or_else(|| std::env::var("GOOGLE_API_KEY").ok());
        }
        if self.grok_api_key.is_none() {
            self.grok_api_key = std::env::var("GROK_API_KEY").ok().or_else(|| std::env::var("XAI_API_KEY").ok());
        }
        if self.deepseek_api_key.is_none() {
            self.deepseek_api_key = std::env::var("DEEPSEEK_API_KEY").ok();
        }
        if self.openrouter_api_key.is_none() {
            self.openrouter_api_key = std::env::var("OPENROUTER_API_KEY").ok();
        }
        if self.groq_api_key.is_none() {
            self.groq_api_key = std::env::var("GROQ_API_KEY").ok();
        }
        if self.cerebras_api_key.is_none() {
            self.cerebras_api_key = std::env::var("CEREBRAS_API_KEY").ok();
        }
        if self.sambanova_api_key.is_none() {
            self.sambanova_api_key = std::env::var("SAMBANOVA_API_KEY").ok();
        }
        if self.mistral_api_key.is_none() {
            self.mistral_api_key = std::env::var("MISTRAL_API_KEY").ok();
        }
        if self.together_api_key.is_none() {
            self.together_api_key = std::env::var("TOGETHER_API_KEY").ok();
        }
        if self.perplexity_api_key.is_none() {
            self.perplexity_api_key = std::env::var("PERPLEXITY_API_KEY").ok();
        }
        if self.cohere_api_key.is_none() {
            self.cohere_api_key = std::env::var("COHERE_API_KEY").ok();
        }
        if self.custom_base_url.is_none() {
            self.custom_base_url = std::env::var("ELYTRA_BASE_URL").ok();
        }
        if self.agent_max_steps.is_none() {
            self.agent_max_steps = std::env::var("ELYTRA_MAX_STEPS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok());
        }
        if self.agent_max_retries.is_none() {
            self.agent_max_retries = std::env::var("ELYTRA_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse::<u32>().ok());
        }
    }

    /// Belirtilen sağlayıcı için anahtarı döndür
    pub fn get_key_for(&self, provider: &str) -> Option<String> {
        match provider.to_lowercase().as_str() {
            "nous" => self.nous_api_key.clone(),
            "openai" => self.openai_api_key.clone(),
            "anthropic" => self.anthropic_api_key.clone(),
            "gemini" | "google" => self.gemini_api_key.clone(),
            "grok" | "xai" => self.grok_api_key.clone(),
            "deepseek" => self.deepseek_api_key.clone(),
            "openrouter" => self.openrouter_api_key.clone(),
            "groq" => self.groq_api_key.clone(),
            "cerebras" => self.cerebras_api_key.clone(),
            "sambanova" => self.sambanova_api_key.clone(),
            "mistral" => self.mistral_api_key.clone(),
            "together" => self.together_api_key.clone(),
            "perplexity" => self.perplexity_api_key.clone(),
            "cohere" => self.cohere_api_key.clone(),
            _ => None,
        }
    }

    /// Küresel config dosya yolu
    pub fn global_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("elytra").join("config.toml"))
    }

    /// Küresel config dosyasına kaydet
    pub fn save_global(&self) -> Result<()> {
        let path = Self::global_config_path()
            .context("Kullanıcı yapılandırma dizini bulunamadı")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
