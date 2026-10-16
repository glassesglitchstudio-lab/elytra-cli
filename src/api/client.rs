//! Elytra Evrensel Model İstemcisi
//!
//! OpenAI, OpenRouter, Groq, DeepSeek, Google Gemini, Cerebras, SambaNova,
//! Mistral, Anthropic, Together, xAI, Perplexity, Cohere ve yerel Ollama/vLLM için
//! yüksek performanslı, asenkron HTTP istemcisi.
//! Ayrıca sıfır-konfigürasyon için dahili akıllı "Elytra Çevrimdışı" motorunu içerir.

use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;

use crate::config::AppConfig;
use crate::tools::ToolDefinition;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(msg: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(msg.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(msg: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(msg.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(msg: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: Some(msg.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunctionCall,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_content: Option<Value>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            call_type: "function".into(),
            function: ToolFunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
            extra_content: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

/// Akış (Streaming) etkinlikleri
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum StreamEvent {
    ContentDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        args_delta: String,
    },
    Done,
    Error(String),
}

/// Sağlayıcı Test Sonucu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTestResult {
    pub provider_id: String,
    pub provider_name: String,
    pub success: bool,
    pub latency_ms: u128,
    pub message: String,
}

/// Model İstemcisi
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    config: AppConfig,
}

impl ApiClient {
    pub fn new(config: AppConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { http, config }
    }

    /// Paylaşılan HTTP istemcisi (bağlantı havuzu yeniden kullanımı için)
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Sağlayıcı için yapılandırılmış anahtarı döner (tarama motoru için)
    pub fn config_key_for(&self, provider_id: &str) -> Option<String> {
        self.config.get_key_for(provider_id)
    }

    /// Sağlayıcıyı test et (ping / bağlantı ve yanıt kontrolü)
    pub async fn test_provider(&self, provider_id: &str) -> ProviderTestResult {
        let prov = crate::api::providers::PROVIDERS
            .iter()
            .find(|p| p.id == provider_id);

        let (name, p_id) = match prov {
            Some(p) => (p.name.to_string(), p.id.to_string()),
            None => (provider_id.to_string(), provider_id.to_string()),
        };

        // 1. Çevrimdışı motor testi
        if p_id == "demo" || p_id == "offline" {
            let start = Instant::now();
            let _ = generate_demo_response("ping");
            return ProviderTestResult {
                provider_id: p_id,
                provider_name: name,
                success: true,
                latency_ms: start.elapsed().as_millis().max(1),
                message: "OK (Dahili Yerel Motor)".into(),
            };
        }

        // 2. API anahtarı kontrolü
        let key = self.config.get_key_for(&p_id);
        if p_id != "local" && p_id != "nous" && key.is_none() {
            return ProviderTestResult {
                provider_id: p_id.clone(),
                provider_name: name,
                success: false,
                latency_ms: 0,
                message: format!("API anahtari eksik (/key {} <anahtar>)", p_id),
            };
        }

        // 3. Gerçek endpoint testi
        let start = Instant::now();
        let default_model = prov.and_then(|p| p.models.first().map(|m| m.id)).unwrap_or("default");

        let test_msg = vec![ChatMessage::user("Hi, reply with OK")];
        let result = self.chat_completion(&p_id, default_model, &test_msg, None).await;
        let elapsed = start.elapsed().as_millis();

        match result {
            Ok(resp) => {
                let preview = crate::util::truncate_chars(resp.content.trim(), 30);
                ProviderTestResult {
                    provider_id: p_id,
                    provider_name: name,
                    success: true,
                    latency_ms: elapsed,
                    message: format!("OK: \"{}\"", preview),
                }
            }
            Err(e) => ProviderTestResult {
                provider_id: p_id,
                provider_name: name,
                success: false,
                latency_ms: elapsed,
                message: format!("{e}"),
            },
        }
    }

    /// Tüm kayıtlı veya anahtarı bulunan sağlayıcıları test et (eski davranış — CLI artık scan motorunu kullanır)
    #[allow(dead_code)]
    pub async fn test_all_providers(&self) -> Vec<ProviderTestResult> {
        let mut results = Vec::new();
        for p in crate::api::providers::PROVIDERS {
            let res = self.test_provider(p.id).await;
            results.push(res);
        }
        results
    }

    /// Tek seferlik senkron yanıt üret (araç destekli)
    pub async fn chat_completion(
        &self,
        provider_id: &str,
        model_id: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<CompletionResponse> {
        // 1. Çevrimdışı dahili motor (Gerçek Araç Çağrısı Destekli)
        if provider_id == "demo" || provider_id == "offline" {
            let resp = generate_demo_completion(messages);
            return Ok(resp);
        }

        // 2. Anthropic Resmi Messages API (Araç Destekli)
        if provider_id == "anthropic" {
            return self.complete_anthropic(model_id, messages, tools).await;
        }

        let (url, api_key) = self.resolve_endpoint(provider_id)?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(ref key) = api_key
            && !key.is_empty() {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {key}"))
                        .map_err(|e| anyhow!("Gecersiz API anahtari: {e}"))?,
                );
                if (provider_id == "google" || provider_id == "gemini")
                    && let Ok(v) = HeaderValue::from_str(key) {
                        headers.insert("x-goog-api-key", v);
                    }
            }

        if provider_id == "openrouter" {
            headers.insert("HTTP-Referer", HeaderValue::from_static("https://github.com/elytra-cli"));
            headers.insert("X-Title", HeaderValue::from_static("Elytra"));
        }

        let effective_model_id = if provider_id == "google" || provider_id == "gemini" {
            match model_id.to_lowercase().as_str() {
                "gemini-1.5-pro" | "gemini-2.0-pro" | "gemini-2.5-pro" | "gemini-pro" => "gemini-3.5-flash-lite",
                "gemini-1.5-flash" | "gemini-2.0-flash" | "gemini-2.5-flash" | "gemini-flash" | "gemini-flash-latest" => "gemini-3.5-flash-lite",
                "gemini-2.5-flash-lite" => "gemini-3.5-flash-lite",
                _ => model_id,
            }
        } else {
            model_id
        };

        let mut body = json!({
            "model": effective_model_id,
            "messages": messages,
            "temperature": self.config.temperature.unwrap_or(0.7),
            "max_tokens": self.config.max_tokens.unwrap_or(2048),
        });

        if let Some(tool_list) = tools
            && !tool_list.is_empty() {
                let formatted_tools: Vec<Value> = tool_list
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters,
                            }
                        })
                    })
                    .collect();
                body["tools"] = json!(formatted_tools);
            }

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Ag baglanti hatasi ({}): {}", url, e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let raw_text = resp.text().await.unwrap_or_default();
            let err_summary = format_api_error(status.as_u16(), &raw_text);
            return Err(anyhow!("API Hatasi [{}]: {}", status, err_summary));
        }

        let json_resp: Value = resp.json().await
            .map_err(|e| anyhow!("JSON ayristirma hatasi: {e}"))?;

        let choice = json_resp
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| anyhow!("Gecersiz API yaniti: choices alani bos"))?;

        let message = choice.get("message").ok_or_else(|| anyhow!("Mesaj nesnesi bulunamadi"))?;
        let mut content = message.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();

        let mut tool_calls = Vec::new();
        if let Some(tc_array) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tc_array {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("call_default").to_string();
                let call_type = tc.get("type").and_then(|t| t.as_str()).unwrap_or("function").to_string();
                let func = tc.get("function").ok_or_else(|| anyhow!("Function alani eksik"))?;
                let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let arguments = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}").to_string();

                let extra_content = tc.get("extra_content").cloned();
                tool_calls.push(ToolCall {
                    id,
                    call_type,
                    function: ToolFunctionCall { name, arguments },
                    extra_content,
                });
            }
        }

        // Eğer API seviyesinde tool_calls dönmediyse, modelin metin içine yazdığı çağrıları çözümle
        if tool_calls.is_empty() {
            let (clean_text, extracted) = extract_tool_calls_from_text(&content);
            if !extracted.is_empty() {
                tool_calls = extracted;
                content = clean_text;
            }
        }

        Ok(CompletionResponse { content, tool_calls })
    }

    /// Anthropic Doğrudan Messages API Çağrısı (Tam Araç Destekli)
    async fn complete_anthropic(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<CompletionResponse> {
        let key = self.config.get_key_for("anthropic")
            .ok_or_else(|| anyhow!("Anthropic API anahtari ayarlanmamis (/key anthropic <anahtar>)"))?;

        let url = "https://api.anthropic.com/v1/messages";
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", HeaderValue::from_str(&key)?);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let mut system_prompt = String::new();
        let mut anthropic_msgs = Vec::new();

        for m in messages {
            if m.role == "system" {
                if let Some(ref c) = m.content {
                    system_prompt.push_str(c);
                    system_prompt.push('\n');
                }
            } else if m.role == "tool" {
                anthropic_msgs.push(json!({
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                            "content": m.content.clone().unwrap_or_default()
                        }
                    ]
                }));
            } else if m.role == "assistant" {
                let mut parts = Vec::new();
                if let Some(ref c) = m.content
                    && !c.is_empty() {
                        parts.push(json!({ "type": "text", "text": c }));
                    }
                if let Some(ref tcs) = m.tool_calls {
                    for tc in tcs {
                        let parsed_input: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));
                        parts.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": parsed_input
                        }));
                    }
                }
                if parts.is_empty() {
                    parts.push(json!({ "type": "text", "text": "" }));
                }
                anthropic_msgs.push(json!({
                    "role": "assistant",
                    "content": parts
                }));
            } else {
                anthropic_msgs.push(json!({
                    "role": "user",
                    "content": m.content.clone().unwrap_or_default()
                }));
            }
        }

        let mut body = json!({
            "model": model_id,
            "messages": anthropic_msgs,
            "max_tokens": 4096,
        });

        if !system_prompt.trim().is_empty() {
            body["system"] = json!(system_prompt.trim());
        }

        if let Some(tool_list) = tools
            && !tool_list.is_empty() {
                let ant_tools: Vec<Value> = tool_list.iter().map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters
                    })
                }).collect();
                body["tools"] = json!(ant_tools);
            }

        let resp = self.http.post(url).headers(headers).json(&body).send().await
            .map_err(|e| anyhow!("Anthropic ag hatasi: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API Hatasi [{}]: {}", status, format_api_error(status.as_u16(), &raw)));
        }

        let json_resp: Value = resp.json().await?;
        let mut text = String::new();
        let mut tool_calls = Vec::new();

        if let Some(blocks) = json_resp.get("content").and_then(|c| c.as_array()) {
            for b in blocks {
                let b_type = b.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if b_type == "text" {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                } else if b_type == "tool_use" {
                    let id = b.get("id").and_then(|i| i.as_str()).unwrap_or("call_anthropic").to_string();
                    let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let input = b.get("input").cloned().unwrap_or_else(|| json!({}));
                    tool_calls.push(ToolCall::new(id, name, input.to_string()));
                }
            }
        }

        Ok(CompletionResponse {
            content: text,
            tool_calls,
        })
    }

    /// Canlı akış (SSE streaming)
    #[allow(dead_code)]
    pub async fn chat_completion_stream(
        &self,
        provider_id: &str,
        model_id: &str,
        messages: &[ChatMessage],
        sender: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        // ── 1. Elytra Dahili Çevrimdışı Motoru ──
        if provider_id == "demo" || provider_id == "offline" {
            let last_user = messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .and_then(|m| m.content.as_deref())
                .unwrap_or("merhaba");

            let full_text = generate_demo_response(last_user);
            for chunk in full_text.split_inclusive(' ') {
                let _ = sender.send(StreamEvent::ContentDelta(chunk.to_string())).await;
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
            let _ = sender.send(StreamEvent::Done).await;
            return Ok(());
        }

        // ── 2. Anthropic Messages Akışı ──
        if provider_id == "anthropic" {
            return self.stream_anthropic(model_id, messages, sender).await;
        }

        // ── 3. Standart OpenAI-Uyumlu Akış ──
        let (url, api_key) = self.resolve_endpoint(provider_id)?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(ref key) = api_key
            && !key.is_empty() {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {key}"))
                        .map_err(|e| anyhow!("Gecersiz API anahtari: {e}"))?,
                );
                if (provider_id == "google" || provider_id == "gemini")
                    && let Ok(v) = HeaderValue::from_str(key) {
                        headers.insert("x-goog-api-key", v);
                    }
            }

        if provider_id == "openrouter" {
            headers.insert("HTTP-Referer", HeaderValue::from_static("https://github.com/elytra-cli"));
            headers.insert("X-Title", HeaderValue::from_static("Elytra"));
        }

        let body = json!({
            "model": model_id,
            "messages": messages,
            "temperature": self.config.temperature.unwrap_or(0.7),
            "max_tokens": self.config.max_tokens.unwrap_or(2048),
            "stream": true,
        });

        let resp_res = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await;

        let resp = match resp_res {
            Ok(r) => r,
            Err(e) => {
                let err_msg = if provider_id == "local" {
                    format!("Ollama yerel sunucusuna baglanilamadi ({}). Ollama'nin calistigindan emin olun.", url)
                } else {
                    format!("Ag baglanti hatasi: {e}")
                };
                let _ = sender.send(StreamEvent::Error(err_msg.clone())).await;
                return Err(anyhow!(err_msg));
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let raw_text = resp.text().await.unwrap_or_default();
            let err_summary = format_api_error(status.as_u16(), &raw_text);
            let full_err = format!("Saglayici Yaniti [HTTP {}]: {}", status, err_summary);
            let _ = sender.send(StreamEvent::Error(full_err.clone())).await;
            return Err(anyhow!(full_err));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = match chunk_res {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = sender.send(StreamEvent::Error(format!("Akis kesildi: {e}"))).await;
                    break;
                }
            };

            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].trim().to_string();
                buffer.drain(..=idx);

                if line.is_empty() {
                    continue;
                }

                // data: veya data:  biçimlerini esnekçe yakala
                let data = if let Some(d) = line.strip_prefix("data:") {
                    d.trim()
                } else {
                    continue;
                };

                if data == "[DONE]" {
                    let _ = sender.send(StreamEvent::Done).await;
                    return Ok(());
                }

                if let Ok(json_val) = serde_json::from_str::<Value>(data)
                    && let Some(choices) = json_val.get("choices").and_then(|c| c.as_array())
                        && let Some(first) = choices.first() {
                            // 1. Standart delta.content
                            if let Some(delta) = first.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if !content.is_empty() {
                                        let _ = sender.send(StreamEvent::ContentDelta(content.to_string())).await;
                                    }
                                } else if let Some(reasoning) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                                    // DeepSeek R1 Akıl yürütme düşünceleri
                                    if !reasoning.is_empty() {
                                        let _ = sender.send(StreamEvent::ContentDelta(reasoning.to_string())).await;
                                    }
                                }
                            } else if let Some(text) = first.get("text").and_then(|t| t.as_str())
                                && !text.is_empty() {
                                    let _ = sender.send(StreamEvent::ContentDelta(text.to_string())).await;
                                }
                        }
            }
        }

        let _ = sender.send(StreamEvent::Done).await;
        Ok(())
    }

    /// Anthropic SSE Akışını Gerçekleştir
    #[allow(dead_code)]
    async fn stream_anthropic(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        sender: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let key = match self.config.get_key_for("anthropic") {
            Some(k) => k,
            None => {
                let err = "Anthropic API anahtari eksik (/key anthropic <anahtar>)".to_string();
                let _ = sender.send(StreamEvent::Error(err.clone())).await;
                return Err(anyhow!(err));
            }
        };

        let url = "https://api.anthropic.com/v1/messages";
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", HeaderValue::from_str(&key)?);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let mut system_prompt = String::new();
        let mut anthropic_msgs = Vec::new();

        for m in messages {
            if m.role == "system" {
                if let Some(ref c) = m.content {
                    system_prompt.push_str(c);
                    system_prompt.push('\n');
                }
            } else {
                let role = if m.role == "assistant" { "assistant" } else { "user" };
                anthropic_msgs.push(json!({
                    "role": role,
                    "content": m.content.clone().unwrap_or_default()
                }));
            }
        }

        let mut body = json!({
            "model": model_id,
            "messages": anthropic_msgs,
            "max_tokens": 4096,
            "stream": true,
        });

        if !system_prompt.trim().is_empty() {
            body["system"] = json!(system_prompt.trim());
        }

        let resp = self.http.post(url).headers(headers).json(&body).send().await
            .map_err(|e| anyhow!("Anthropic ag hatasi: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            let err = format!("Anthropic API Hatasi [{}]: {}", status, format_api_error(status.as_u16(), &raw));
            let _ = sender.send(StreamEvent::Error(err.clone())).await;
            return Err(anyhow!(err));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = match chunk_res {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = sender.send(StreamEvent::Error(format!("Akis kesildi: {e}"))).await;
                    break;
                }
            };

            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].trim().to_string();
                buffer.drain(..=idx);

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if let Ok(json_val) = serde_json::from_str::<Value>(data) {
                        let ev_type = json_val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if ev_type == "content_block_delta" {
                            if let Some(delta) = json_val.get("delta")
                                && let Some(txt) = delta.get("text").and_then(|t| t.as_str()) {
                                    let _ = sender.send(StreamEvent::ContentDelta(txt.to_string())).await;
                                }
                        } else if ev_type == "message_stop" {
                            let _ = sender.send(StreamEvent::Done).await;
                            return Ok(());
                        }
                    }
                }
            }
        }

        let _ = sender.send(StreamEvent::Done).await;
        Ok(())
    }

    /// Sağlayıcıya göre URL ve API anahtarını çöz
    fn resolve_endpoint(&self, provider_id: &str) -> Result<(String, Option<String>)> {
        if provider_id == "demo" || provider_id == "offline" {
            return Ok(("local://elytra-engine".into(), None));
        }

        // Özel endpoint tanımlıysa
        if let Some(ref base) = self.config.custom_base_url {
            let url = if base.ends_with("/chat/completions") {
                base.clone()
            } else {
                format!("{}/chat/completions", base.trim_end_matches('/'))
            };
            return Ok((url, self.config.custom_api_key.clone()));
        }

        let provider = crate::api::providers::PROVIDERS
            .iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| anyhow!("Tanimsiz saglayici: '{}'", provider_id))?;

        let url = if provider.base_url.ends_with("/chat/completions") || provider.base_url.ends_with("/messages") {
            provider.base_url.to_string()
        } else {
            format!("{}/chat/completions", provider.base_url.trim_end_matches('/'))
        };

        let key = self.config.get_key_for(provider.id);

        // Yerel sağlayıcı (Ollama) için anahtar gerekmez
        if provider.id == "local" {
            return Ok((url, None));
        }

        // Nous Research için ücretsiz mock/demo anahtar yoksa kontrol et
        if provider.id == "nous" && key.is_none() {
            return Ok((url, Some("nous-public-preview".into())));
        }

        Ok((url, key))
    }
}

