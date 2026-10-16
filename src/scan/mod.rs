//! Elytra Sağlayıcı Tarama Motoru (Scan Engine) — The Revolution
//!
//! Eski davranış: 16 sağlayıcı SIRAYLA ve her birine GERÇEK chat completion
//! ("Hi, reply with OK") gönderilirdi. Yavaş, kota yakıcı ve ilerlemesizdi.
//!
//! Yeni davranış (varsayılan):
//! - **Paralel tarama**: tüm sağlayıcılar eşzamanlı taranır (tokio JoinSet).
//! - **İki kademeli doğrulama**: önce ucuz bağlantı kontrolü (GET /models),
//!   isteğe bağlı olarak tam chat doğrulaması (Full).
//! - **Sıfır maliyet**: anahtarı olmayan sağlayıcıya hiç ağ isteği atılmaz.
//! - **Akışkan sonuçlar**: her sonuç hazır olduğunda callback ile akar.
//! - **Akıllı sınıflandırma**: Ok / NoKey / Unauthorized / RateLimited /
//!   Unreachable / HttpError ayrımı ve özet istatistikler.
//! - **Sert zaman aşımı**: her sağlayıcı için ayrı timeout (varsayılan 6s).

use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::api::client::ApiClient;
use crate::api::providers::{Provider, PROVIDERS};

/// Tarama kademesi
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// Sadece anahtar varlığı kontrolü (ağa hiç çıkmaz)
    Keys,
    /// Ucuz bağlantı kontrolü (GET /models) — varsayılan
    Connectivity,
    /// Bağlantı + minimal chat completion doğrulaması (kota harcar)
    Full,
}

impl ScanMode {
    pub fn label(&self) -> &'static str {
        match self {
            ScanMode::Keys => "Anahtar Kontrolu",
            ScanMode::Connectivity => "Baglanti Taramasi",
            ScanMode::Full => "Tam Dogrulama",
        }
    }
}

/// Tarama seçenekleri
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub mode: ScanMode,
    /// Sağlayıcı başına zaman aşımı
    pub per_provider_timeout: Duration,
    /// Aynı anda çalışacak maksimum tarama sayısı
    pub concurrency: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            mode: ScanMode::Connectivity,
            per_provider_timeout: Duration::from_secs(6),
            concurrency: 8,
        }
    }
}

/// Tarama sonucu sınıflandırması
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScanStatus {
    /// Bağlantı kuruldu ve kimlik doğrulandı
    Ok,
    /// API anahtarı tanımlı değil
    NoKey,
    /// Anahtar geçersiz / yetkisiz (401/403)
    Unauthorized,
    /// Kota / istek sınırı (429)
    RateLimited,
    /// Ağa çıkılamadı veya zaman aşımı
    Unreachable,
    /// Diğer HTTP hataları
    HttpError,
    /// Keys modunda anahtarı olmayan sağlayıcı (bilinçli atlama)
    Skipped,
}

impl ScanStatus {
    pub fn mark(&self) -> &'static str {
        match self {
            ScanStatus::Ok => "[OK]",
            ScanStatus::NoKey => "[KEY YOK]",
            ScanStatus::Unauthorized => "[YETKISIZ]",
            ScanStatus::RateLimited => "[KOTA]",
            ScanStatus::Unreachable => "[ERISILEMEZ]",
            ScanStatus::HttpError => "[HATA]",
            ScanStatus::Skipped => "[ATLANDI]",
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, ScanStatus::Ok)
    }
}

/// Tek sağlayıcı tarama sonucu
#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub provider_id: String,
    pub provider_name: String,
    pub status: ScanStatus,
    pub latency_ms: u128,
    pub detail: String,
}

impl ScanResult {
    /// Kullanıcıya gösterilecek tek satırlık rapor
    pub fn report_line(&self) -> String {
        format!(
            "{} {:<18} ({:>4}ms) : {}",
            self.status.mark(),
            self.provider_name,
            self.latency_ms,
            self.detail
        )
    }
}

/// Tarama özeti
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanSummary {
    pub total: usize,
    pub ok: usize,
    pub no_key: usize,
    pub unauthorized: usize,
    pub rate_limited: usize,
    pub unreachable: usize,
    pub http_error: usize,
    pub skipped: usize,
    pub fastest: Option<(String, u128)>,
    pub total_duration_ms: u128,
}

