//! Elytra TUI — OpenCode İlhamlı Terminal Kullanıcı Arayüzü
//!
//! Tasarım Özellikleri:
//! - Geniş ve ferah tam ekran sohbet düzeni (Gereksiz yan panel kalabalığı yok)
//! - OpenCode / Glitch Code tarzı dikey sol şeritli mesaj kartları (Border-Left)
//! - Sade ve modern karşılama başlığı
//! - Canlı /test sağlayıcı testi, 15 model sağlayıcısı, sıfır emoji
//! - Birleşik Model & Sağlayıcı Paleti (Split-pane Modal)
//! - Canlı Slash (/) Komut Tamamlama Menüsü (Floating Autocomplete Popup)

pub mod code_view;
pub mod diff_view;
pub mod file_tree;
pub mod mascot;
pub mod theme;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
    DefaultTerminal, Frame,
};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::agent::{AgentEvent, AgentSession};
use crate::api::providers::{Selection, PROVIDERS};
use crate::api::ApiClient;
use crate::config::AppConfig;
use mascot::{inline_wing_badge, welcome_banner, MascotMood};

/// TUI Mesaj Türleri
#[derive(Debug, Clone)]
pub enum UiMessage {
    User(String),
    Assistant(String),
    System(String),
    Tool(String),
}

/// Arka plan async iş parçacığından gelen olaylar
#[allow(dead_code)]
pub enum WorkerEvent {
    Token(String),
    Done,
    Error(String),
    AgentThought(String),
    AgentToolStart(String, String),
    AgentToolEnd(String, String),
    AgentFinished(String),
    /// Geçici hata sonrası yeniden deneme bildirimi
    AgentRetrying(String),
    TestReport(String),
}

/// Aktif Açılır Modal Menü (OpenCode Diyalogları)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    UnifiedModelPicker {
        provider_idx: usize,
        model_idx: usize,
        focus_models: bool, // false: Sağlayıcılar (Sol), true: Modeller (Sağ)
    },
    CommandMenu {
        selected_idx: usize,
    },
}

/// Slash (/) Komut Tanımı
#[derive(Debug, Clone)]
pub struct SlashCommandDef {
    pub name: &'static str,
    pub args_hint: &'static str,
    pub desc: &'static str,
}

/// Canlı autocomplete listesinde yer alan sistem komutları
pub const SLASH_COMMANDS: &[SlashCommandDef] = &[
    SlashCommandDef {
        name: "/model",
        args_hint: "[model-adi]",
        desc: "Model ve saglayici paletini ac veya model sec",
    },
    SlashCommandDef {
        name: "/agent",
        args_hint: "<gorev>",
        desc: "15 dahili aracla otonom kod ajanini calistir",
    },
    SlashCommandDef {
        name: "/tools",
        args_hint: "",
        desc: "15 dahili kod ve terminal aracini listele",
    },
    SlashCommandDef {
        name: "/test",
        args_hint: "[saglayici|all] [--full|--keys]",
        desc: "15 saglayiciyi paralel tara (iki kademeli: baglanti + chat)",
    },
    SlashCommandDef {
        name: "/key",
        args_hint: "<saglayici> <key>",
        desc: "API anahtarini guvenle kaydet",
    },
    SlashCommandDef {
        name: "/config",
        args_hint: "",
        desc: "Mevcut ayarlari ve anahtar durumlarini listele",
    },
    SlashCommandDef {
        name: "/clear",
        args_hint: "",
        desc: "Sohbet gecmisini ve ekrani temizle",
    },
    SlashCommandDef {
        name: "/help",
        args_hint: "",
        desc: "Kisayollar, komutlar ve kullanim kilavuzunu goruntule",
    },
    SlashCommandDef {
        name: "/exit",
        args_hint: "",
        desc: "Elytra terminal uygulamasindan ayril",
    },
];

/// TUI'yi başlat
pub async fn run(initial_model: Option<&str>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = ratatui::init();

    let result = boot_and_loop(terminal, initial_model).await;

    ratatui::restore();
    let _ = disable_raw_mode();
    let _ = execute!(stdout, LeaveAlternateScreen);

    result
}

