//! Elytra renk paleti — "iridesan böcek kabuğu" konsepti
//!
//! Elytra (kanat kılıfı) adını böceklerin iridesan kabuklarından alıyor.
//! Renkler bu yağsı görünümü taklit ediyor: yeşil-mor-turkuaz geçişleri.

use ratatui::style::{Color, Modifier, Style};

/// Elytra'nın ana renk paleti.
///
/// `ratatui` renkleri terminalde RGB olarak çizilir (TrueColor destekleyen
/// terminallerde). Paletin tamamı koyu zemine göre tasarlandı.
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    /// Arka plan — çok koyu, hafif yeşil tonlu siyah (böcek kabuğu gölgesi)
    pub const BG: Color = Color::Rgb(8, 12, 10);

    /// Ana metin — kırık beyaz (saf beyaz yerine hafif yeşil)
    pub const TEXT: Color = Color::Rgb(220, 230, 222);

    /// Soluk metin — ipuçları, zaman damgaları
    pub const MUTED: Color = Color::Rgb(110, 125, 115);

    /// İridesan yeşil — birincil vurgu (kanat kılıfı parlaması)
    pub const PRIMARY: Color = Color::Rgb(64, 224, 160);

    /// İridesan mor — ikincil vurgu (kabuğun mor yansıması)
    pub const SECONDARY: Color = Color::Rgb(160, 110, 230);

    /// İridesan turkuaz — üçüncül vurgu
    pub const ACCENT: Color = Color::Rgb(80, 200, 220);

    /// Hata/acil — ılık kırmızı (paletle uyumlu)
    pub const ERROR: Color = Color::Rgb(230, 90, 90);

    /// Başarı — parlak yeşil
    pub const SUCCESS: Color = Color::Rgb(120, 230, 130);

    /// Kullanıcı mesajı rengi
    pub const USER: Color = Color::Rgb(230, 220, 160);

    /// Asistan mesajı rengi
    pub const ASSISTANT: Color = Color::Rgb(180, 240, 210);
}

#[allow(dead_code)]
pub fn error() -> Style {
    Style::default().fg(Theme::ERROR)
}

#[allow(dead_code)]
pub fn success() -> Style {
    Style::default().fg(Theme::SUCCESS)
}

/// Sık kullanılan stiller — tek satırda erişim için yardımcı fonksiyonlar
pub fn text() -> Style {
    Style::default().fg(Theme::TEXT)
}

pub fn muted() -> Style {
    Style::default().fg(Theme::MUTED)
}

pub fn primary() -> Style {
    Style::default().fg(Theme::PRIMARY)
}

pub fn secondary() -> Style {
    Style::default().fg(Theme::SECONDARY)
}

pub fn accent() -> Style {
    Style::default().fg(Theme::ACCENT)
}

/// Başlık stili — kalın + birincil renk
pub fn title() -> Style {
    Style::default()
        .fg(Theme::PRIMARY)
        .add_modifier(Modifier::BOLD)
}

/// Kullanıcı mesajı stili
#[allow(dead_code)]
pub fn user() -> Style {
    Style::default().fg(Theme::USER)
}

/// Asistan mesajı stili
pub fn assistant() -> Style {
    Style::default().fg(Theme::ASSISTANT)
}

/// İridesan geçiş için renk listesi (gradient bar çizmekte kullanılır)
#[allow(dead_code)]
pub const IRIDESCENT: [Color; 5] = [
    Color::Rgb(64, 224, 160),  // yeşil
    Color::Rgb(80, 200, 220),  // turkuaz
    Color::Rgb(160, 110, 230), // mor
    Color::Rgb(200, 120, 200), // pembe-mor
    Color::Rgb(230, 160, 120), // amber
];
