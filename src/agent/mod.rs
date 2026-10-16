//! Elytra Otonom Ajan (Agent) Katmanı — ReAct Döngüsü
//!
//! Modelin araçları (read_file, edit_file, bash_exec vb.) ardışık olarak
//! çağırıp görevleri otonom şekilde çözdüğü yürütme motoru.
//!
//! Sürdürülebilirlik Güvenceleri:
//! - Maksimum adım sınırı (varsayılan: 12 adım, config ile ayarlanabilir).
//! - 429/5xx ve ağ hatalarında üssel geri çekilme (exponential backoff + jitter) ile yeniden deneme.
//! - Context bütçesi: araç çıktıları biriktikçe eski çıktılar kırpılır (API 400 ve maliyet patlaması koruması).
//! - Araç çıktıları bellek taşmasına karşı filtrelenir ve kırpılır.

use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

use crate::api::{ApiClient, ChatMessage};
use crate::config::AppConfig;
use crate::tools::{execute_tool, get_available_tools, ToolDefinition};

/// Ajan olayları (TUI veya CLI çıktısı için)
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thought(String),
    ToolExecuting { name: String, args: String },
    ToolFinished { name: String, result: String },
    FinalAnswer(String),
    StepLimitReached,
    /// Geçici hata sonrası yeniden deneme (kota/5xx/ağ)
    Retrying { attempt: u32, reason: String },
    Error(String),
}

/// Ajan Oturum Ayarları
#[derive(Debug, Clone)]
pub struct AgentOptions {
    /// Maksimum ReAct adım sayısı
    pub max_steps: usize,
    /// Geçici hatalarda maksimum yeniden deneme sayısı
    pub max_retries: u32,
    /// Toplam context bütçesi (karakter). Aşılırsa eski araç çıktıları kırpılır.
    pub context_budget: usize,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            max_steps: 12,
            max_retries: 3,
            context_budget: 60_000,
        }
    }
}

/// Ajan Oturumu
pub struct AgentSession {
    client: ApiClient,
    provider_id: String,
    model_id: String,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    options: AgentOptions,
}

impl AgentSession {
    pub fn new(provider_id: &str, model_id: &str, config: AppConfig) -> Self {
        // Config'ten sürdürülebilirlik ayarlarını oku
        let options = AgentOptions {
            max_steps: config.agent_max_steps.unwrap_or(12).clamp(2, 100),
            max_retries: config.agent_max_retries.unwrap_or(3).clamp(0, 10),
            ..Default::default()
        };
        Self::with_options(provider_id, model_id, config, options)
    }

    /// Özel ayarlarla ajan oturumu
    pub fn with_options(
        provider_id: &str,
        model_id: &str,
        config: AppConfig,
        options: AgentOptions,
    ) -> Self {
        let tools = get_available_tools();
        let client = ApiClient::new(config);

        let system_prompt = "Sen Elytra Ajanisin (Autonomous Rust CLI Coding Agent). Gorevin yazilim gelistirme, \
             hata ayiklama, mimari analiz ve terminal gorevlerini en yuksek dogrulukla cozmek.\n\n\
             Kullanabilecegin 15 Dahili Arac:\n\
             - `read_file`: Dosya okuma (satir araligi destekli)\n\
             - `write_file`: Yeni dosya olusturma / tam yazma\n\
             - `edit_file`: Dosyada cerrahi hedef blok degistirme\n\
             - `apply_patch`: Unified diff veya degisiklik yamasi uygulama\n\
             - `list_dir`: Dizin iceriklerini listeleme (akilli filtreli)\n\
             - `grep_search`: Kod icinde regex/metin arama (glob filtresi, baglam satirlari)\n\
             - `file_search`: Glob desenli dosya arama ('*.rs')\n\
             - `git_status`: Git dal ve degisiklik durumu\n\
             - `git_diff`: Git farklarini inceleme\n\
             - `diagnostics`: Proje derleme ve linter hatalarini kontrol etme (cargo check, npm lint)\n\
             - `bash_exec`: Terminal komutu calistirma (30s sert zaman asimi korumali)\n\
             - `web_fetch`: URL icerigi cekme (HTTP GET)\n\
             - `web_search`: Canli web aramasi yapma (DuckDuckGo motoru)\n\
             - `task_tracker`: Cok adimli gorev ve checklist yonetimi (add, list, complete, clear)\n\
             - `repo_map`: Kod tabani mimarisi, proje tipi ve dosya haritasini cikarir\n\n\
             Operasyon Kurallari:\n\
             1. Dosyada degisiklik yapmadan once daima `read_file` ile oku.\n\
             2. Projeyi incelemek icin once `repo_map` veya `list_dir` calistir.\n\
             3. Dosya duzenleme sonrasi `diagnostics` veya `bash_exec` ile projeyi kontrol et.\n\
             4. KotASINA (429) takilirsan baska bir araci dene veya bekleyerek devam et; ayni hatayi tekrar etme.\n\
             5. Asla emoji kullanma. Turkce, net ve teknik acidan kusursuz aciklamalar yap.".to_string();

        let messages = vec![ChatMessage::system(system_prompt)];

        Self {
            client,
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            messages,
            tools,
            options,
        }
    }