/// HTTP hata mesajlarını insan tarafından okunabilir kıl
pub fn format_api_error(status: u16, raw: &str) -> String {
    // 1. JSON hata gövdesi varsa "error.message" alanını çıkart
    if let Ok(val) = serde_json::from_str::<Value>(raw) {
        if let Some(msg) = val.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            return format!("{msg} (HTTP {status})");
        }
        if let Some(arr) = val.as_array()
            && let Some(msg) = arr.first().and_then(|v| v.get("error")).and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                return format!("{msg} (HTTP {status})");
            }
    }

    if status == 401 {
        return "API anahtari gecersiz veya yetkisiz (401). /key <saglayici> <anahtar> ile kontrol edin.".into();
    }
    if status == 429 {
        return "Istek veya kota siniri asildi (429 Rate Limit / Quota Exceeded). Lutfen bekleyin veya baska saglayici secin.".into();
    }
    if status == 403 || raw.contains("Cloudflare") || raw.contains("<html") {
        return "Erisim engellendi (403). Gecerli bir API anahtari gereklidir.".into();
    }
    if status == 404 {
        return "Model veya endpoint bulunamadi (404). Lutfen model seciciden aktif bir model secin.".into();
    }
    if status == 503 {
        return "Model sunucusu su an asiri yuk altinda (503). Lutfen birkac saniye sonra tekrar deneyin.".into();
    }
    if raw.len() > 200 {
        crate::util::truncate_chars(raw, 200)
    } else if raw.trim().is_empty() {
        format!("HTTP kodu: {}", status)
    } else {
        raw.trim().to_string()
    }
}

