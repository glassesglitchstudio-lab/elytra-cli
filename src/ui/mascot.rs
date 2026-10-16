//! Elytra Vektör Başlık & Kompakt Rozet Bileşenleri
//!
//! OpenCode tarzı saf vektör blok glifleri (▀, ▄, █) ve minimalist rozetler.
//! Sıfır emoji, sade ve profesyonel terminal estetiği.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MascotMood {
    Idle(usize),
    Thinking(usize),
    Success,
    Error,
}

/// Üst Bar İçin Sade Durum Rozeti (Sıfır Emoji)
pub fn inline_wing_badge(mood: MascotMood) -> Span<'static> {
    match mood {
        MascotMood::Thinking(_) => {
            Span::styled(" [THINKING] ", Style::default().fg(Color::Rgb(245, 195, 75)).add_modifier(Modifier::BOLD))
        }
        MascotMood::Error => {
            Span::styled(" [ERROR] ", Style::default().fg(Color::Rgb(230, 90, 90)).add_modifier(Modifier::BOLD))
        }
        MascotMood::Success => {
            Span::styled(" [OK] ", Style::default().fg(Color::Rgb(120, 230, 130)).add_modifier(Modifier::BOLD))
        }
        MascotMood::Idle(f) => {
            let frames = ["◆", "◇", "◆", "◇"];
            let glyph = frames[f % frames.len()];
            Span::styled(format!(" {glyph} "), Style::default().fg(Color::Rgb(64, 224, 160)))
        }
    }
}

/// OpenCode Tarzı Temiz Karşılama Banner'ı (Sohbet Başında Gösterilir)
pub fn welcome_banner(provider_name: &str, model_name: &str) -> Vec<Line<'static>> {
    let st_title = Style::default().fg(Color::Rgb(64, 224, 160)).add_modifier(Modifier::BOLD);
    let st_sub = Style::default().fg(Color::Rgb(160, 110, 230));
    let st_muted = Style::default().fg(Color::Rgb(110, 125, 115));
    let st_accent = Style::default().fg(Color::Rgb(80, 200, 220));

    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ███████╗██╗  ██╗   ██╗████████╗██████╗  █████╗ ", st_title),
        ]),
        Line::from(vec![
            Span::styled("  ██╔════╝██║  ╚██╗ ██╔╝╚══██╔══╝██╔══██╗██╔══██╗", st_title),
        ]),
        Line::from(vec![
            Span::styled("  █████╗  ██║   ╚████╔╝    ██║   ██████╔╝███████║", st_title),
        ]),
        Line::from(vec![
            Span::styled("  ██╔══╝  ██║    ╚██╔╝     ██║   ██╔══██╗██╔══██║", st_title),
        ]),
        Line::from(vec![
            Span::styled("  ███████╗███████╗██║      ██║   ██║  ██║██║  ██║", st_title),
        ]),
        Line::from(vec![
            Span::styled("  ╚══════╝╚══════╝╚═╝      ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═╝", st_title),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Terminal-Native AI Coding Assistant · 16 Saglayici & 15 Dahili Arac", st_sub),
        ]),
        Line::from(vec![
            Span::styled("  Aktif: ", st_muted),
            Span::styled(format!("{} > {}", provider_name, model_name), st_accent),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [Tab] ", st_accent),
            Span::styled("Model & Saglayici Paleti   ", st_muted),
            Span::styled("[/] ", st_title),
            Span::styled("Canli Komutlar   ", st_muted),
            Span::styled("[F1] ", st_accent),
            Span::styled("Hizli Menu   ", st_muted),
            Span::styled("[/help] ", st_title),
            Span::styled("Rehber", st_muted),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  • Tab tusuna basarak 16 saglayici ve modelleri arasinda gecis yapabilirsiniz.", st_muted),
        ]),
        Line::from(vec![
            Span::styled("  • / yazarak canli komut listesini goruntuleyebilir veya /agent ile gorev verebilirsiniz.", st_muted),
        ]),
        Line::from(""),
    ]
}