impl ScanSummary {
    /// İnsan okunabilir özet raporu
    pub fn format_report(&self, mode: ScanMode) -> String {
        let mut out = format!(
            "◆ TARAMA OZETI [{}] — {} saglayici, {:.1}s toplam sure\n",
            mode.label(),
            self.total,
            self.total_duration_ms as f64 / 1000.0
        );
        out.push_str(&format!(
            "  Calisan: {} | Anahtarsiz: {} | Yetkisiz: {} | Kota: {} | Erisilemez: {} | Hata: {} | Atlanan: {}\n",
            self.ok, self.no_key, self.unauthorized, self.rate_limited,
            self.unreachable, self.http_error, self.skipped
        ));
        if let Some((name, ms)) = &self.fastest {
            out.push_str(&format!("  En hizli saglayici: {} ({}ms)", name, ms));
        }
        out
    }
}

fn summarize(results: &[ScanResult], total_duration_ms: u128) -> ScanSummary {
    let mut s = ScanSummary {
        total: results.len(),
        total_duration_ms,
        ..Default::default()
    };
    let mut fastest: Option<(String, u128)> = None;
    for r in results {
        match r.status {
            ScanStatus::Ok => {
                s.ok += 1;
                if fastest.as_ref().map(|(_, m)| r.latency_ms < *m).unwrap_or(true) {
                    fastest = Some((r.provider_name.clone(), r.latency_ms));
                }
            }
            ScanStatus::NoKey => s.no_key += 1,
            ScanStatus::Unauthorized => s.unauthorized += 1,
            ScanStatus::RateLimited => s.rate_limited += 1,
            ScanStatus::Unreachable => s.unreachable += 1,
            ScanStatus::HttpError => s.http_error += 1,
            ScanStatus::Skipped => s.skipped += 1,
        }
    }
    s.fastest = fastest;
    s
}

/// Tüm sağlayıcıları paralel tara. Her sonuç hazır olduğunda `on_result` çağrılır.
pub async fn scan_all_providers<F>(
    client: &ApiClient,
    options: &ScanOptions,
    mut on_result: F,
) -> ScanSummary
where
    F: FnMut(&ScanResult),
{
    let start = Instant::now();
    let (tx, mut rx) = mpsc::channel::<ScanResult>(providers_count());

    let mut set: JoinSet<()> = JoinSet::new();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(options.concurrency.max(1)));

    for provider in PROVIDERS {
        let tx = tx.clone();
        let sem = semaphore.clone();
        let client = client.clone();
        let options = options.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore kapali");
            let res = scan_single(&client, provider, &options).await;
            let _ = tx.send(res).await;
        });
    }
    drop(tx);

    let mut results = Vec::with_capacity(providers_count());
    while let Some(r) = rx.recv().await {
        on_result(&r);
        results.push(r);
    }
    while set.join_next().await.is_some() {}

    summarize(&results, start.elapsed().as_millis())
}

/// Tek bir sağlayıcıyı tara
pub async fn scan_single(client: &ApiClient, provider: &Provider, options: &ScanOptions) -> ScanResult {
    let base = ScanResult {
        provider_id: provider.id.to_string(),
        provider_name: provider.name.to_string(),
        status: ScanStatus::Skipped,
        latency_ms: 0,
        detail: String::new(),
    };

    // 0. Çevrimdışı dahili motor — her zaman anında OK
    if provider.id == "demo" || provider.id == "offline" {
        let start = Instant::now();
        let _ = crate::api::client::generate_demo_response("ping");
        return ScanResult {
            status: ScanStatus::Ok,
            latency_ms: start.elapsed().as_millis().max(1),
            detail: "Dahili yerel motor (sifir ag maliyeti)".into(),
            ..base
        };
    }

    // 1. Anahtar kontrolü — anahtarı olmayan sağlayıcıya asla ağa çıkmayız
    let key = client.config_key_for(provider.id);
    let keyless_ok = provider.id == "local" || provider.id == "nous";
    if key.is_none() && !keyless_ok {
        return ScanResult {
            status: match options.mode {
                ScanMode::Keys => ScanStatus::Skipped,
                _ => ScanStatus::NoKey,
            },
            detail: format!("API anahtari eksik: elytra config set {}_api_key <anahtar>", provider.id),
            ..base
        };
    }

    match options.mode {
        ScanMode::Keys => ScanResult {
            status: ScanStatus::Ok,
            latency_ms: 0,
            detail: "Anahtar tanimli".into(),
            ..base
        },
        ScanMode::Connectivity => {
            let start = Instant::now();
            let probe = tokio::time::timeout(
                options.per_provider_timeout,
                connectivity_probe(client, provider, key.as_deref()),
            )
            .await;
            let latency = start.elapsed().as_millis();
            match probe {
                Err(_) => ScanResult {
                    status: ScanStatus::Unreachable,
                    latency_ms: latency,
                    detail: format!("Zaman asimi ({}ms)", options.per_provider_timeout.as_millis()),
                    ..base
                },
                Ok(Err((status, detail))) => ScanResult { status, latency_ms: latency, detail, ..base },
                Ok(Ok(detail)) => ScanResult {
                    status: ScanStatus::Ok,
                    latency_ms: latency,
                    detail,
                    ..base
                },
            }
        }
        ScanMode::Full => {
            // Önce ucuz bağlantı kontrolü; başarısızsa token harcamayız
            let start = Instant::now();
            let conn = tokio::time::timeout(
                options.per_provider_timeout,
                connectivity_probe(client, provider, key.as_deref()),
            )
            .await;
            let conn_latency = start.elapsed().as_millis();
            match conn {
                Ok(Ok(detail)) => {
                    // Bağlantı tamam — şimdi minimal chat doğrulaması
                    let model = provider.models.first().map(|m| m.id).unwrap_or("default");
                    let msgs = vec![crate::api::ChatMessage::user("Hi, reply with OK")];
                    let full = tokio::time::timeout(
                        options.per_provider_timeout,
                        client.chat_completion(provider.id, model, &msgs, None),
                    )
                    .await;
                    let total = start.elapsed().as_millis();
                    match full {
                        Err(_) => ScanResult {
                            status: ScanStatus::Unreachable,
                            latency_ms: total,
                            detail: format!("Chat zaman asimi ({})", model),
                            ..base
                        },
                        Ok(Err(e)) => classify_error(&e, total, &base),
                        Ok(Ok(resp)) => ScanResult {
                            status: ScanStatus::Ok,
                            latency_ms: total,
                            detail: format!(
                                "{} — chat yaniti: \"{}\"",
                                detail,
                                crate::util::truncate_chars(resp.content.trim(), 40)
                            ),
                            ..base
                        },
                    }
                }
                Ok(Err((status, detail))) => {
                    let _ = conn_latency;
                    ScanResult { status, latency_ms: conn_latency, detail, ..base }
                }
                Err(_) => ScanResult {
                    status: ScanStatus::Unreachable,
                    latency_ms: conn_latency,
                    detail: format!("Zaman asimi ({}ms)", options.per_provider_timeout.as_millis()),
                    ..base
                },
            }
        }
    }
}