/// Çevrimdışı Elytra Motoru — ReAct Araç Çağrısı ve Yanıt Üretimi
pub fn generate_demo_completion(messages: &[ChatMessage]) -> CompletionResponse {
    let last_msg = match messages.last() {
        Some(m) => m,
        None => return CompletionResponse { content: "Merhaba!".into(), tool_calls: vec![] },
    };

    // 1. Eğer bir araç az önce çalıştıysa (role == "tool"):
    if last_msg.role == "tool" {
        let tool_out = last_msg.content.as_deref().unwrap_or("");
        let first_user = messages
            .iter()
            .find(|m| m.role == "user")
            .and_then(|m| m.content.as_deref())
            .unwrap_or("Gorev");

        let summary = format!(
            "◆ [Dahili Elytra Motoru — Arac Basariyla Calistirildi]\n\
             Gorev: \"{}\"\n\n\
             Elde Edilen Gercek Cikti:\n{}\n\n\
             ✓ Islem tamamlandi.",
            first_user, tool_out
        );

        return CompletionResponse {
            content: summary,
            tool_calls: vec![],
        };
    }

    // 2. Kullanıcı mesajı geldiğinde araç gerekiyorsa ToolCall üret:
    let prompt = last_msg.content.as_deref().unwrap_or("").to_lowercase();

    // 2.1 Dosya okuma
    if (prompt.contains("oku") || prompt.contains("read") || prompt.contains("goster") || prompt.contains("göster") || prompt.contains("içerik") || prompt.contains("icerik"))
        && (prompt.contains(".toml") || prompt.contains(".rs") || prompt.contains(".json") || prompt.contains(".md") || prompt.contains("cargo") || prompt.contains("readme"))
    {
        let file = if prompt.contains("cargo.toml") || prompt.contains("cargo") {
            "Cargo.toml"
        } else if prompt.contains("readme") {
            "README.md"
        } else if prompt.contains("main.rs") {
            "src/main.rs"
        } else if prompt.contains("lib.rs") {
            "src/lib.rs"
        } else {
            prompt.split_whitespace().find(|w| w.contains('.') || w.contains('/')).unwrap_or("Cargo.toml")
        };

        return CompletionResponse {
            content: format!("'{file}' dosyasini okumak icin read_file aracini cagiriyorum."),
            tool_calls: vec![ToolCall::new("call_demo_read", "read_file", json!({ "path": file }).to_string())],
        };
    }

    // 2.2 Görev listesi
    if prompt.contains("gorev") || prompt.contains("görev") || prompt.contains("task") || prompt.contains("todo") {
        return CompletionResponse {
            content: "Gorev listesini goruntulemek icin task_tracker aracini cagiriyorum.".into(),
            tool_calls: vec![ToolCall::new("call_demo_task", "task_tracker", json!({ "action": "list" }).to_string())],
        };
    }

    // 2.3 Dizin listeleme
    if prompt.contains("list") || prompt.contains("dizin") || prompt.contains("klasor") || prompt.contains("klasör") || prompt.contains("dosyalar") {
        let dir = if prompt.contains("src") {
            "src"
        } else if prompt.contains("test") {
            "tests"
        } else {
            "."
        };

        return CompletionResponse {
            content: format!("'{dir}' dizinini listelemek icin list_dir aracini cagiriyorum."),
            tool_calls: vec![ToolCall::new("call_demo_list", "list_dir", json!({ "path": dir, "max_depth": 2 }).to_string())],
        };
    }

    // 2.3 Git durumu
    if prompt.contains("git status") || prompt.contains("git durum") || (prompt.contains("git") && (prompt.contains("dal") || prompt.contains("branch"))) {
        return CompletionResponse {
            content: "Git durumunu kontrol etmek icin git_status aracini cagiriyorum.".into(),
            tool_calls: vec![ToolCall::new("call_demo_git", "git_status", "{}")],
        };
    }

    // 2.4 Git diff
    if prompt.contains("git diff") || prompt.contains("diff") || prompt.contains("fark") {
        return CompletionResponse {
            content: "Git degisikliklerini incelemek icin git_diff aracini cagiriyorum.".into(),
            tool_calls: vec![ToolCall::new("call_demo_diff", "git_diff", "{}")],
        };
    }

    // 2.5 Diagnostics / Cargo check
    if prompt.contains("check") || prompt.contains("diagnostics") || prompt.contains("derle") || prompt.contains("linter") {
        return CompletionResponse {
            content: "Proje kontrolleri icin diagnostics aracini cagiriyorum.".into(),
            tool_calls: vec![ToolCall::new("call_demo_diag", "diagnostics", "{}")],
        };
    }

    // 2.6 Repo haritası
    if prompt.contains("repo") || prompt.contains("harita") || prompt.contains("mimari") || prompt.contains("yapı") || prompt.contains("yapi") {
        return CompletionResponse {
            content: "Kod tabani mimarisi ve dosya haritasini cikarmak icin repo_map aracini cagiriyorum.".into(),
            tool_calls: vec![ToolCall::new("call_demo_map", "repo_map", json!({ "dir": "." }).to_string())],
        };
    }

    // 2.7 Web arama
    if prompt.contains("web") || prompt.contains("ara") || prompt.contains("search") || prompt.contains("google") {
        let q = last_msg.content.as_deref().unwrap_or("rust lang");
        return CompletionResponse {
            content: format!("Web'de '{}' aramasini gerceklestirmek icin web_search aracini cagiriyorum.", q),
            tool_calls: vec![ToolCall::new("call_demo_search", "web_search", json!({ "query": q, "max_results": 5 }).to_string())],
        };
    }

    // Normal sohbet veya soru
    let resp = generate_demo_response(last_msg.content.as_deref().unwrap_or("merhaba"));
    CompletionResponse {
        content: resp,
        tool_calls: vec![],
    }
}

