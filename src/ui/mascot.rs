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

    // Her satır: kanat (*) mor, kedi beyaz, göz yeşil, burun pembe
    vec![
        // 0: üst kanat parçacıkları
        Line::from(vec![
            Span::styled("    .", wing),
            Span::raw("     "),
            Span::styled(".", wing),
            Span::raw("    "),
        ]),
        // 1: kulaklar + üst kanatlar
        Line::from(vec![
            Span::raw(" "),
            Span::styled("*", wing),
            Span::raw("   "),
            Span::styled("/\\_/\\", fur),
            Span::raw("   "),
            Span::styled("*", wing),
            Span::raw(" "),
        ]),
        // 2: gözler
        Line::from(vec![
            Span::styled("*", wing),
            Span::raw("   ( "),
            Span::styled("o", eye),
            Span::raw("."),
            Span::styled("o", eye),
            Span::raw(" )   "),
            Span::styled("*", wing),
        ]),
        // 3: burun/ağız
        Line::from(vec![
            Span::raw("     > "),
            Span::styled("^", nose),
            Span::raw(" <     "),
        ]),
        // 4: gövde üst
        Line::from(vec![Span::raw("    /|   |\\    ")]),
        // 5: gövde alt + kanat uçları
        Line::from(vec![
            Span::styled("*", wing),
            Span::raw("  (_|   |_)  "),
            Span::styled("*", wing),
        ]),
        // 6: alt kanat gölgesi
        Line::from(vec![Span::raw("     '     '    ")]),
        // 7: taban kanat ucu
        Line::from(vec![
            Span::raw(" "),
            Span::styled("*", wing),
            Span::raw("           "),
            Span::styled("*", wing),
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
