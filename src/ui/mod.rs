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

    loop {
        terminal.draw(|frame| draw_main(frame, &messages, &input))?;

        // Kullanıcı girdisini bekle (100ms timeout — animasyonlar için)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        break
                    }
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

/// Ana arayüz: gradient başlık + sohbet alanı + giriş kutusu
fn draw_main(frame: &mut Frame, messages: &[(bool, String)], input: &str) {
    let area = frame.area();

    // Arka plan
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::Theme::BG)),
        area,
    );

    // Dikey yerleşim: başlık / (sohbet + masajot paneli) / giriş
    let chunks = Layout::vertical([
        Constraint::Length(3), // başlık
        Constraint::Min(8),    // sohbet + panel
        Constraint::Length(3), // giriş
    ])
    .split(area);

    // Orta bölge: sola sohbet, sağa maskot paneli (terminal genişse)
    let show_sidebar = area.width >= 60;
    let main_chunks = if show_sidebar {
        Layout::horizontal([Constraint::Min(30), Constraint::Length(26)]).split(chunks[1])
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
    let mut header_lines: Vec<Line> = vec![Line::from(bar_spans)];
    header_lines.push(Line::from(vec![
        Span::styled(" ◆ elytra ", theme::title()),
        Span::styled("| model: ", theme::muted()),
        Span::styled("bağlanmadı", theme::secondary()),
        Span::styled(" | çıkmak için: Esc", theme::muted()),
    ]));
    frame.render_widget(Paragraph::new(header_lines), chunks[0]);

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
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        main_chunks[0],
    );

    // ── Masajot paneli (sağ kenar) ──
    if show_sidebar {
        let colors = mascot::MascotColors::default();
        let mut panel_lines: Vec<Line> = Vec::new();

        // Panel başlığı — ince ayırıcı
        panel_lines.push(Line::from(Span::styled(
            "─".repeat(24),
            theme::muted(),
        )));
        panel_lines.push(Line::from(""));

        // Maskot
        panel_lines.extend(mascot::mascot_lines(&colors));
        panel_lines.push(Line::from(""));

        // Bilgi satırları
        panel_lines.push(mascot::mascot_status(false));
        panel_lines.push(Line::from(Span::styled(
            "sürüm: 0.1.0",
            theme::muted(),
        )));
        panel_lines.push(Line::from(""));
        panel_lines.push(Line::from(Span::styled(
            "ipuçları:",
            theme::secondary(),
        )));
        panel_lines.push(Line::from(Span::styled(
            "  Esc    çıkış",
            theme::muted(),
        )));
        panel_lines.push(Line::from(Span::styled(
            "  Enter  gönder",
            theme::muted(),
        )));
        panel_lines.push(Line::from(Span::styled(
            "  model: API sonraki",
            theme::muted(),
        )));
        panel_lines.push(Line::from(Span::styled(
            "         sürümde",
            theme::muted(),
        )));

        frame.render_widget(Paragraph::new(panel_lines), main_chunks[1]);
    }

    // ── Giriş kutusu — kendi glifimiz ──
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