/// Metin içine serpiştirilmiş araç çağrılarını tespit et ve ayrıştır
pub fn extract_tool_calls_from_text(content: &str) -> (String, Vec<ToolCall>) {
    let mut tool_calls = Vec::new();
    let mut clean_text = content.to_string();

    let valid_tools = [
        "read_file", "write_file", "edit_file", "apply_patch", "list_dir",
        "grep_search", "file_search", "git_status", "git_diff", "diagnostics",
        "bash_exec", "web_fetch", "web_search", "task_tracker", "repo_map"
    ];

    // 1. <tool_call> ... </tool_call> etiketleri
    if let Ok(re_tag) = regex::Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>") {
        for cap in re_tag.captures_iter(content) {
            let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Ok(val) = serde_json::from_str::<Value>(inner) {
                let name = val.get("name").or_else(|| val.get("tool")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let args = val.get("arguments").or_else(|| val.get("parameters")).or_else(|| val.get("args"))
                    .map(|v| if v.is_string() { v.as_str().unwrap().to_string() } else { v.to_string() })
                    .unwrap_or_else(|| "{}".to_string());
                if valid_tools.contains(&name.as_str()) {
                    let id = format!("call_extracted_{}", tool_calls.len() + 1);
                    tool_calls.push(ToolCall::new(id, name, args));
                }
            }
        }
        if !tool_calls.is_empty() {
            clean_text = re_tag.replace_all(&clean_text, "").to_string();
            return (clean_text.trim().to_string(), tool_calls);
        }
    }

    // 2. ```json ... ``` blokları
    if let Ok(re_json) = regex::Regex::new(r"(?s)```(?:json)?\s*(\{\s*.*?\s*\})\s*```") {
        for cap in re_json.captures_iter(content) {
            let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Ok(val) = serde_json::from_str::<Value>(inner) {
                let name = val.get("name").or_else(|| val.get("tool")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let args = val.get("arguments").or_else(|| val.get("parameters")).or_else(|| val.get("args"))
                    .map(|v| if v.is_string() { v.as_str().unwrap().to_string() } else { v.to_string() })
                    .unwrap_or_else(|| "{}".to_string());
                if valid_tools.contains(&name.as_str()) {
                    let id = format!("call_extracted_{}", tool_calls.len() + 1);
                    tool_calls.push(ToolCall::new(id, name, args));
                }
            }
        }
        if !tool_calls.is_empty() {
            clean_text = re_json.replace_all(&clean_text, "").to_string();
            return (clean_text.trim().to_string(), tool_calls);
        }
    }

    // 3. Action: tool_name \n Action Input: {...} formatı
    if let Ok(re_act) = regex::Regex::new(r#"(?s)Action:\s*([a-zA-Z0-9_]+)\s*\nAction Input:\s*(\{.*?\})"#) {
        for cap in re_act.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let args = cap.get(2).map(|m| m.as_str()).unwrap_or("{}").to_string();
            if valid_tools.contains(&name.as_str()) {
                let id = format!("call_extracted_{}", tool_calls.len() + 1);
                tool_calls.push(ToolCall::new(id, name, args));
            }
        }
        if !tool_calls.is_empty() {
            clean_text = re_act.replace_all(&clean_text, "").to_string();
            return (clean_text.trim().to_string(), tool_calls);
        }
    }

    (clean_text, tool_calls)
}

