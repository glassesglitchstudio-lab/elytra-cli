//! Elytra sağlayıcı kataloğu — bulut modelleri
//!
//! v0.1'de katalog statiktir (elle yazılmış liste). API bağlantısı
//! kurulunca bu liste sunucudan canlı çekilecek.
//!
//! NOT: Ollama/yerel model YOK — sadece bulut sağlayıcıları.

/// Bir model tanımı
#[derive(Debug, Clone)]
pub struct Model {
    /// Model kimliği (API'de kullanılır)
    pub id: &'static str,
    /// Görünen ad
    pub name: &'static str,
    /// Bağlam penceresi (token)
    pub context: u32,
    /// Ücretsiz mi?
    pub free: bool,
}

/// Bir sağlayıcı tanımı
#[derive(Debug, Clone)]
pub struct Provider {
    /// Sağlayıcı kimliği
    pub id: &'static str,
    /// Görünen ad
    pub name: &'static str,
    /// Kısa açıklama
    pub hint: &'static str,
    /// Bu sağlayıcının modelleri
    pub models: &'static [Model],
}

/// Elytra'nın bildiği tüm bulut sağlayıcıları
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "nous",
        name: "Nous Research",
        hint: "ücretsiz modeller",
        models: &[
            Model { id: "stealth", name: "Stealth (ücretsiz)", context: 128_000, free: true },
            Model { id: "space-bunny-alpha", name: "Space Bunny Alpha", context: 128_000, free: true },
            Model { id: "longcat-2.5-preview", name: "Longcat 2.5", context: 128_000, free: true },
        ],
    },
    Provider {
        id: "anthropic",
        name: "Anthropic",
        hint: "Claude ailesi",
        models: &[
            Model { id: "claude-opus-5", name: "Claude Opus 5", context: 200_000, free: false },
            Model { id: "claude-sonnet-5", name: "Claude Sonnet 5", context: 200_000, free: false },
        ],
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        hint: "GPT ailesi",
        models: &[
            Model { id: "gpt-6-sol", name: "GPT-6 Sol", context: 400_000, free: false },
            Model { id: "gpt-5", name: "GPT-5", context: 400_000, free: false },
        ],
    },
    Provider {
        id: "xai",
        name: "xAI",
        hint: "Grok ailesi",
        models: &[
            Model { id: "grok-4.6", name: "Grok 4.6", context: 256_000, free: false },
        ],
    },
    Provider {
        id: "google",
        name: "Google",
        hint: "Gemini ailesi",
        models: &[
            Model { id: "gemini-4-argon", name: "Gemini 4 Argon", context: 1_000_000, free: false },
        ],
    },
];

/// Seçili sağlayıcı + model durumu
#[derive(Debug, Clone)]
pub struct Selection {
    pub provider_index: usize,
    pub model_index: usize,
}

impl Default for Selection {
    fn default() -> Self {
        // Varsayılan: nous / stealth (ücretsiz)
        Self {
            provider_index: 0,
            model_index: 0,
        }
    }
}

impl Selection {
    /// Seçili sağlayıcı
    pub fn provider(&self) -> &'static Provider {
        &PROVIDERS[self.provider_index]
    }

    /// Seçili model
    pub fn model(&self) -> &'static Model {
        &self.provider().models[self.model_index]
    }

    /// "nous/stealth" biçiminde tam kimlik
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.provider().id, self.model().id)
    }

    /// Sonraki sağlayıcıya geç (model ilkine dön)
    pub fn next_provider(&mut self) {
        self.provider_index = (self.provider_index + 1) % PROVIDERS.len();
        self.model_index = 0;
    }

    /// Aynı sağlayıcı içinde sonraki modele geç
    pub fn next_model(&mut self) {
        let len = self.provider().models.len();
        self.model_index = (self.model_index + 1) % len;
    }
}