async fn boot_and_loop(mut terminal: DefaultTerminal, initial_model: Option<&str>) -> Result<()> {
    let mut config = AppConfig::load();
    let mut selection = Selection::default();

    if let Some(m) = initial_model {
        selection.set_by_name(m);
    } else if let Some(ref def_m) = config.default_model {
        selection.set_by_name(def_m);
    } else if let Some(ref def_p) = config.default_provider {
        selection.set_provider_by_id(def_p);
    } else {
        // Önceden anahtarı olan sağlayıcıyı seç
        if config.openrouter_api_key.is_some() {
            selection.set_provider_by_id("openrouter");
        } else if config.groq_api_key.is_some() {
            selection.set_provider_by_id("groq");
        } else if config.gemini_api_key.is_some() {
            selection.set_provider_by_id("google");
        } else if config.deepseek_api_key.is_some() {
            selection.set_provider_by_id("deepseek");
        } else if config.openai_api_key.is_some() {
            selection.set_provider_by_id("openai");
        } else {
            // Anahtarsız ortamda katalogdaki ilk sağlayıcı (OpenRouter) seçilir;
            // kullanıcıya anahtar gerekli mesajı gösterilir (sahte motor yok).
            selection.set_provider_by_id("openrouter");
        }
    }

    let mut input = String::new();
    let mut messages: Vec<UiMessage> = Vec::new();
    let mut scroll_offset: u16 = 0;
    let mut active_modal = ActiveModal::None;
    let mut anim_frame: usize = 0;
    let mut slash_selected_idx: usize = 0;
    // Elytra IDE — dosya ağacı paneli (Ctrl+B ile açılır)
    let mut file_tree = file_tree::FileTree::from_cwd();
    // Elytra IDE — kod görüntüleyici paneli (Ctrl+E ile açılır)
    let mut code_view = code_view::CodeView::new();
    // Elytra IDE — diff görünümü paneli (Ctrl+D ile açılır)
    let mut diff_view = diff_view::DiffView::new();

    let (worker_tx, mut worker_rx) = mpsc::channel::<WorkerEvent>(100);
    let mut is_streaming = false;

    loop {
        // Arka plan olaylarını tüket
        while let Ok(event) = worker_rx.try_recv() {
            match event {
                WorkerEvent::Token(token) => {
                    if let Some(UiMessage::Assistant(last)) = messages.last_mut() {
                        last.push_str(&token);
                    } else {
                        messages.push(UiMessage::Assistant(token));
                    }
                }
                WorkerEvent::Done => {
                    is_streaming = false;
                }
                WorkerEvent::Error(err) => {
                    is_streaming = false;
                    messages.push(UiMessage::System(format!(
                        "[X] API Hatasi: {err}\nIpucu: /key <saglayici> <anahtar> ile anahtarinizi girin veya /test calistirin."
                    )));
                }
                WorkerEvent::AgentThought(th) => {
                    messages.push(UiMessage::System(format!("◆ Dusunce: {th}")));
                }
                WorkerEvent::AgentToolStart(name, args) => {
                    messages.push(UiMessage::Tool(format!("Calistiriliyor: {name}({args})")));
                }
                WorkerEvent::AgentToolEnd(name, res) => {
                    let preview = crate::util::truncate_chars(&res, 240);
                    messages.push(UiMessage::Tool(format!("{name} tamamlandi:\n{preview}")));
                }
                WorkerEvent::AgentFinished(ans) => {
                    is_streaming = false;
                    let should_push = match messages.last() {
                        Some(UiMessage::Assistant(last)) => last != &ans,
                        _ => true,
                    };
                    if should_push {
                        messages.push(UiMessage::Assistant(ans));
                    }
                }
                WorkerEvent::TestReport(report) => {
                    messages.push(UiMessage::System(report));
                }
                WorkerEvent::AgentRetrying(notice) => {
                    messages.push(UiMessage::System(notice));
                }
            }
        }

        anim_frame = anim_frame.wrapping_add(1);
        let mood = if is_streaming {
            MascotMood::Thinking(anim_frame)
        } else {
            MascotMood::Idle(anim_frame / 6)
        };

        // Render
        let current_input = input.clone();
        terminal.draw(|frame| {
            draw_main(
                frame,
                &messages,
                &current_input,
                &selection,
                mood,
                scroll_offset,
                &active_modal,
                &config,
                slash_selected_idx,
                &file_tree,
                &code_view,
                &diff_view,
            );
        })?;

        // Girdi dinleme
        let poll_dur = if is_streaming { Duration::from_millis(25) } else { Duration::from_millis(60) };
        if event::poll(poll_dur)?
            && let Event::Key(key) = event::read()? {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }

                // Modal açıkken tuşlar
                if active_modal != ActiveModal::None {
                    match key.code {
                        KeyCode::Esc => {
                            active_modal = ActiveModal::None;
                        }
                        KeyCode::Left => {
                            if let ActiveModal::UnifiedModelPicker { ref mut focus_models, .. } = active_modal {
                                *focus_models = false;
                            }
                        }
                        KeyCode::Right => {
                            if let ActiveModal::UnifiedModelPicker { ref mut focus_models, .. } = active_modal {
                                *focus_models = true;
                            }
                        }
                        KeyCode::Tab | KeyCode::BackTab => {
                            if let ActiveModal::UnifiedModelPicker { ref mut focus_models, .. } = active_modal {
                                *focus_models = !*focus_models;
                            }
                        }
                        KeyCode::Up => {
                            match active_modal {
                                ActiveModal::UnifiedModelPicker {
                                    ref mut provider_idx,
                                    ref mut model_idx,
                                    focus_models,
                                } => {
                                    if !focus_models {
                                        *provider_idx = provider_idx.saturating_sub(1);
                                        *model_idx = 0;
                                    } else {
                                        *model_idx = model_idx.saturating_sub(1);
                                    }
                                }
                                ActiveModal::CommandMenu { ref mut selected_idx } => {
                                    *selected_idx = selected_idx.saturating_sub(1);
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Down => {
                            match active_modal {
                                ActiveModal::UnifiedModelPicker {
                                    ref mut provider_idx,
                                    ref mut model_idx,
                                    focus_models,
                                } => {
                                    if !focus_models {
                                        if *provider_idx + 1 < PROVIDERS.len() {
                                            *provider_idx += 1;
                                            *model_idx = 0;
                                        }
                                    } else {
                                        let max = PROVIDERS[*provider_idx].models.len();
                                        if *model_idx + 1 < max {
                                            *model_idx += 1;
                                        }
                                    }
                                }
                                ActiveModal::CommandMenu { ref mut selected_idx } => {
                                    let max = 10;
                                    if *selected_idx + 1 < max {
                                        *selected_idx += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Enter => {
                            match active_modal {
                                ActiveModal::UnifiedModelPicker {
                                    provider_idx,
                                    model_idx,
                                    focus_models,
                                } => {
                                    if !focus_models {
                                        // Sol paneldeyken Enter'a basınca sağ panele (modellere) odaklan
                                        active_modal = ActiveModal::UnifiedModelPicker {
                                            provider_idx,
                                            model_idx: 0,
                                            focus_models: true,
                                        };
                                    } else {
                                        // Sağ paneldeyken Enter'a basınca hem sağlayıcıyı hem modeli seç ve onayla!
                                        if provider_idx < PROVIDERS.len() {
                                            selection.provider_index = provider_idx;
                                            let models = PROVIDERS[provider_idx].models;
                                            if model_idx < models.len() {
                                                selection.model_index = model_idx;
                                            } else {
                                                selection.model_index = 0;
                                            }
                                            selection.custom_model = None;
                                            messages.push(UiMessage::System(format!(
                                                "✓ Model secildi: {} > {} ({})",
                                                selection.provider().name,
                                                selection.model().name,
                                                selection.model_id()
                                            )));
                                        }
                                        active_modal = ActiveModal::None;
                                    }
                                }
                                ActiveModal::CommandMenu { selected_idx } => {
                                    active_modal = ActiveModal::None;
                                    match selected_idx {
                                        0 => {
                                            active_modal = ActiveModal::UnifiedModelPicker {
                                                provider_idx: selection.provider_index,
                                                model_idx: selection.model_index,
                                                focus_models: true,
                                            };
                                        }
                                        1 => run_provider_test(selection.provider().id, &config, worker_tx.clone()),
                                        2 => run_all_providers_test(&config, worker_tx.clone()),
                                        3 => messages.push(UiMessage::System(
                                            "Anahtar eklemek icin: /key <saglayici> <anahtar> (Orn: /key groq gsk_...)".into(),
                                        )),
                                        4 => messages.push(UiMessage::System(
                                            "Otonom ajan baslatmak icin: /agent <gorev metni>".into(),
                                        )),
                                        5 => {
                                            let tools = crate::tools::get_available_tools();
                                            let mut out = String::from(
                                                "▌ DAHILI AJAN ARACLARI (15 Dahili Arac - Glitch Code / Claude Code Seviyesi)\n",
                                            );
                                            for (idx, t) in tools.iter().enumerate() {
                                                out.push_str(&format!("  {:>2}. {:<14} : {}\n", idx + 1, t.name, t.description));
                                            }
                                            messages.push(UiMessage::System(out));
                                        }
                                        6 => print_config_status(&selection, &config, &mut messages),
                                        7 => print_help_guide(&mut messages),
                                        8 => {
                                            messages.clear();
                                            messages.push(UiMessage::System("Sohbet gecmisi temizlendi.".into()));
                                        }
                                        9 => std::process::exit(0),
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Canlı Slash Menüsü Kontrolü (Kullanıcı '/' ile başlayan bir komut yazıyorsa)
                let is_slash_query = input.starts_with('/') && !input.contains(' ');
                let slash_matches: Vec<&SlashCommandDef> = if is_slash_query {
                    let q = input.to_lowercase();
                    SLASH_COMMANDS
                        .iter()
                        .filter(|c| c.name.starts_with(&q) || c.name.contains(&q))
                        .collect()
                } else {
                    Vec::new()
                };

                if is_slash_query && !slash_matches.is_empty() {
                    match key.code {
                        KeyCode::Up => {
                            slash_selected_idx = slash_selected_idx.saturating_sub(1);
                            continue;
                        }
                        KeyCode::Down => {
                            if slash_selected_idx + 1 < slash_matches.len() {
                                slash_selected_idx += 1;
                            }
                            continue;
                        }
                        KeyCode::Tab => {
                            let chosen = slash_matches[slash_selected_idx.min(slash_matches.len() - 1)];
                            if chosen.args_hint.is_empty() {
                                input = chosen.name.to_string();
                            } else {
                                input = format!("{} ", chosen.name);
                            }
                            slash_selected_idx = 0;
                            continue;
                        }
                        _ => {}
                    }
                }

                // Normal Sohbet Tuşları
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,

                    // ── ELYTRA IDE: dosya ağacı + kod görüntüleyici ──
                    // Ctrl+B ağacı, Ctrl+E kodu açar/kapatır.
                    KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        file_tree.toggle();
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        code_view.toggle();
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Seçili dosya varsa onun diff'i, yoksa tüm değişiklikler
                        let secili = file_tree.secili_dosya();
                        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                        diff_view.yukle(secili.as_deref(), &cwd);
                    }
                    // Diff paneli açıkken onu kaydır (en üstte — odak diff'te)
                    KeyCode::Up if diff_view.open => diff_view.yukari(),
                    KeyCode::Down if diff_view.open => diff_view.asagi(),
                    KeyCode::PageUp if diff_view.open => diff_view.sayfa_yukari(20),
                    KeyCode::PageDown if diff_view.open => diff_view.sayfa_asagi(20),
                    // Kod paneli açıkken ok/Page tuşları onu kaydırır (odak kodda)
                    KeyCode::Up if code_view.open => code_view.yukari(),
                    KeyCode::Down if code_view.open => code_view.asagi(),
                    KeyCode::PageUp if code_view.open => code_view.sayfa_yukari(20),
                    KeyCode::PageDown if code_view.open => code_view.sayfa_asagi(20),
                    KeyCode::Up if file_tree.open => file_tree.yukari(),
                    KeyCode::Down if file_tree.open => file_tree.asagi(),
                    KeyCode::Enter if file_tree.open && input.is_empty() => {
                        // Klasörde aç/kapa; dosyada KOD GÖRÜNTÜLEYİCİYE yükle
                        if let Some(yol) = file_tree.secili_dosya() {
                            code_view.yukle(&yol);
                        } else {
                            file_tree.ac_kapa();
                        }
                    }
                    KeyCode::Left if file_tree.open => file_tree.ust_dizine_cik(),

                    KeyCode::F(1) => {
                        active_modal = ActiveModal::CommandMenu { selected_idx: 0 };
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        active_modal = ActiveModal::UnifiedModelPicker {
                            provider_idx: selection.provider_index,
                            model_idx: selection.model_index,
                            focus_models: true,
                        };
                    }

                    KeyCode::Up => scroll_offset = scroll_offset.saturating_add(1),
                    KeyCode::Down => scroll_offset = scroll_offset.saturating_sub(1),
                    KeyCode::PageUp => scroll_offset = scroll_offset.saturating_add(8),
                    KeyCode::PageDown => scroll_offset = scroll_offset.saturating_sub(8),

                    KeyCode::Char(c) => {
                        input.push(c);
                        if input.starts_with('/') {
                            slash_selected_idx = 0;
                        }
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        if input.starts_with('/') {
                            slash_selected_idx = 0;
                        }
                    }

                    KeyCode::Enter => {
                        let trimmed = input.trim().to_string();
                        if !trimmed.is_empty() {
                            input.clear();
                            scroll_offset = 0;
                            slash_selected_idx = 0;

                            if trimmed.starts_with('/') {
                                handle_slash_command(
                                    &trimmed,
                                    &mut messages,
                                    &mut selection,
                                    &mut config,
                                    worker_tx.clone(),
                                    &mut is_streaming,
                                    &mut active_modal,
                                );
                            } else {
                                messages.push(UiMessage::User(trimmed.clone()));

                                let prov_id = selection.provider().id.to_string();
                                let has_key = config.get_key_for(&prov_id).is_some();
                                let free_tier = selection.provider().is_free_tier;

                                if !free_tier && !has_key {
                                    messages.push(UiMessage::System(format!(
                                        "[!] [{} ({})] icin henuz API anahtari eklenmemis!\n\
                                         Anahtar eklemek icin: /key {} <api_anahtariniz>",
                                        selection.provider().name,
                                        prov_id,
                                        prov_id
                                    )));
                                    continue;
                                }

                                if !is_streaming {
                                    is_streaming = true;

                                    let tx = worker_tx.clone();
                                    let mod_id = selection.model_id();
                                    let prov_id_clone = prov_id.clone();
                                    let cfg_clone = config.clone();

                                    tokio::spawn(async move {
                                        let mut session = AgentSession::new(&prov_id_clone, &mod_id, cfg_clone);
                                        let tx_clone = tx.clone();

                                        let run_res = session
                                            .run(&trimmed, move |event| {
                                                match event {
                                                    AgentEvent::Thought(th) => {
                                                        let _ = tx_clone.try_send(WorkerEvent::AgentThought(th));
                                                    }
                                                    AgentEvent::ToolExecuting { name, args } => {
                                                        let _ = tx_clone.try_send(WorkerEvent::AgentToolStart(name, args));
                                                    }
                                                    AgentEvent::ToolFinished { name, result } => {
                                                        let _ = tx_clone.try_send(WorkerEvent::AgentToolEnd(name, result));
                                                    }
                                                    AgentEvent::FinalAnswer(ans) => {
                                                        let _ = tx_clone.try_send(WorkerEvent::AgentFinished(ans));
                                                    }
                                                    AgentEvent::Error(err) => {
                                                        let _ = tx_clone.try_send(WorkerEvent::Error(err));
                                                    }
                                                    AgentEvent::StepLimitReached => {
                                                        let _ = tx_clone
                                                            .try_send(WorkerEvent::Error("Ajan adim limitine ulasti".into()));
                                                    }
                                                    AgentEvent::Retrying { attempt, reason } => {
                                                        let _ = tx_clone.try_send(WorkerEvent::AgentRetrying(
                                                            format!("[!] Gecici hata — yeniden deneniyor ({attempt}. deneme): {reason}"),
                                                        ));
                                                    }
                                                }
                                            })
                                            .await;

                                        if let Err(e) = run_res {
                                            let _ = tx.send(WorkerEvent::Error(format!("{e}"))).await;
                                        }

                                        let _ = tx.send(WorkerEvent::Done).await;
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
    }

    Ok(())
}

fn run_provider_test(prov_id: &str, config: &AppConfig, tx: mpsc::Sender<WorkerEvent>) {
    let cfg = config.clone();
    let p_str = prov_id.to_string();
    tokio::spawn(async move {
        let client = ApiClient::new(cfg);
        let mode = crate::scan::ScanMode::Connectivity;
        let options = crate::scan::ScanOptions {
            mode,
            ..Default::default()
        };
        let msg = if let Some(pv) = crate::api::PROVIDERS
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&p_str))
        {
            let res = crate::scan::scan_single(&client, pv, &options).await;
            format!(
                "▌ SAGLAYICI TARAMASI [{}]\n  {}",
                mode.label(),
                res.report_line()
            )
        } else {
            let legacy = client.test_provider(&p_str).await;
            format!(
                "▌ SAGLAYICI TARAMASI\n  [{}] {} ({}) : {}ms — {}",
                if legacy.success { "OK" } else { "FAIL" },
                legacy.provider_name,
                legacy.provider_id,
                legacy.latency_ms,
                legacy.message
            )
        };
        let _ = tx.send(WorkerEvent::TestReport(msg)).await;
    });
}

fn run_all_providers_test(config: &AppConfig, tx: mpsc::Sender<WorkerEvent>) {
    let cfg = config.clone();
    tokio::spawn(async move {
        let client = ApiClient::new(cfg);
        let options = crate::scan::ScanOptions::default();
        let mode = options.mode;
        let tx2 = tx.clone();
        let _ = tx
            .send(WorkerEvent::TestReport(format!(
                "▌ SAGLAYICI TARAMASI BASLADI [{}] — sonuclar hazir oldukca akar...",
                mode.label()
            )))
            .await;

        let summary = crate::scan::scan_all_providers(&client, &options, |r| {
            let _ = tx2.try_send(WorkerEvent::TestReport(format!("  {}", r.report_line())));
        })
        .await;

        let _ = tx
            .send(WorkerEvent::TestReport(format!(
                "▌ TARAMA TAMAMLANDI\n{}",
                summary.format_report(mode)
            )))
            .await;
    });
}

fn print_help_guide(messages: &mut Vec<UiMessage>) {
    let guide = "\
▌ ELYTRA KULLANIM REHBERI & KOMUT KILAVUZU
  ◆ Hizli Kisayollar:
    Tab          : Model ve Saglayici Paleti (15 Saglayici + 60+ Model)
    F1           : Hizli Komut Menusu (Tum sistem eylemleri)
    /            : Canli Komut Tamamlama (Yazarken otomatik acilir)
    Yukari/Asagi : Sohbet kaydirma veya komut secimi
    Ctrl+C / Esc : Elytra'dan cikis

  ◆ Slash (/) Komutlari:
    /model [ad]               : Model & Saglayici paletini acar veya dogrudan secer
    /agent <gorev>            : 15 dahili aracla otonom ReAct kod ajanini baslatir
    /tools                    : 15 dahili kod, git, web ve terminal aracini listeler
    /test [saglayici|all]     : API baglantilarini ve yanit gecikme surelerini sinamasini yapar
    /key <saglayici> <anahtar>: API anahtarini guvenli sekilde kaydeder
    /config                   : Aktif ayarlari ve saglayicilarin anahtar durumunu listeler
    /clear                    : Sohbet gecmisini ve ekrani temizler
    /help                     : Bu yardim kilavuzunu gosterir
    /exit                     : Terminalden guvenli cikis yapar

  ◆ Desteklenen Model Saglayicileri:
    OpenRouter, Nous Research (ucretsiz katman), Groq, Google Gemini,
    Cerebras, DeepSeek, Mistral, Z.ai, Moonshot, xAI (Grok), OpenAI,
    Anthropic, Together AI, Fireworks, NVIDIA.";
    messages.push(UiMessage::System(guide.to_string()));
}

fn print_config_status(selection: &Selection, config: &AppConfig, messages: &mut Vec<UiMessage>) {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "workspace".into());

    let mut out = format!(
        "▌ ELYTRA YAPILANDIRMASI & BAGLANTI DURUMU\n\
         Aktif Saglayici : {} ({})\n\
         Aktif Model     : {} ({})\n\
         Calisma Dizini  : {}\n\
         Dahili Araclar  : 15 Kod & Terminal Araci Aktif\n\n\
         ◆ Saglayici Anahtar Durumlari:\n",
        selection.provider().name,
        selection.provider().id,
        selection.model().name,
        selection.model().id,
        cwd
    );

    // Sağlayıcı listesi artık KATALOGDAN türetilir (elle kopya yok, sahte giriş yok).
    for p in PROVIDERS {
        let has_key = config.get_key_for(p.id).is_some();
        let badge = if p.is_free_tier {
            "[Ucretsiz]"
        } else if has_key {
            "[✓ Bagli] "
        } else {
            "[Key Yok] "
        };
        let hint = if !has_key && !p.is_free_tier {
            format!("(Ayarla: /key {} <anahtar>)", p.id)
        } else {
            String::new()
        };
        out.push_str(&format!("  {} {:<18} {}\n", badge, p.name, hint));
    }

    messages.push(UiMessage::System(out));
}

fn handle_slash_command(
    cmd: &str,
    messages: &mut Vec<UiMessage>,
    selection: &mut Selection,
    config: &mut AppConfig,
    tx: mpsc::Sender<WorkerEvent>,
    is_streaming: &mut bool,
    active_modal: &mut ActiveModal,
) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let base = parts[0].to_lowercase();

    match base.as_str() {
        "/menu" => {
            *active_modal = ActiveModal::CommandMenu { selected_idx: 0 };
        }
        "/help" => {
            print_help_guide(messages);
        }
        "/config" | "/status" => {
            print_config_status(selection, config, messages);
        }
        "/models" | "/model" => {
            if parts.len() > 1 {
                let target = parts[1..].join(" ");
                selection.set_by_name(&target);
                messages.push(UiMessage::System(format!(
                    "✓ Model secildi: {} > {} ({})",
                    selection.provider().name,
                    selection.model_name(),
                    selection.model_id()
                )));
            } else {
                *active_modal = ActiveModal::UnifiedModelPicker {
                    provider_idx: selection.provider_index,
                    model_idx: selection.model_index,
                    focus_models: true,
                };
            }
        }
        "/providers" | "/provider" => {
            if parts.len() > 1 {
                let target = parts[1].to_lowercase();
                if selection.set_provider_by_id(&target) {
                    messages.push(UiMessage::System(format!(
                        "✓ Saglayici degistirildi: {} > {}",
                        selection.provider().name,
                        selection.model_name()
                    )));
                } else {
                    messages.push(UiMessage::System(format!("Saglayici '{}' bulunamadi.", parts[1])));
                }
            } else {
                *active_modal = ActiveModal::UnifiedModelPicker {
                    provider_idx: selection.provider_index,
                    model_idx: 0,
                    focus_models: false,
                };
            }
        }
        "/test" => {
            if parts.len() > 1 && parts[1].to_lowercase() == "all" {
                messages.push(UiMessage::System("Tum saglayicilar test ediliyor, lutfen bekleyin...".into()));
                run_all_providers_test(config, tx);
            } else if parts.len() > 1 {
                let target = parts[1].to_lowercase();
                messages.push(UiMessage::System(format!("'{}' test ediliyor...", target)));
                run_provider_test(&target, config, tx);
            } else {
                let current_id = selection.provider().id;
                messages.push(UiMessage::System(format!("Aktif saglayici '{}' test ediliyor...", current_id)));
                run_provider_test(current_id, config, tx);
            }
        }
        "/key" | "/setkey" => {
            if parts.len() < 3 {
                messages.push(UiMessage::System(
                    "Kullanim: /key <saglayici> <api_anahtari> (Orn: /key groq gsk_...)".into(),
                ));
                return;
            }
            let prov = parts[1].to_lowercase();
            let key_val = parts[2].to_string();

            match prov.as_str() {
                "groq" => config.groq_api_key = Some(key_val),
                "google" | "gemini" => config.gemini_api_key = Some(key_val),
                "openrouter" => config.openrouter_api_key = Some(key_val),
                "cerebras" => config.cerebras_api_key = Some(key_val),
                "sambanova" => config.sambanova_api_key = Some(key_val),
                "mistral" => config.mistral_api_key = Some(key_val),
                "deepseek" => config.deepseek_api_key = Some(key_val),
                "openai" => config.openai_api_key = Some(key_val),
                "anthropic" => config.anthropic_api_key = Some(key_val),
                "together" => config.together_api_key = Some(key_val),
                "perplexity" => config.perplexity_api_key = Some(key_val),
                "cohere" => config.cohere_api_key = Some(key_val),
                "nous" => config.nous_api_key = Some(key_val),
                "grok" | "xai" => config.grok_api_key = Some(key_val),
                _ => {
                    messages.push(UiMessage::System(format!("Bilinmeyen saglayici: '{}'", prov)));
                    return;
                }
            }

            let _ = config.save_global();
            selection.set_provider_by_id(&prov);

            messages.push(UiMessage::System(format!(
                "✓ {} API anahtari basariyla kaydedildi! Baglantiyi test etmek icin: /test",
                prov
            )));
        }
        "/clear" => {
            messages.clear();
        }
        "/new" | "/reset" => {
            messages.clear();
            messages.push(UiMessage::System("Yeni oturum baslatildi.".into()));
        }
        "/agent" | "/task" => {
            if parts.len() < 2 {
                messages.push(UiMessage::System("Kullanim: /agent <gorev metni>".into()));
                return;
            }
            let task = parts[1..].join(" ");
            messages.push(UiMessage::User(format!("[Ajan Gorevi]: {}", task)));
            *is_streaming = true;

            let prov_id = selection.provider().id.to_string();
            let mod_id = selection.model_id();
            let cfg = config.clone();

            tokio::spawn(async move {
                let mut agent = AgentSession::new(&prov_id, &mod_id, cfg);
                let tx_clone = tx.clone();

                let run_res = agent
                    .run(&task, move |event| {
                        match event {
                            AgentEvent::Thought(th) => {
                                let _ = tx_clone.try_send(WorkerEvent::AgentThought(th));
                            }
                            AgentEvent::ToolExecuting { name, args } => {
                                let _ = tx_clone.try_send(WorkerEvent::AgentToolStart(name, args));
                            }
                            AgentEvent::ToolFinished { name, result } => {
                                let _ = tx_clone.try_send(WorkerEvent::AgentToolEnd(name, result));
                            }
                            AgentEvent::FinalAnswer(ans) => {
                                let _ = tx_clone.try_send(WorkerEvent::AgentFinished(ans));
                            }
                            AgentEvent::Error(err) => {
                                let _ = tx_clone.try_send(WorkerEvent::Error(err));
                            }
                            AgentEvent::StepLimitReached => {
                                let _ = tx_clone.try_send(WorkerEvent::Error("Ajan adim limitine ulasti".into()));
                            }
                            AgentEvent::Retrying { attempt, reason } => {
                                let _ = tx_clone.try_send(WorkerEvent::AgentRetrying(
                                    format!("[!] Gecici hata — yeniden deneniyor ({attempt}. deneme): {reason}"),
                                ));
                            }
                        }
                    })
                    .await;

                if let Err(e) = run_res {
                    let _ = tx.send(WorkerEvent::Error(format!("{e}"))).await;
                }

                let _ = tx.send(WorkerEvent::Done).await;
            });
        }
        "/tools" => {
            let tools = crate::tools::get_available_tools();
            let mut out = String::from(
                "▌ DAHILI AJAN ARACLARI (15 Dahili Arac - Glitch Code / Claude Code Seviyesi)\n",
            );
            for (idx, t) in tools.iter().enumerate() {
                out.push_str(&format!("  {:>2}. {:<14} : {}\n", idx + 1, t.name, t.description));
            }
            messages.push(UiMessage::System(out));
        }
        "/exit" | "/quit" => {
            std::process::exit(0);
        }
        _ => {
            messages.push(UiMessage::System(format!(
                "Bilinmeyen komut: '{}'. Tum komutlar icin /help yazabilir veya F1'e basabilirsiniz.",
                cmd
            )));
        }
    }
}

// ─────────────────────── Çizim Fonksiyonları ───────────────────────

/// OpenCode Tarzı Ferah ve Modern Ana Arayüz Düzeni
#[allow(clippy::too_many_arguments)]
fn draw_main(
    frame: &mut Frame,
    messages: &[UiMessage],
    input: &str,
    selection: &Selection,
    mood: MascotMood,
    scroll_offset: u16,
    active_modal: &ActiveModal,
    config: &AppConfig,
    slash_selected_idx: usize,
    file_tree: &file_tree::FileTree,
    code_view: &code_view::CodeView,
    diff_view: &diff_view::DiffView,
) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(theme::Theme::BG)), area);

    // ── ELYTRA IDE: sol dosya ağacı paneli (Ctrl+B) ──
    // Panel açıksa içerik alanı sağa kaydırılır.
    let icerik_alani = if file_tree.open {
        let yatay = Layout::horizontal([Constraint::Length(32), Constraint::Min(20)]).split(area);
        render_file_tree(frame, yatay[0], file_tree);
        yatay[1]
    } else {
        area
    };
    // ── ELYTRA IDE: kod görüntüleyici (Ctrl+E) ──
    // Açıkken içerik alanı [kod | sohbet] olarak bölünür.
    let area = if diff_view.open {
        let yatay = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(icerik_alani);
        render_diff_view(frame, yatay[0], diff_view);
        yatay[1]
    } else if code_view.open {
        let yatay = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(icerik_alani);
        render_code_view(frame, yatay[0], code_view);
        yatay[1]
    } else {
        icerik_alani
    };

    // Dikey Yerleşim:
    // 0. Neon Accent Çizgisi (1)
    // 1. Üst Çubuk (1)
    // 2. Sohbet & İçerik Alanı (Min 6) - TAM GENİŞLİK
    // 3. Giriş Kutusu (3)
    // 4. Alt Kısayol İpuçları (1)
    let chunks = Layout::vertical([
        Constraint::Length(1), // Neon accent
        Constraint::Length(1), // Top Bar
        Constraint::Min(6),    // Chat Area
        Constraint::Length(3), // Input Bar
        Constraint::Length(1), // Hotkeys Hint Bar
    ])
    .split(area);

    // ── 0. Neon Accent Çizgisi ──
    let accent_line = Line::from(Span::styled("━".repeat(area.width as usize), Style::default().fg(theme::Theme::PRIMARY)));
    frame.render_widget(Paragraph::new(accent_line), chunks[0]);

    // ── 1. Üst Bar (OpenCode Top Bar) ──
    let cwd = std::env::current_dir()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
        .unwrap_or_else(|_| "workspace".into());

    let status_dot = match mood {
        MascotMood::Thinking(_) => Span::styled("● ", Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)),
        MascotMood::Error => Span::styled("● ", Style::default().fg(theme::Theme::ERROR).add_modifier(Modifier::BOLD)),
        _ => Span::styled("● ", Style::default().fg(theme::Theme::SUCCESS)),
    };

    let top_bar_line = Line::from(vec![
        Span::styled(" ◆ ELYTRA", Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
        inline_wing_badge(mood),
        status_dot,
        Span::styled(format!("{} ", selection.full_id()), Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("· ~/{} ", cwd), theme::muted()),
        Span::styled("· 15 Arac ", theme::muted()),
        Span::styled("[Tab: Model & Saglayici  /: Komutlar  F1: Menu] ", theme::secondary()),
    ]);
    frame.render_widget(Paragraph::new(top_bar_line), chunks[1]);

    // ── 2. Sohbet & İçerik Alanı (OpenCode Left-Border ChatBubble Tasarımı) ──
    let mut chat_lines: Vec<Line> = Vec::new();

    if messages.is_empty() {
        chat_lines.extend(welcome_banner(selection.provider().name, &selection.model_name()));
    } else {
        chat_lines.push(Line::from(""));
        for msg in messages {
            match msg {
                UiMessage::User(text) => {
                    chat_lines.push(Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(theme::Theme::USER).add_modifier(Modifier::BOLD)),
                        Span::styled("You", Style::default().fg(theme::Theme::USER).add_modifier(Modifier::BOLD)),
                    ]));
                    for l in text.lines() {
                        chat_lines.push(Line::from(vec![
                            Span::styled("  ", theme::muted()),
                            Span::styled(l.to_string(), theme::text()),
                        ]));
                    }
                    chat_lines.push(Line::from(""));
                }
                UiMessage::Assistant(text) => {
                    chat_lines.push(Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
                        Span::styled("Elytra ", Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("· {}", selection.model_name()), theme::muted()),
                    ]));
                    for l in text.lines() {
                        chat_lines.push(Line::from(vec![
                            Span::styled("  ", theme::muted()),
                            Span::styled(l.to_string(), theme::assistant()),
                        ]));
                    }
                    chat_lines.push(Line::from(""));
                }
                UiMessage::System(text) => {
                    chat_lines.push(Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(theme::Theme::SECONDARY).add_modifier(Modifier::BOLD)),
                        Span::styled("System", Style::default().fg(theme::Theme::SECONDARY).add_modifier(Modifier::BOLD)),
                    ]));
                    for l in text.lines() {
                        chat_lines.push(Line::from(vec![
                            Span::styled("  ", theme::muted()),
                            Span::styled(l.to_string(), theme::muted()),
                        ]));
                    }
                    chat_lines.push(Line::from(""));
                }
                UiMessage::Tool(text) => {
                    let mut lines_iter = text.lines();
                    if let Some(first_line) = lines_iter.next() {
                        chat_lines.push(Line::from(vec![
                            Span::styled("▌ ", Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)),
                            Span::styled("Tool · ", Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)),
                            Span::styled(first_line.to_string(), theme::text()),
                        ]));
                        for l in lines_iter {
                            chat_lines.push(Line::from(vec![
                                Span::styled("  ", theme::muted()),
                                Span::styled(l.to_string(), theme::muted()),
                            ]));
                        }
                    }
                    chat_lines.push(Line::from(""));
                }
            }
        }
    }

    let chat_area = chunks[2];
    let total_lines = chat_lines.len() as u16;
    let visible_lines = chat_area.height;

    let scroll_y = if total_lines > visible_lines {
        let max_scroll = total_lines.saturating_sub(visible_lines);
        max_scroll.saturating_sub(scroll_offset)
    } else {
        0
    };

    frame.render_widget(Paragraph::new(chat_lines).scroll((scroll_y, 0)), chat_area);

    // ── 3. Giriş Çubuğu (OpenCode Prompt Box) ──
    let input_placeholder = if input.is_empty() {
        Span::styled(
            "Mesajinizi yazin veya / ile komut arayin... (Tab: Model Paleti, F1: Hizli Menu)",
            theme::muted(),
        )
    } else {
        Span::styled(input, theme::text())
    };

    let input_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::Theme::PRIMARY));

    let input_p = Paragraph::new(Line::from(vec![
        Span::styled(" ❯ ", Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
        input_placeholder,
        Span::styled("█", theme::accent()),
    ]));

    frame.render_widget(input_block.clone(), chunks[3]);
    frame.render_widget(input_p, input_block.inner(chunks[3]));

    // ── 4. Alt Kısayol İpuçları Çubuğu ──
    let hints_line = Line::from(vec![
        Span::styled("  [Enter] ", theme::primary()),
        Span::styled("Gonder   ", theme::muted()),
        Span::styled("[Tab] ", theme::accent()),
        Span::styled("Model Paleti   ", theme::muted()),
        Span::styled("[/] ", theme::primary()),
        Span::styled("Komutlar   ", theme::muted()),
        Span::styled("[F1] ", theme::accent()),
        Span::styled("Hizli Menu   ", theme::muted()),
        Span::styled("[/test] ", theme::secondary()),
        Span::styled("Test   ", theme::muted()),
        Span::styled("[Esc] ", theme::muted()),
        Span::styled("Cikis", theme::muted()),
    ]);
    frame.render_widget(Paragraph::new(hints_line), chunks[4]);

    // ── 5. Canlı Slash Autocomplete Açılır Kutusu (Prompt Üzerinde Yüzer) ──
    if *active_modal == ActiveModal::None && input.starts_with('/') && !input.contains(' ') {
        let q = input.to_lowercase();
        let matches: Vec<&SlashCommandDef> = SLASH_COMMANDS
            .iter()
            .filter(|c| c.name.starts_with(&q) || c.name.contains(&q))
            .collect();
        if !matches.is_empty() {
            render_floating_slash_menu(frame, chunks[3], &matches, slash_selected_idx);
        }
    }

    // ── 6. Açılır Modal Menüler ──
    match active_modal {
        ActiveModal::UnifiedModelPicker {
            provider_idx,
            model_idx,
            focus_models,
        } => {
            render_unified_model_picker_modal(
                frame,
                selection,
                *provider_idx,
                *model_idx,
                *focus_models,
                config,
            );
        }
        ActiveModal::CommandMenu { selected_idx } => {
            render_command_menu_modal(frame, *selected_idx);
        }
        ActiveModal::None => {}
    }
}