    /// Bir görevi ReAct döngüsüyle baştan sona yürüt
    pub async fn run<F>(&mut self, task: &str, mut on_event: F) -> Result<String>
    where
        F: FnMut(AgentEvent),
    {
        self.messages.push(ChatMessage::user(task));

        let mut step = 0;
        let mut final_answer = String::new();

        while step < self.options.max_steps {
            step += 1;

            // Context bütçesi kontrolü (eski araç çıktılarını kırp)
            compact_messages(&mut self.messages, self.options.context_budget);

            let response = match self
                .chat_with_retry(&mut on_event)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let err_msg = format!("Model API Hatasi: {e}");
                    on_event(AgentEvent::Error(err_msg.clone()));
                    return Err(anyhow::anyhow!(err_msg));
                }
            };

            // Araç çağrısı var mı?
            if response.tool_calls.is_empty() {
                // Model görevi tamamladı veya doğrudan yanıt verdi
                final_answer = response.content.clone();
                self.messages.push(ChatMessage::assistant(response.content));
                on_event(AgentEvent::FinalAnswer(final_answer.clone()));
                break;
            }

            // Eğer model araç çağırmadan önce düşünce/açıklama metni verdiyse
            if !response.content.is_empty() {
                on_event(AgentEvent::Thought(response.content.clone()));
            }

            // Asistan yanıtını geçmişe ekle (araç çağrılarıyla birlikte)
            let mut assistant_msg = ChatMessage::assistant(response.content.clone());
            assistant_msg.tool_calls = Some(response.tool_calls.clone());
            self.messages.push(assistant_msg);

            // Tüm araç çağrılarını sırayla çalıştır
            for tool_call in &response.tool_calls {
                let name = &tool_call.function.name;
                let args_str = &tool_call.function.arguments;

                on_event(AgentEvent::ToolExecuting {
                    name: name.clone(),
                    args: args_str.clone(),
                });

                let parsed_args: Value = serde_json::from_str(args_str)
                    .unwrap_or_else(|_| serde_json::json!({}));

                let result = match execute_tool(name, &parsed_args).await {
                    Ok(out) => out,
                    Err(e) => format!("Araç çalıştırma hatası: {e}"),
                };

                on_event(AgentEvent::ToolFinished {
                    name: name.clone(),
                    result: result.clone(),
                });

                // Sonucu mesaja ekle
                self.messages.push(ChatMessage::tool(tool_call.id.clone(), result));
            }
        }

        if step >= self.options.max_steps && final_answer.is_empty() {
            on_event(AgentEvent::StepLimitReached);
            final_answer = format!(
                "Maksimum adım sınırına ({}) ulaşıldı. Görev kısmen tamamlanmış olabilir.",
                self.options.max_steps
            );
        }

        Ok(final_answer)
    }

    /// Geçici hatalarda üssel geri çekilmeli chat tamamlama
    async fn chat_with_retry<F>(
        &self,
        on_event: &mut F,
    ) -> Result<crate::api::CompletionResponse>
    where
        F: FnMut(AgentEvent),
    {
        let mut attempt: u32 = 0;
        loop {
            let result = self
                .client
                .chat_completion(
                    &self.provider_id,
                    &self.model_id,
                    &self.messages,
                    Some(&self.tools),
                )
                .await;

            match result {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if attempt >= self.options.max_retries || !is_retryable_error(&e) {
                        return Err(e);
                    }
                    let wait = backoff_duration(attempt);
                    on_event(AgentEvent::Retrying {
                        attempt: attempt + 1,
                        reason: e.to_string(),
                    });
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
            }
        }
    }
}

/// Hatanın yeniden denemeye değer olup olmadığı (kota, 5xx, ağ, zaman aşımı)
pub fn is_retryable_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    if msg.contains("429") || msg.contains("kota") || msg.contains("rate limit") {
        return true;
    }
    // 5xx sunucu hataları
    if msg.contains("500") || msg.contains("502") || msg.contains("503") || msg.contains("504")
        || msg.contains("asiri yuk") || msg.contains("overloaded")
    {
        return true;
    }
    // Ağ ve zaman aşımı hataları
    if msg.contains("ag baglanti") || msg.contains("baglanti hatasi")
        || msg.contains("timeout") || msg.contains("zaman asimi")
        || msg.contains("akis kesildi") || msg.contains("error sending request")
    {
        return true;
    }
    false
}

/// Üssel geri çekilme süresi: 400ms * 2^attempt, 5s tavan, attempt bazlı deterministic jitter
pub fn backoff_duration(attempt: u32) -> Duration {
    let base_ms: u64 = 400u64.saturating_mul(1u64 << attempt.min(6));
    let capped = base_ms.min(5_000);
    // Deterministik jitter: attempt'in küçük bir hash'i (test edilebilirlik için)
    let jitter = (attempt.wrapping_mul(2654435761) % 250) as u64;
    Duration::from_millis(capped + jitter)
}