/// Sağlayıcının base_url'inden /models uç noktasını türet
fn models_endpoint(provider: &Provider) -> String {
    let base = provider.base_url.trim_end_matches('/');
    let base = base
        .strip_suffix("/chat/completions")
        .or_else(|| base.strip_suffix("/messages"))
        .unwrap_or(base);
    format!("{base}/models")
}

/// Ucuz bağlantı kontrolü — GET /models (OpenAI uyumlu sağlayıcılar)
/// Başarılıysa Ok(detail), değilse sınıflandırılmış hata döner.
async fn connectivity_probe(
    client: &ApiClient,
    provider: &Provider,
    key: Option<&str>,
) -> Result<String, (ScanStatus, String)> {
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    let url = models_endpoint(provider);
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if let Some(k) = key
        && !k.is_empty() {
            if provider.id == "anthropic" {
                headers.insert(
                    "x-api-key",
                    HeaderValue::from_str(k).map_err(|e| (ScanStatus::HttpError, format!("Gecersiz anahtar: {e}")))?,
                );
                headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
            } else if provider.id == "google" || provider.id == "gemini" {
                let _ = HeaderValue::from_str(k).map(|v| {
                    headers.insert("x-goog-api-key", v);
                });
            } else {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {k}"))
                        .map_err(|e| (ScanStatus::HttpError, format!("Gecersiz anahtar: {e}")))?,
                );
            }
        }

    let http = client.http_client();
    let resp = http
        .get(&url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| (ScanStatus::Unreachable, format!("Ag hatasi: {e}")))?;

    let status = resp.status();
    if status.is_success() {
        // Model listesinden bir isim çekmeye çalış (kanıt olarak)
        let detail = match resp.json::<serde_json::Value>().await {
            Ok(v) => {
                let count = v
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|a| a.len())
                    .or_else(|| v.get("models").and_then(|d| d.as_array()).map(|a| a.len()))
                    .unwrap_or(0);
                if count > 0 {
                    format!("Baglandi — {count} model listelendi")
                } else {
                    "Baglandi — /models erisilebilir".to_string()
                }
            }
            Err(_) => "Baglandi — /models erisilebilir".to_string(),
        };
        return Ok(detail);
    }

    let code = status.as_u16();
    let raw = resp.text().await.unwrap_or_default();
    let detail = crate::api::client::format_api_error(code, &raw);
    let scan_status = match code {
        401 | 403 => ScanStatus::Unauthorized,
        429 => ScanStatus::RateLimited,
        404 => ScanStatus::Ok, // Sunucu erişilebilir; /models uç noktası desteklenmiyor
        _ => ScanStatus::HttpError,
    };
    let msg = if code == 404 {
        "Sunucu erisilebilir (/models desteklenmiyor)".to_string()
    } else {
        detail
    };
    Err((scan_status, msg))
}