/// Sol dosya ağacı panelini çizer (Elytra IDE).
fn render_file_tree(frame: &mut Frame, area: Rect, tree: &file_tree::FileTree) {
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let kok = tree
        .root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| tree.root.display().to_string());

    let blok = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::Theme::ACCENT))
        .title(format!(" {kok} "));

    // Görünür pencere (kaydırma)
    let ic_yukseklik = area.height.saturating_sub(2) as usize;
    let bas = tree.scroll as usize;
    let son = (bas + ic_yukseklik).min(tree.entries.len());

    let satirlar: Vec<ListItem> = tree.entries[bas..son]
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let gercek = bas + i;
            let secili = gercek == tree.selected;
            let girinti = "  ".repeat(e.depth);
            let ikon = if e.is_dir {
                if e.expanded { "[-]" } else { "[+]" }
            } else {
                " · "
            };
            let metin = format!("{girinti}{ikon} {}", e.name);
            let stil = if secili {
                Style::default()
                    .bg(theme::Theme::ACCENT)
                    .fg(theme::Theme::BG)
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else if e.is_dir {
                Style::default().fg(theme::Theme::ACCENT)
            } else {
                Style::default().fg(theme::Theme::TEXT)
            };
            ListItem::new(metin).style(stil)
        })
        .collect();

    let icerik = if satirlar.is_empty() {
        List::new(vec![ListItem::new("(boş)")])
    } else {
        List::new(satirlar)
    };

    frame.render_widget(icerik.block(blok.clone()), area);

    // Alt bilgi: seçili öğe veya kısayol ipucu
    let ipucu = match tree.secili() {
        Some(e) if e.is_dir => format!("{}  (Enter: aç/kapa)", e.name),
        Some(e) => format!("{}  (Enter: aç)", e.name),
        None => "Ctrl+B: kapat".to_string(),
    };
    if area.height >= 3 {
        let alt = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(format!(" {ipucu}")).style(Style::default().fg(theme::Theme::MUTED)),
            alt,
        );
    }
}