/// Context bütçesi aşımında eski araç çıktılarını kırp (sistem mesajı ve son 4 mesaj korunur)
pub fn compact_messages(messages: &mut [ChatMessage], budget: usize) {
    let total: usize = messages.iter().map(|m| {
        m.content.as_ref().map(|c| c.chars().count()).unwrap_or(0)
            + m.role.chars().count()
    }).sum();

    if total <= budget {
        return;
    }

    // En eski araç çıktılarından başlayarak kırp
    let protect_tail = 4; // son 4 mesaja dokunma
    let cut_from = messages.len().saturating_sub(protect_tail);
    let placeholder = "[Onceki arac ciktisi context tuketimi azaltmak icin kirpildi]".to_string();

    for (idx, msg) in messages.iter_mut().enumerate() {
        if idx == 0 || idx >= cut_from {
            continue; // sistem mesajı ve kuyruk korunsun
        }
        if msg.role == "tool"
            && let Some(content) = &mut msg.content
                && content.chars().count() > 200 {
                    *content = placeholder.clone();
                }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        let d0 = backoff_duration(0);
        let d1 = backoff_duration(1);
        let d2 = backoff_duration(2);
        let d_big = backoff_duration(50);
        assert!(d0.as_millis() >= 400 && d0.as_millis() < 700);
        assert!(d1.as_millis() >= 800 && d1.as_millis() < 1100);
        assert!(d2.as_millis() >= 1600 && d2.as_millis() < 1900);
        assert!(d_big.as_millis() >= 5000 && d_big.as_millis() <= 5250, "5s tavan aşilmamali");
    }

    #[test]
    fn retryable_error_classification() {
        // Kota ve 5xx yeniden denenmeli
        for msg in [
            "API Hatasi [429]: rate limit",
            "API Hatasi [503]: asiri yuk",
            "Ag baglanti hatasi (https://x): timeout",
            "Istek veya kota siniri asildi",
            "error sending request",
        ] {
            assert!(is_retryable_error(&anyhow::anyhow!(msg)), "'{msg}' yeniden denenebilir olmali");
        }
        // Kalıcı hatalar denenmemeli
        for msg in [
            "API anahtari gecersiz veya yetkisiz (401)",
            "Gecersiz API yaniti: choices alani bos",
            "Tanimsiz saglayici: 'xxx'",
        ] {
            assert!(!is_retryable_error(&anyhow::anyhow!(msg)), "'{msg}' kalici hata olmali");
        }
    }

    #[test]
    fn compact_messages_trims_old_tool_outputs() {
        let mut messages = vec![
            ChatMessage::system("Sistem prompts"),
            ChatMessage::user("Gorev"),
        ];
        // 3 büyük eski araç çıktısı
        for i in 0..3 {
            messages.push(ChatMessage::assistant(format!("Dusunce {i}")));
            messages.push(ChatMessage::tool(
                format!("call_{i}"),
                "x".repeat(5_000),
            ));
        }
        // Son mesajlar korunmali
        messages.push(ChatMessage::assistant("Son dusunce"));
        messages.push(ChatMessage::tool("call_final", "y".repeat(3_000)));

        let original_total: usize = messages
            .iter()
            .map(|m| m.content.as_ref().map(|c| c.chars().count()).unwrap_or(0))
            .sum();
        assert!(original_total > 15_000);

        compact_messages(&mut messages, 10_000);

        // İlk 2 araç çıktısı kırpilmis olmali (kuyruk koruması kadar)
        for idx in [3usize, 5] {
            let content = messages[idx].content.as_ref().unwrap();
            assert!(
                content.contains("kirpildi"),
                "Mesaj {idx} kirpilmis olmali: {}",
                crate::util::truncate_chars(content, 60)
            );
        }
        // Kuyruk korumasi: son 4 mesaja dokunulmaz (idx 7 hala tam içerik)
        assert_eq!(
            messages[7].content.as_ref().unwrap().len(),
            5_000,
            "Kuyruktaki araç çiktisi korunmali"
        );
        // Sistem mesajı ve son mesajlar korunmuş olmali
        assert!(messages[0].content.as_ref().unwrap().contains("Sistem"));
        let last = messages.last().unwrap();
        assert!(last.content.as_ref().unwrap().contains('y'), "Son mesaj korunmali");
    }

    #[test]
    fn compact_messages_noop_under_budget() {
        let mut messages = vec![
            ChatMessage::system("kisa"),
            ChatMessage::user("kisa2"),
            ChatMessage::tool("c", "cikti"),
        ];
        let before = messages.clone();
        compact_messages(&mut messages, 10_000);
        assert_eq!(messages.len(), before.len());
        assert_eq!(messages[2].content.as_deref(), Some("cikti"));
    }

    #[test]
    fn agent_options_defaults_sane() {
        let o = AgentOptions::default();
        assert!(o.max_steps >= 2 && o.max_steps <= 100);
        assert!(o.max_retries <= 10);
        assert!(o.context_budget >= 10_000);
    }
}
