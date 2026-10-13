//! Elytra TUI — terminal arayüzü
//!
//! Görsel kimlik: "kanat kılıfı açılışı". Açılışta iki yarı-kanat yana
//! kayar, arkasındaki arayüz ortaya çıkar. Standart kutu çerçeveleri
//! kullanılmaz — ayırıcılar iridesan gradient çubuklardır.

pub mod mascot;
pub mod theme;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use std::time::{Duration, Instant};

/// TUI'yi çalıştır: kurulum → boot animasyonu → ana döngü → temizlik
pub async fn run(_model: Option<&str>) -> Result<()> {
    // Terminal hazırlığı: raw mode (tuşları anında yakala) + alternate screen
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = ratatui::init();

    // Ana iş: boot + döngü. Hata olsa bile terminal eski haline dönsün.
    let result = boot_and_loop(terminal).await;

    // Temizlik (her durumda çalışır)
    ratatui::restore();
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    result
}

/// Boot animasyonu + ana etkinlik döngüsü
async fn boot_and_loop(mut terminal: DefaultTerminal) -> Result<()> {
    // ── Aşama 1: Kanat açılış animasyonu ──
    let start = Instant::now();
    let boot_duration = Duration::from_millis(1600);

    while start.elapsed() < boot_duration {
        let progress = start.elapsed().as_secs_f64() / boot_duration.as_secs_f64();
        terminal.draw(|frame| draw_boot(frame, progress))?;

        // Animasyon sırasında çıkış tuşu kontrolü
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    return Ok(());
                }
            }
        }
    }

    // ── Aşama 2: Ana arayüz ──
    let mut input = String::new();
    let mut messages: Vec<(bool, String)> = Vec::new(); // (kullanıcı mı?, metin)
    let mut selection = crate::api::providers::Selection::default();

    loop {
        terminal.draw(|frame| draw_main(frame, &messages, &input, &selection))?;

        // Kullanıcı girdisini bekle (100ms timeout — animasyonlar için)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        break
                    }
                    // Tab: aynı sağlayıcı içinde model değiştir
                    KeyCode::Tab => selection.next_model(),
                    // BackTab (Shift+Tab): sağlayıcı değiştir
                    KeyCode::BackTab => selection.next_provider(),
                    KeyCode::Char(c) => input.push(c),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Enter => {
                        if !input.trim().is_empty() {
                            // Kullanıcı mesajını ekle (API bağlantısı sonra eklenecek)
                            messages.push((true, std::mem::take(&mut input)));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

// ─────────────────────── Çizim fonksiyonları ───────────────────────

/// Boot animasyonu: kapalı kanat kılıfı → iki yarı yana kayar → açılır
fn draw_boot(frame: &mut Frame, progress: f64) {
    let area = frame.area();

    // Arka plan
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::Theme::BG)),
        area,
    );

    // Kanatların kayma miktarı (0 → tam açık)
    let slide = (progress * 2.0).min(1.0); // ilk yarıda tamamen açılır
    let width = area.width;

    // Sol kanat kılıfı yarısı — ovalin sol tarafı, sola doğru kayar
    let offset = (slide * (width / 3) as f64) as u16;

    // Oval kanat kılıfı (kapalı hâli) — ortada
    let center_y = area.height / 2;
    let oval_w = 31u16.min(width.saturating_sub(4).max(9));

    if slide < 1.0 {
        // İki yarı-kanat: kapalıyken ortada birleşip oval olur, açılınca
        // sola/sağa kayarlar. Her yarı ovalin yarısı kadar geniş.
        let half_w = oval_w / 2;
        let center_x = width / 2;

        // Sol yarı: [center - half_w - offset, center - offset)
        let left_x = center_x.saturating_sub(half_w + offset);
        draw_wing_half(frame, left_x, center_y, half_w, true);

        // Sağ yarı: [center + offset, center + half_w + offset)
        let right_x = center_x + offset;
        draw_wing_half(frame, right_x, center_y, half_w, false);
    }

    // Ortada logos — kanatlar açıkken görünür
    if slide > 0.3 {
        let logo = vec![
            Line::from(Span::styled("E L Y T R A", theme::title())),
            Line::from(Span::styled("kanat kılıfı açılıyor...", theme::muted())),
        ];
        let logo_area = Rect::new(width / 2 - oval_w / 2, center_y.saturating_sub(2), oval_w, 4);
        frame.render_widget(Paragraph::new(logo).centered(), logo_area);
    }

    // Alt kısımda ilerleme çubuğu — iridesan gradient
    // (evrensel blok karakterler: Windows konsolunda ▰ "?" görünür)
    let bar_w = area.width.saturating_sub(20);
    let filled = ((bar_w as f64) * progress) as usize;
    let mut bar_spans: Vec<Span> = Vec::with_capacity(bar_w as usize);
    for i in 0..bar_w as usize {
        if i < filled {
            // İlerleyen kısım: iridesan renkler sırayla
            let color = theme::IRIDESCENT[i % theme::IRIDESCENT.len()];
            bar_spans.push(Span::styled("█", Style::default().fg(color)));
        } else {
            bar_spans.push(Span::styled("░", theme::muted()));
        }
    }
    let bar_area = Rect::new(10, area.height.saturating_sub(3), bar_w, 1);
    frame.render_widget(Paragraph::new(Line::from(bar_spans)), bar_area);
}