// ─────────────────────── Modal & Popup Diyalogları ───────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Canlı Slash Komut Açılır Menüsü (Prompt Kutusunun Hemen Üstünde Yüzer)
fn render_floating_slash_menu(
    frame: &mut Frame,
    input_chunk: Rect,
    matches: &[&SlashCommandDef],
    selected_idx: usize,
) {
    let count = matches.len().min(8);
    let popup_height = (count + 2) as u16;
    let popup_y = input_chunk.y.saturating_sub(popup_height);
    let popup_width = (input_chunk.width.saturating_sub(4)).clamp(48, 84);

    let popup_rect = Rect {
        x: input_chunk.x + 2,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_rect);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::Theme::ACCENT))
        .title(Span::styled(
            " ◆ KOMUTLAR (Tab: Tamamla | ↑↓: Sec | Enter: Calistir) ◆ ",
            Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup_rect);
    frame.render_widget(block, popup_rect);

    let mut lines = Vec::new();
    for (i, cmd) in matches.iter().take(count).enumerate() {
        let is_cursor = i == selected_idx.min(matches.len() - 1);
        let cursor_mark = if is_cursor { " ► " } else { "   " };

        let name_style = if is_cursor {
            Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)
        } else {
            theme::text()
        };

        lines.push(Line::from(vec![
            Span::styled(cursor_mark, Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<8} ", cmd.name), name_style),
            Span::styled(format!("{:<15} ", cmd.args_hint), theme::secondary()),
            Span::styled(format!("— {}", cmd.desc), theme::muted()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Birleşik Model & Sağlayıcı Paleti (Split-pane Dialog: Sol Sağlayıcılar, Sağ Modeller)
fn render_unified_model_picker_modal(
    frame: &mut Frame,
    selection: &Selection,
    provider_idx: usize,
    model_idx: usize,
    focus_models: bool,
    config: &AppConfig,
) {
    let area = centered_rect(84, 72, frame.area());
    frame.render_widget(Clear, area);

    let modal_block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(if focus_models { theme::Theme::ACCENT } else { theme::Theme::PRIMARY }))
        .title(Span::styled(
            " ◆ MODEL & SAGLAYICI PALETI (Tab/←→: Gecis | ↑↓: Gezin | Enter: Sec | Esc: Kapat) ◆ ",
            theme::title(),
        ));

    let inner = modal_block.inner(area);
    frame.render_widget(modal_block, area);

    // İki dikey sütuna böl: Sol (Sağlayıcılar), Sağ (Modeller)
    let columns = Layout::horizontal([
        Constraint::Percentage(38),
        Constraint::Length(1), // Dikey ayırıcı çizgi
        Constraint::Percentage(62),
    ])
    .split(inner);

    // Dikey ayırıcı çizgi
    let sep_lines: Vec<Line> = (0..columns[1].height)
        .map(|_| Line::from(Span::styled("│", theme::muted())))
        .collect();
    frame.render_widget(Paragraph::new(sep_lines), columns[1]);

    // ── Sol Sütun: Sağlayıcı Listesi ──
    let mut prov_lines = Vec::new();
    let prov_title_style = if !focus_models {
        Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)
    } else {
        theme::muted()
    };
    prov_lines.push(Line::from(Span::styled(
        format!(" ▌ SAGLAYICILAR ({})", PROVIDERS.len()),
        prov_title_style
    )));
    prov_lines.push(Line::from(""));

    for (i, p) in PROVIDERS.iter().enumerate() {
        let is_cursor = i == provider_idx;
        let is_active = i == selection.provider_index;

        let cursor_mark = if is_cursor && !focus_models {
            " ► "
        } else if is_cursor {
            " · "
        } else {
            "   "
        };

        let active_mark = if is_active { "● " } else { "  " };

        let has_key = config.get_key_for(p.id).is_some();
        let badge = if p.is_free_tier {
            Span::styled("[Ucr.]", Style::default().fg(theme::Theme::SUCCESS))
        } else if has_key {
            Span::styled("[✓Key]", Style::default().fg(theme::Theme::PRIMARY))
        } else {
            Span::styled("[Yok ]", theme::muted())
        };

        let name_style = if is_cursor && !focus_models {
            Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default().fg(theme::Theme::TEXT).add_modifier(Modifier::BOLD)
        } else {
            theme::muted()
        };

        prov_lines.push(Line::from(vec![
            Span::styled(cursor_mark, Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(active_mark, Style::default().fg(theme::Theme::SUCCESS)),
            Span::styled(format!("{:<15} ", p.name), name_style),
            badge,
        ]));
    }
    frame.render_widget(Paragraph::new(prov_lines), columns[0]);

    // ── Sağ Sütun: Seçili Sağlayıcının Modelleri ──
    let current_p = &PROVIDERS[provider_idx.min(PROVIDERS.len() - 1)];
    let mut model_lines = Vec::new();
    let model_title_style = if focus_models {
        Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)
    } else {
        theme::muted()
    };

    model_lines.push(Line::from(vec![
        Span::styled(format!(" ▌ {} MODELLERI", current_p.name), model_title_style),
        Span::styled(format!(" — {}", current_p.hint), theme::muted()),
    ]));
    model_lines.push(Line::from(""));

    for (m_i, m) in current_p.models.iter().enumerate() {
        let is_cursor = m_i == model_idx;
        let is_active = provider_idx == selection.provider_index
            && selection.custom_model.is_none()
            && m_i == selection.model_index;

        let cursor_mark = if is_cursor && focus_models {
            " ► "
        } else if is_cursor {
            " · "
        } else {
            "   "
        };

        let active_badge = if is_active {
            Span::styled(" [AKTIF] ", Style::default().fg(theme::Theme::SUCCESS).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("         ", theme::muted())
        };

        let free_badge = if m.free {
            Span::styled("[Ucretsiz] ", Style::default().fg(theme::Theme::SUCCESS))
        } else {
            Span::styled("[API Key]  ", theme::muted())
        };

        let ctx_str = if m.context >= 1_000_000 {
            format!("{:>4}M ctx", m.context / 1_000_000)
        } else {
            format!("{:>4}K ctx", m.context / 1000)
        };

        let model_style = if is_cursor && focus_models {
            Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default().fg(theme::Theme::TEXT).add_modifier(Modifier::BOLD)
        } else {
            theme::text()
        };

        model_lines.push(Line::from(vec![
            Span::styled(cursor_mark, Style::default().fg(theme::Theme::ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<30} ", m.name), model_style),
            Span::styled(format!("{} ", ctx_str), theme::accent()),
            free_badge,
            active_badge,
        ]));

        model_lines.push(Line::from(vec![
            Span::styled("     ", theme::muted()),
            Span::styled(format!("ID: {}", m.id), theme::muted()),
        ]));
    }

    if provider_idx == selection.provider_index
        && let Some(ref custom) = selection.custom_model {
            model_lines.push(Line::from(""));
            model_lines.push(Line::from(vec![
                Span::styled("  * Ozel Model Aktif: ", theme::secondary()),
                Span::styled(custom, Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
            ]));
        }

    frame.render_widget(Paragraph::new(model_lines), columns[2]);
}

/// Komut Menüsü Açılır Modalı (F1 Command Palette)
fn render_command_menu_modal(frame: &mut Frame, current_idx: usize) {
    let area = centered_rect(68, 62, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::Theme::SECONDARY))
        .title(Span::styled(
            " ◆ ELYTRA KOMUT MERKEZI (Yukari/Asagi | Enter: Calistir | Esc: Kapat) ◆ ",
            theme::title(),
        ));

    let commands = [
        ("Model & Saglayici Paleti (Tab)", "15 saglayici ve 60+ model arasinda gezinin", "MODEL & BAGLANTI"),
        ("Secili Saglayiciyi Test Et (/test)", "Gecikme suresini ve yaniti dogrulayin", "MODEL & BAGLANTI"),
        ("Tum Saglayicilari Test Et (/test all)", "15 saglayici baglantisini topluca sinayin", "MODEL & BAGLANTI"),
        ("API Anahtari Ayarla (/key)", "Guvenli API anahtari yapilandirin", "MODEL & BAGLANTI"),

        ("Otonom Kod Ajani (/agent)", "15 dahili aracla kod yazma ve terminal dongusu", "AJAN & OTOMASYON"),
        ("Dahili Kod Araclari (/tools)", "15 yetenekli arac detaylarini goruntuleyin", "AJAN & OTOMASYON"),
        ("Yapilandirma ve Durum (/config)", "Aktif ayarlar, calisma dizini ve anahtar listesi", "AJAN & OTOMASYON"),

        ("Kullanim Kilavuzu (/help)", "Kapsamli kisayol ve slash komut rehberi", "SISTEM"),
        ("Sohbeti Temizle (/clear)", "Ekran gecmisini sifirlayin", "SISTEM"),
        ("Elytra'dan Cikis (/exit)", "Terminal oturumunu sonlandirin", "SISTEM"),
    ];

    let mut lines = Vec::new();
    let mut current_cat = "";

    for (i, (title, desc, cat)) in commands.iter().enumerate() {
        if *cat != current_cat {
            current_cat = cat;
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![
                Span::styled(format!("  ── {} ──", current_cat), theme::secondary()),
            ]));
        }

        let is_cursor = i == current_idx;
        let prefix = if is_cursor { " ► " } else { "   " };

        let style = if is_cursor {
            Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)
        } else {
            theme::text()
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(theme::Theme::PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<38} ", title), style),
            Span::styled(format!("— {}", desc), theme::muted()),
        ]));
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Kod görüntüleyici panelini çizer (Elytra IDE).
fn render_code_view(frame: &mut Frame, area: Rect, cv: &code_view::CodeView) {
    use ratatui::widgets::{Block, Borders, Paragraph};

    let baslik = if cv.kirpildi {
        format!(" {} (kirpildi) ", cv.dosya_adi())
    } else {
        format!(" {} ", cv.dosya_adi())
    };
    let blok = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::Theme::PRIMARY))
        .title(baslik);

    let ic_yukseklik = area.height.saturating_sub(2) as usize;
    let ic_genislik = area.width.saturating_sub(2) as usize;
    let bas = cv.scroll as usize;

    let govde = if let Some(ref h) = cv.hata {
        format!("[!] {h}")
    } else if cv.lines.is_empty() {
        "(dosya ağacından bir dosya seç — Enter)".to_string()
    } else {
        let num_w = cv.numara_genisligi();
        let son = (bas + ic_yukseklik).min(cv.lines.len());
        cv.lines[bas..son]
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let no = bas + i + 1;
                format!("{no:>num_w$} | {l}")
            })
            .collect::<Vec<_>>()
            .join("
")
    };

    frame.render_widget(
        Paragraph::new(govde)
            .block(blok)
            .style(Style::default().fg(theme::Theme::TEXT))
            .scroll((0, 0))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );

    // Alt bilgi: konum + kısayol
    if area.height >= 3 {
        let toplam = cv.lines.len();
        let konum = if toplam == 0 { 0 } else { bas + 1 };
        let alt = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        let ipucu = format!(" {konum}/{toplam}  ·  ↑↓ kaydır  ·  Ctrl+E kapat");
        frame.render_widget(
            Paragraph::new(ipucu).style(Style::default().fg(theme::Theme::MUTED)),
            alt,
        );
    }
    let _ = ic_genislik;
}

