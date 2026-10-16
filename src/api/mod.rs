//! Elytra API Katmanı — Çoklu Model Bağlantısı ve Streaming

pub mod client;
pub mod providers;

#[allow(unused_imports)]
pub use client::{ApiClient, ChatMessage, CompletionResponse, StreamEvent, ToolCall};
#[allow(unused_imports)]
pub use providers::{Model, Provider, Selection, PROVIDERS};

use anyhow::Result;
use crate::config::AppConfig;

/// Tek seferlik soru sor (CLI modu: `elytra "soru"` — Tam Araç Destekli)
pub async fn ask_once(prompt: &str, model_override: Option<&str>) -> Result<String> {
    let config = AppConfig::load();
    let mut selection = Selection::default();

    if let Some(m) = model_override {
        selection.set_by_name(m);
    } else if let Some(ref default_m) = config.default_model {
        selection.set_by_name(default_m);
    }

    let provider_id = selection.provider().id.to_string();
    let model_id = selection.model_id();

    let mut session = crate::agent::AgentSession::new(&provider_id, &model_id, config);
    let mut final_result = String::new();

    let res = session.run(prompt, |event| {
        match event {
            crate::agent::AgentEvent::Thought(th) => {
                eprintln!("\x1b[38;2;110;125;115m◆ Dusunce: {}\x1b[0m", th);
            }
            crate::agent::AgentEvent::ToolExecuting { name, args } => {
                eprintln!("\x1b[38;2;80;200;220m▶ Calistiriliyor: {}({})\x1b[0m", name, args);
            }
            crate::agent::AgentEvent::ToolFinished { name, result } => {
                let preview = crate::util::truncate_chars(&result, 140);
                eprintln!("\x1b[38;2;64;224;160m✓ {} tamamlandi: {}\x1b[0m", name, preview.trim());
            }
            crate::agent::AgentEvent::FinalAnswer(ans) => {
                final_result = ans;
            }
            crate::agent::AgentEvent::Retrying { attempt, reason } => {
                eprintln!(
                    "\x1b[38;2;230;160;120m[!] Gecici hata — yeniden deneniyor ({}. deneme): {}\x1b[0m",
                    attempt, reason
                );
            }
            _ => {}
        }
    }).await;

    match res {
        Ok(ans) => {
            if !final_result.is_empty() {
                Ok(final_result)
            } else {
                Ok(ans)
            }
        }
        Err(err) => Ok(format!(
            "◆ [elytra · {}/{}]\n\
             Hata veya API anahtari eksik: {}\n\n\
             Ipucu: API anahtarinizi kaydetmek icin: elytra config set {}_api_key <anahtar>",
            provider_id, model_id, err, provider_id
        )),
    }
}