/// Kanat kılıfının bir yarısını çiz — kapalıyken iki yarı birleşip oval olur.
/// `left = true` sol yarı (iç kenar sağda açık), `false` sağ yarı.
fn draw_wing_half(frame: &mut Frame, x: u16, y: u16, w: u16, left: bool) {
    let area = frame.area();
    // Yükseklik: ovalin yüksekliği (tek sayı olsun — orta satır simetrik)
    let h = 9u16.min(area.height.saturating_sub(4));
    if h < 3 || w < 4 {
        return;
    }
    let x = x.min(area.width.saturating_sub(w));
    let y = y.saturating_sub(h / 2).min(area.height.saturating_sub(h));

    let wing_style = Style::default()
        .fg(theme::Theme::SECONDARY)
        .add_modifier(ratatui::style::Modifier::DIM);

    // Yarı-oval: dış kenar yuvarlak (╭ ╮ ╰ ╯), iç kenar açık
    let mut lines = Vec::with_capacity(h as usize);
    for row in 0..h {
        let mut line = String::with_capacity(w as usize);
        if row == 0 {
            // Üst kenar
            if left {
                line.push('╭');
                line.push_str(&"─".repeat(w as usize - 1));
            } else {
                line.push_str(&"─".repeat(w as usize - 1));
                line.push('╮');
            }
        } else if row == h - 1 {
            // Alt kenar
            if left {
                line.push('╰');
                line.push_str(&"─".repeat(w as usize - 1));
            } else {
                line.push_str(&"─".repeat(w as usize - 1));
                line.push('╯');
            }
        } else {
            // Yan kenar — sadece dış tarafta
            if left {
                line.push('│');
                line.push_str(&" ".repeat(w as usize - 1));
            } else {
                line.push_str(&" ".repeat(w as usize - 1));
                line.push('│');
            }
        }
        lines.push(Line::from(Span::styled(line, wing_style)));
    }

    let rect = Rect::new(x, y, w, h);
    frame.render_widget(Paragraph::new(lines), rect);
}

