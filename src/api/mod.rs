//! Elytra API — modele bağlantı katmanı
//!
//! v0.1: iskelet. Gerçek API çağrısı (Nous Inference) sonraki adımda.

use anyhow::Result;

/// Tek seferlik mesaj gönder (CLI modu: `elytra "selam"`)
pub async fn ask_once(prompt: &str, model: Option<&str>) -> Result<String> {
    let model = model.unwrap_or("stealth");
    // TODO: gerçek API çağrısı eklenecek
    Ok(format!(
        "[elytra · {model}] mesajını aldım: \"{prompt}\"\n\
         (API bağlantısı henüz kurulmadı — sıradaki adım)"
    ))
}
