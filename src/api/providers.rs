//! Elytra sağlayıcı katmanı — katalog artık `elytra-providers` crate'inde.
//!
//! Bu modül yalnız **seçim mantığını** (Selection) tutar; sağlayıcı ve model
//! verisi tek kaynaktan gelir: [`elytra_providers`].
//!
//! ## Neden ayrı crate?
//! - Elytra CLI, Elytra Sandbox ve gelecek araçlar aynı kataloğu paylaşır.
//! - Katalog tek başına test edilir (`cargo test -p elytra-providers`).
//! - Veri ile seçim mantığı ayrılır: katalog saf veri, Selection etkileşim.

pub use elytra_providers::{ApiKind, Model, Provider, PROVIDERS};

/// Sağlayıcı/model seçimi — TUI ve CLI aynı nesneyi kullanır.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub provider_index: usize,
    pub model_index: usize,
    /// Kullanıcının elle girdiği özel / dinamik model (örn: "gpt-6.1-sol")
    pub custom_model: Option<String>,
}

impl Selection {
    /// Seçili sağlayıcı
    pub fn provider(&self) -> &'static Provider {
        &PROVIDERS[self.provider_index.min(PROVIDERS.len() - 1)]
    }

    /// Seçili statik model
    pub fn model(&self) -> &'static Model {
        let p = self.provider();
        &p.models[self.model_index.min(p.models.len() - 1)]
    }

    /// Aktif model kimliği (özel model varsa onu, yoksa statik modeli döner)
    pub fn model_id(&self) -> String {
        match self.custom_model {
            Some(ref c) => c.clone(),
            None => self.model().id.to_string(),
        }
    }

    /// Aktif model adı
    pub fn model_name(&self) -> String {
        match self.custom_model {
            Some(ref c) => format!("{c} (Özel Model)"),
            None => self.model().name.to_string(),
        }
    }

    /// Aktif model bağlam boyutu
    #[allow(dead_code)]
    pub fn model_context(&self) -> u32 {
        if self.custom_model.is_some() { 128_000 } else { self.model().context }
    }

    /// `"openrouter/thinkingmachines/inkling:free"` biçiminde tam kimlik
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.provider().id, self.model_id())
    }

    /// Sonraki sağlayıcıya geç
    #[allow(dead_code)]
    pub fn next_provider(&mut self) {
        self.provider_index = (self.provider_index + 1) % PROVIDERS.len();
        self.model_index = 0;
        self.custom_model = None;
    }

    /// Aynı sağlayıcı içinde sonraki modele geç
    #[allow(dead_code)]
    pub fn next_model(&mut self) {
        self.custom_model = None;
        let len = self.provider().models.len();
        self.model_index = (self.model_index + 1) % len;
    }

    /// Sağlayıcıyı kimliğine göre ayarla
    pub fn set_provider_by_id(&mut self, prov_id: &str) -> bool {
        let want = prov_id.to_lowercase();
        match PROVIDERS.iter().position(|p| p.id.to_lowercase() == want) {
            Some(idx) => {
                self.provider_index = idx;
                self.model_index = 0;
                self.custom_model = None;
                true
            }
            None => false,
        }
    }

    /// Model veya sağlayıcı adına göre ayarla.
    ///
    /// Örnek girdiler: `"groq"`, `"openai/gpt-5.6-sol"`, `"deepseek-chat"`.
    /// Hiçbir statik modelle eşleşmezse aktif sağlayıcı altında **özel model**
    /// olarak kabul edilir (yeni çıkan modeller elle yazılabilsin).
    pub fn set_by_name(&mut self, query: &str) -> bool {
        let q = query.trim();
        let q_lower = q.to_lowercase();

        // 0. Doğrudan sağlayıcı kimliği ("groq", "openrouter", ...)
        if let Some(p_idx) = PROVIDERS.iter().position(|p| p.id.to_lowercase() == q_lower) {
            self.provider_index = p_idx;
            self.model_index = 0;
            self.custom_model = None;
            return true;
        }

        // Emekliye ayrılmış / yaygın yazılan Gemini adlarını güncel modele yönlendir
        let mapped = match q_lower.as_str() {
            "gemini-1.5-pro" | "gemini-2.0-pro" | "gemini-2.5-pro" | "gemini-pro" => "gemini-3.5-flash-lite",
            "gemini-1.5-flash" | "gemini-2.0-flash" | "gemini-2.5-flash" | "gemini-flash" => "gemini-3.5-flash-lite",
            other => other,
        };

        // 1. Önekli yazım: "openrouter/...", "openai/..."
        for (p_idx, p) in PROVIDERS.iter().enumerate() {
            let prefix = format!("{}/", p.id);
            if let Some(rest) = q.strip_prefix(&prefix) {
                self.provider_index = p_idx;
                match p.models.iter().position(|m| m.id.eq_ignore_ascii_case(rest)) {
                    Some(m_idx) => {
                        self.model_index = m_idx;
                        self.custom_model = None;
                    }
                    None => self.custom_model = Some(rest.to_string()),
                }
                return true;
            }
        }

        // 2. Statik katalogda tam eşleşme (kimlik veya ad)
        for (p_idx, p) in PROVIDERS.iter().enumerate() {
            for (m_idx, m) in p.models.iter().enumerate() {
                if m.id.to_lowercase() == mapped || m.name.to_lowercase() == mapped {
                    self.provider_index = p_idx;
                    self.model_index = m_idx;
                    self.custom_model = None;
                    return true;
                }
            }
        }

        // 3. Kısmi eşleşme
        for (p_idx, p) in PROVIDERS.iter().enumerate() {
            for (m_idx, m) in p.models.iter().enumerate() {
                if m.id.to_lowercase().contains(mapped) {
                    self.provider_index = p_idx;
                    self.model_index = m_idx;
                    self.custom_model = None;
                    return true;
                }
            }
        }

        // 4. Eşleşme yok → aktif sağlayıcı altında özel model
        self.custom_model = Some(q.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varsayilan_secim_gecerli() {
        let s = Selection::default();
        assert!(!s.provider().models.is_empty());
        assert!(!s.model_id().is_empty());
    }

    #[test]
    fn saglayici_ve_model_secimi() {
        let mut s = Selection::default();
        assert!(s.set_by_name("groq"));
        assert_eq!(s.provider().id, "groq");
        assert!(s.set_by_name("llama-3.3-70b-versatile"));
        assert_eq!(s.model_id(), "llama-3.3-70b-versatile");
        assert_eq!(s.full_id(), "groq/llama-3.3-70b-versatile");
    }

    #[test]
    fn onekli_yazim_ve_ozel_model() {
        let mut s = Selection::default();
        assert!(s.set_by_name("openrouter/thinkingmachines/inkling:free"));
        assert_eq!(s.provider().id, "openrouter");
        assert_eq!(s.model_id(), "thinkingmachines/inkling:free");

        // Katalogda olmayan model → özel model olarak kabul
        assert!(s.set_by_name("openai/yeni-cikan-model"));
        assert_eq!(s.provider().id, "openai");
        assert_eq!(s.model_id(), "yeni-cikan-model");
    }

    #[test]
    fn dongusel_gecisler() {
        let mut s = Selection::default();
        let first = s.provider().id;
        s.next_provider();
        assert_ne!(s.provider().id, first);
        // İlk adımdan sonra kalan tur sayısı kadar dönünce başa dönmeli
        for _ in 0..PROVIDERS.len() - 1 {
            s.next_provider();
        }
        assert_eq!(s.provider().id, first);
    }
}