/// Elytra Dahili Çevrimdışı Motor Yanıt Üreticisi
pub fn generate_demo_response(prompt: &str) -> String {
    let p_lower = prompt.to_lowercase();

    // 1. Araç Listesi
    if p_lower.contains("tool") || p_lower.contains("arac") || p_lower.contains("araç") {
        let tools = crate::tools::get_available_tools();
        let mut out = String::from("◆ Elytra 15 Dahili Ajan Araci:\n\n");
        for t in tools {
            out.push_str(&format!("  • {:<16} : {}\n", t.name, t.description));
        }
        return out;
    }

    if p_lower.contains("ping") || p_lower.contains("test") {
        return "OK (Elytra Cevrimdisi Motor Calisiyor)".into();
    }

    if p_lower.contains("merhaba") || p_lower.contains("selam") || p_lower.contains("hey") || p_lower.contains("kimsin") {
        return "◆ Merhaba! Ben Elytra — Native Rust ile gelistirilmis, ultra-hizli (<12MB RAM, 0% CPU) terminal yapay zeka asistaniyim.\n\n\
                Dahili 15 Ajan Araci aktif: Dosya okuyabilir/yazabilir/duzenleyebilir, git durumunu gorebilir, web'de arama yapabilir ve terminal komutlari calistirabilirim.\n\n\
                Deneyin:\n\
                - \"Cargo.toml dosyasini oku\"\n\
                - \"src klasorunu listele\"\n\
                - \"git status\"\n\
                - \"cargo check\"\n\
                - \"proje repo haritasini cikar\"\n\
                - \"web'de rust async ara\"\n\n\
                ► Bulut Modellerine Baglanmak Icin: /key openrouter <anahtar> veya /key groq <anahtar>".into();
    }

    format!(
        "◆ Elytra Cevrimdisi Motoru aktif.\n\n\
        Sorunuz: \"{}\"\n\n\
        Yerel dosya ve kod islemleri icin: \"Cargo.toml oku\", \"src listele\" veya \"git status\" diyebilirsiniz.\n\
        Bulut modelleriyle derin kodlama icin: /key groq <anahtar> veya /key openrouter <anahtar> ekleyin.",
        prompt
    )
}