/// Diff görünümü panelini çizer (Elytra IDE).
fn render_diff_view(frame: &mut Frame, area: Rect, dv: &diff_view::DiffView) {
    use ratatui::text::{Line, Span, Text};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let (ekle, sil) = dv.ozet();
    let baslik = if dv.kirpildi {
        format!(" diff: {} (+{ekle} -{sil}) [kirpildi] ", dv.baslik)
    } else {
        format!(" diff: {} (+{ekle} -{sil}) ", dv.baslik)
    };
    let blok = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::Theme::SECONDARY))
        .title(baslik);

    let ic_yukseklik = area.height.saturating_sub(2) as usize;
    let bas = dv.scroll as usize;

    let satirlar: Vec<Line> = if let Some(ref h) = dv.hata {
        vec![Line::from(Span::styled(
            format!("[!] {h}"),
            Style::default().fg(theme::Theme::MUTED),
        ))]
    } else {
        let son = (bas + ic_yukseklik).min(dv.satirlar.len());
        dv.satirlar[bas..son]
            .iter()
            .map(|s| {
                let renk = match s.tur {
                    diff_view::DiffTur::Ekle => theme::Theme::SUCCESS,
                    diff_view::DiffTur::Sil => theme::Theme::ERROR,
                    diff_view::DiffTur::Baslik => theme::Theme::ACCENT,
                    diff_view::DiffTur::Dosya => theme::Theme::MUTED,
                    diff_view::DiffTur::Baglam => theme::Theme::TEXT,
                };
                Line::from(Span::styled(s.metin.clone(), Style::default().fg(renk)))
            })
            .collect()
    };

    frame.render_widget(
        Paragraph::new(Text::from(satirlar))
            .block(blok)
            .wrap(Wrap { trim: false }),
        area,
    );

    if area.height >= 3 {
        let toplam = dv.satirlar.len();
        let konum = if toplam == 0 { 0 } else { bas + 1 };
        let alt = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(format!(" {konum}/{toplam}  ·  Ctrl+D yenile/kapat"))
                .style(Style::default().fg(theme::Theme::MUTED)),
            alt,
        );
    }
}
