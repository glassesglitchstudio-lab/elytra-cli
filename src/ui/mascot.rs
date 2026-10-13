//! Elytra maskotu — "kanatlı kedi" (winged cat)
//!
//! Elytra'nın maskotu iridesan kanatlı bir kedi. Terminalde geniş
//! desteklenen karakterlerle çizilir (kutu çizimleri + temel bloklar).

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Maskotun renkleri
pub struct MascotColors {
    /// Kedi tüyü — kırık beyaz
    pub fur: Color,
    /// Kedi gözü — iridesan yeşil
    pub eyes: Color,
    /// Kanatlar — iridesan mor
    pub wings: Color,
    /// Burun/ağız — ılık pembe
    pub nose: Color,
}

impl Default for MascotColors {
    fn default() -> Self {
        Self {
            fur: Color::Rgb(230, 235, 228),
            eyes: Color::Rgb(64, 224, 160),
            wings: Color::Rgb(160, 110, 230),
            nose: Color::Rgb(230, 160, 170),
        }
    }
}

/// Kanatlı kedi maskotunu satır satır üret.
///
/// Her satır: (metin, karakter rengi) çiftleri — kanatlar mor, kedi beyaz.
pub fn mascot_lines(colors: &MascotColors) -> Vec<Line<'static>> {
    let wing = Style::default().fg(colors.wings);
    let fur = Style::default().fg(colors.fur);
    let eye = Style::default()
        .fg(colors.eyes)
        .add_modifier(Modifier::BOLD);
    let nose = Style::default().fg(colors.nose);

    // Maskot: ortada kedi, yanlarda iridesan kanat parçacıkları
    vec![
        // Kanat uçları (üst)
        Line::from(vec![
            Span::styled("  .  *", wing),
            Span::raw("        "),
            Span::styled(" *  .", wing),
        ]),
        Line::from(vec![
            Span::styled(" *    ", wing),
            Span::styled("/\\___/\\", fur),
            Span::styled("    *", wing),
        ]),
        // Kedi yüzü — gözler parlak
        Line::from(vec![
            Span::styled("*  ", wing),
            Span::styled("( ", fur),
            Span::styled("o", eye),
            Span::styled(" . ", fur),
            Span::styled("o", eye),
            Span::styled(" )", fur),
            Span::styled("  *", wing),
        ]),
        Line::from(vec![
            Span::styled("   ", wing),
            Span::styled(" >   ", fur),
            Span::styled("^", nose),
            Span::styled("  <", fur),
        ]),
        Line::from(vec![
            Span::styled("  ", wing),
            Span::styled(" /|   |\\", fur),
        ]),
        Line::from(vec![
            Span::styled("* ", wing),
            Span::styled("(_|   |_)", fur),
            Span::styled(" *", wing),
        ]),
        // Alt kanat gölgesi
        Line::from(vec![
            Span::styled("  '     '", wing),
        ]),
    ]
}

/// Küçük durum göstergesi — maskotun altında mini bilgi satırı
pub fn mascot_status(model_connected: bool) -> Line<'static> {
    if model_connected {
        Line::from(vec![
            Span::styled(" (^._.^) ", Style::default().fg(Color::Rgb(120, 230, 130))),
            Span::styled("bağlı", Style::default().fg(Color::Rgb(120, 230, 130))),
        ])
    } else {
        Line::from(vec![
            Span::styled(" (-_-) ", Style::default().fg(Color::Rgb(230, 90, 90))),
            Span::styled("bağlantı yok", Style::default().fg(Color::Rgb(230, 90, 90))),
        ])
    }
}