/// Ana arayüz: gradient başlık + maskot paneli + sohbet + sağlayıcı barları + giriş
fn draw_main(
    frame: &mut Frame,
    messages: &[(bool, String)],
    input: &str,
    selection: &crate::api::providers::Selection,
) {
    let area = frame.area();

    // Arka plan
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::Theme::BG)),
        area,
    );

    // Dikey yerleşim: başlık / (maskot|sohbet) / sağlayıcı / model / giriş
    let chunks = Layout::vertical([
        Constraint::Length(2), // gradient bar + başlık satırı
        Constraint::Min(8),    // maskot paneli + sohbet
        Constraint::Length(1), // sağlayıcı şeridi
        Constraint::Length(1), // model şeridi
        Constraint::Length(3), // giriş kutusu
    ])
    .split(area);

    // Orta bölge: sola maskot paneli, sağa sohbet (terminal genişse)
    let wide = area.width >= 70;
    let main_chunks = if wide {
        Layout::horizontal([Constraint::Length(19), Constraint::Min(25)]).split(chunks[1])
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(chunks[1])
    };

    // ── Başlık: iridesan gradient çubuk ──
    // Her karakter kendi rengiyle ayrı bir Span olarak çizilir —
    // ratatui'de tek Span içinde karakter bazlı renk olmaz.
    // NOT: Windows konsolunda desteklenmeyen glifler (▰ gibi) "?" görünür,
    // bu yüzden evrensel blok karakterleri kullanıyoruz (█ ░).
    let bar_w = area.width.saturating_sub(2) as usize;
    let mut bar_spans: Vec<Span> = Vec::with_capacity(bar_w);
    for i in 0..bar_w {
        let color = theme::IRIDESCENT[i % theme::IRIDESCENT.len()];
        bar_spans.push(Span::styled("█", Style::default().fg(color)));
    }
    let header_lines = vec![
        Line::from(bar_spans),
        Line::from(vec![
            Span::styled(" ◆ ELYTRA ", theme::title()),
            Span::styled("| kanatlı kedi ", theme::secondary()),
            Span::styled("| v0.1", theme::muted()),
            Span::styled(
                format!(" | {} ", selection.full_id()),
                theme::accent(),
            ),
            Span::styled("| çıkış: Esc", theme::muted()),
        ]),
    ];
    frame.render_widget(Paragraph::new(header_lines), chunks[0]);

    // ── Maskot paneli (sol kenar) ──
    if wide {
        let colors = mascot::MascotColors::default();
        let mut panel: Vec<Line> = Vec::new();
        let avail = main_chunks[0].height; // panelin sığdığı satır sayısı

        // Üst kenar
        panel.push(Line::from(Span::styled(
            format!("┌{}┐", "─".repeat(17)),
            theme::muted(),
        )));

        // Maskot (8 satır) — yer varsa
        if avail >= 12 {
            for l in mascot::mascot_lines(&colors) {
                let mut spans = vec![Span::styled("│", theme::muted()), Span::raw(" ")];
                spans.extend(l.spans);
                panel.push(Line::from(spans));
            }
            // Durum satırı
            let status = mascot::mascot_status(false);
            let mut status_spans = vec![Span::styled("│", theme::muted()), Span::raw(" ")];
            status_spans.extend(status.spans);
            panel.push(Line::from(status_spans));
            // İpuçları — sadece yer kalıyorsa
            if avail >= 14 {
                panel.push(Line::from(vec![
                    Span::styled("│", theme::muted()),
                    Span::styled(" Tab: model değiştir", theme::muted()),
                ]));
            }
            if avail >= 15 {
                panel.push(Line::from(vec![
                    Span::styled("│", theme::muted()),
                    Span::styled(" S-Tab: sağlayıcı", theme::muted()),
                ]));
            }
        } else {
            // Küçük terminal: sadece başlık
            panel.push(Line::from(vec![
                Span::styled("│", theme::muted()),
                Span::styled("  ELYTRA", theme::secondary()),
            ]));
        }

        // Alt kenar — kalan satırları doldur (taşmayı önler)
        while (panel.len() as u16) < avail.saturating_sub(1) {
            panel.push(Line::from(vec![
                Span::styled("│", theme::muted()),
                Span::raw("                  "),
            ]));
        }
        panel.push(Line::from(Span::styled(
            format!("└{}┘", "─".repeat(17)),
            theme::muted(),
        )));

        frame.render_widget(Paragraph::new(panel), main_chunks[0]);
    }

    // ── Sohbet alanı ──
    let mut lines: Vec<Line> = Vec::new();
    if messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Elytra hazır. Bir şeyler yaz ve Enter'a bas...",
            theme::muted(),
        )));
    }
    for (is_user, text) in messages {
        let (prefix, style) = if *is_user {
            (" ◆ sen ", theme::user())
        } else {
            (" ◆ elytra ", theme::assistant())
        };
        lines.push(Line::from(Span::styled(prefix, style)));
        for l in text.lines() {
            lines.push(Line::from(Span::styled(format!("   {l}"), style)));
        }
        lines.push(Line::from(""));
    }
    let chat_area = if wide { main_chunks[1] } else { chunks[1] };
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        chat_area,
    );

    // ── Sağlayıcı şeridi — tüm sağlayıcılar, seçili olan vurgulu ──
    // Dar terminalde sığmayan sağlayıcılar gizlenir (sığdığı kadar).
    let bar_budget = area.width.saturating_sub(12) as usize; // "sağlayıcı" etiketi payı
    let mut prov_spans: Vec<Span> = vec![Span::styled(" sağlayıcı ", theme::muted())];
    let mut used = 0usize;
    for (i, p) in crate::api::providers::PROVIDERS.iter().enumerate() {
        let active = i == selection.provider_index;
        let label = format!("[{}] ", p.id);
        if used + label.len() > bar_budget {
            prov_spans.push(Span::styled("...", theme::muted()));
            break;
        }
        used += label.len();
        if active {
            prov_spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme::Theme::PRIMARY)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        } else {
            prov_spans.push(Span::styled(label, theme::muted()));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(prov_spans)), chunks[2]);

    // ── Model şeridi — seçili sağlayıcının modelleri ──
    let provider = selection.provider();
    let mut model_spans: Vec<Span> = vec![Span::styled(
        format!(" {} ", provider.name),
        theme::secondary(),
    )];
    for (i, m) in provider.models.iter().enumerate() {
        let active = i == selection.model_index;
        let star = if m.free { "*" } else { " " };
        let label = format!(" {}{} ", m.id, star);
        if active {
            model_spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme::Theme::ACCENT)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        } else {
            model_spans.push(Span::styled(label, theme::muted()));
        }
    }
    model_spans.push(Span::styled(" (* = ücretsiz)", theme::muted()));
    frame.render_widget(Paragraph::new(Line::from(model_spans)), chunks[3]);

    // ── Giriş kutusu ──
    let input_lines = vec![
        Line::from(Span::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            theme::muted(),
        )),
        Line::from(vec![
            Span::styled(" > ", theme::primary()),
            Span::styled(input, theme::text()),
            Span::styled("▌", theme::accent()),
        ]),
    ];
    frame.render_widget(Paragraph::new(input_lines), chunks[2]);
}