/// anyhow hata mesajından tarama durumu çıkar
pub fn classify_error(err: &anyhow::Error, latency_ms: u128, base: &ScanResult) -> ScanResult {
    let msg = err.to_string();
    let status = if msg.contains("429") || msg.to_lowercase().contains("rate limit") || msg.to_lowercase().contains("kota") {
        ScanStatus::RateLimited
    } else if msg.contains("401") || msg.contains("403") || msg.to_lowercase().contains("anahtar") {
        ScanStatus::Unauthorized
    } else if msg.to_lowercase().contains("ag") || msg.to_lowercase().contains("baglanti") || msg.contains("timeout") || msg.contains("zaman") {
        ScanStatus::Unreachable
    } else {
        ScanStatus::HttpError
    };
    ScanResult {
        status,
        latency_ms,
        detail: msg,
        ..base.clone()
    }
}

fn providers_count() -> usize {
    PROVIDERS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_endpoint_derivation() {
        let openai = PROVIDERS.iter().find(|p| p.id == "openai").unwrap();
        assert_eq!(models_endpoint(openai), "https://api.openai.com/v1/models");

        let anthropic = PROVIDERS.iter().find(|p| p.id == "anthropic").unwrap();
        assert_eq!(models_endpoint(anthropic), "https://api.anthropic.com/v1/models");

        let openrouter = PROVIDERS.iter().find(|p| p.id == "openrouter").unwrap();
        assert_eq!(models_endpoint(openrouter), "https://openrouter.ai/api/v1/models");
    }

    #[test]
    fn katalogda_yerel_saglayici_yok() {
        // Proje kuralı: Ollama / yerel motor sağlayıcısı BULUNMAMALI.
        for p in PROVIDERS {
            let hay = format!("{} {} {}", p.id, p.name, p.base_url).to_lowercase();
            assert!(
                !hay.contains("ollama") && !hay.contains("local://") && !hay.contains("localhost"),
                "yerel sağlayıcı bulundu: {}",
                p.id
            );
        }
    }

    #[test]
    fn her_saglayicinin_models_ucu_gecerli() {
        for p in PROVIDERS {
            let u = models_endpoint(p);
            assert!(
                u.starts_with("https://") && u.ends_with("/models"),
                "{} için geçersiz uç: {}",
                p.id,
                u
            );
        }
    }

    #[tokio::test]
    async fn scan_keyless_provider_no_network() {
        let config = crate::config::AppConfig::default(); // tüm anahtarlar boş
        let client = ApiClient::new(config);
        let openai = PROVIDERS.iter().find(|p| p.id == "openai").unwrap();
        let res = scan_single(&client, openai, &ScanOptions::default()).await;
        assert_eq!(res.status, ScanStatus::NoKey, "Anahtarsız sağlayıcı NoKey dönmeli");
    }

    #[test]
    fn scan_all_with_callbacks() {
        // Senkron test: özet doğru sayıları üretiyor mu?
        let results = vec![
            ScanResult {
                provider_id: "a".into(),
                provider_name: "A".into(),
                status: ScanStatus::Ok,
                latency_ms: 50,
                detail: String::new(),
            },
            ScanResult {
                provider_id: "b".into(),
                provider_name: "B".into(),
                status: ScanStatus::NoKey,
                latency_ms: 0,
                detail: String::new(),
            },
            ScanResult {
                provider_id: "c".into(),
                provider_name: "C".into(),
                status: ScanStatus::Unauthorized,
                latency_ms: 120,
                detail: String::new(),
            },
        ];
        let s = summarize(&results, 300);
        assert_eq!(s.total, 3);
        assert_eq!(s.ok, 1);
        assert_eq!(s.no_key, 1);
        assert_eq!(s.unauthorized, 1);
        assert_eq!(s.fastest.as_ref().unwrap().1, 50);
        let report = s.format_report(ScanMode::Connectivity);
        assert!(report.contains("TARAMA OZETI"));
        assert!(report.contains("A (50ms)") || report.contains("En hizli"));
    }

    #[test]
    fn keys_mode_skips_keyless() {
        let config = crate::config::AppConfig::default();
        let client = ApiClient::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let openai = PROVIDERS.iter().find(|p| p.id == "openai").unwrap();
        let res = rt.block_on(scan_single(
            &client,
            openai,
            &ScanOptions {
                mode: ScanMode::Keys,
                ..Default::default()
            },
        ));
        assert_eq!(res.status, ScanStatus::Skipped);
    }
}
