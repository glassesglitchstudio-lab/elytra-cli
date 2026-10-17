//! Terminal paneli — Elytra IDE'nin dördüncü taşı.
//!
//! Komut çalıştırır ve çıktısını panel içinde gösterir. `Ctrl+T` ile açılır.
//!
//! ## Tasarım notları
//! - Komut **arka planda** (ayrı iş parçacığında) çalışır; arayüz donmaz.
//! - Sonuç `mpsc` kanalıyla ana döngüye döner, her karede yoklanır.
//! - Çıktı satır sayısı ve uzunluk sınırlıdır (bellek koruması).
//! - Platform: Windows'ta `cmd /C`, diğerlerinde `sh -c`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};

/// Gösterilecek en fazla çıktı satırı.
pub const MAX_CIKTI: usize = 2000;
/// Tek satırda en fazla karakter.
pub const MAX_KARAKTER: usize = 500;
/// Saklanacak en fazla komut geçmişi.
pub const MAX_GECMIS: usize = 50;

/// Arka plandan dönen sonuç.
pub struct KomutSonucu {
    pub komut: String,
    pub cikti: Vec<String>,
    pub basarili: bool,
}

/// Terminal paneli durumu.
pub struct TerminalPanel {
    pub open: bool,
    /// Yazılmakta olan komut
    pub input: String,
    /// Komut geçmişi (yukarı ok ile gezinilir)
    pub gecmis: Vec<String>,
    /// Geçmişte kaç adım yukarıdayız (0 = en yeni)
    pub gecmis_imlec: usize,
    /// Panelde görünen çıktı
    pub cikti: Vec<String>,
    pub scroll: u16,
    /// Komut çalışıyor mu?
    pub calisiyor: bool,
    /// Çalışma dizini
    pub cwd: PathBuf,
    tx: Sender<KomutSonucu>,
    rx: Receiver<KomutSonucu>,
}

impl TerminalPanel {
    pub fn new(cwd: PathBuf) -> Self {
        let (tx, rx) = channel();
        Self {
            open: false,
            input: String::new(),
            gecmis: Vec::new(),
            gecmis_imlec: 0,
            cikti: Vec::new(),
            scroll: 0,
            calisiyor: false,
            cwd,
            tx,
            rx,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Komutu arka planda başlatır.
    pub fn calistir(&mut self) {
        let komut = self.input.trim().to_string();
        if komut.is_empty() || self.calisiyor {
            return;
        }
        self.input.clear();
        self.gecmis_imlec = 0;
        if self.gecmis.last().map(|s| s != &komut).unwrap_or(true) {
            self.gecmis.push(komut.clone());
            if self.gecmis.len() > MAX_GECMIS {
                self.gecmis.remove(0);
            }
        }
        self.cikti.push(format!("$ {komut}"));
        self.calisiyor = true;
        self.scroll = 0;

        let tx = self.tx.clone();
        let cwd = self.cwd.clone();
        let k = komut.clone();
        std::thread::spawn(move || {
            let sonuc = komut_calistir(&k, &cwd);
            let _ = tx.send(sonuc);
        });
    }

    /// Arka plandan gelen sonucu yoklar (her karede çağrılır).
    pub fn sonuc_yokla(&mut self) {
        while let Ok(s) = self.rx.try_recv() {
            self.calisiyor = false;
            for l in s.cikti {
                self.cikti.push(l);
            }
            if !s.basarili {
                self.cikti.push("[!] Komut hata koduyla bitti".into());
            }
            if self.cikti.len() > MAX_CIKTI {
                let fazla = self.cikti.len() - MAX_CIKTI;
                self.cikti.drain(0..fazla);
            }
        }
    }

    /// Geçmişte yukarı gez (yukarı ok).
    pub fn gecmis_yukari(&mut self) {
        if self.gecmis.is_empty() {
            return;
        }
        let yeni = (self.gecmis_imlec + 1).min(self.gecmis.len());
        self.gecmis_imlec = yeni;
        let idx = self.gecmis.len() - yeni;
        self.input = self.gecmis[idx].clone();
    }

    /// Geçmişte aşağı gez (aşağı ok).
    pub fn gecmis_asagi(&mut self) {
        if self.gecmis_imlec == 0 {
            return;
        }
        self.gecmis_imlec -= 1;
        if self.gecmis_imlec == 0 {
            self.input.clear();
        } else {
            let idx = self.gecmis.len() - self.gecmis_imlec;
            self.input = self.gecmis[idx].clone();
        }
    }

    pub fn asagi(&mut self) {
        let son = self.cikti.len().saturating_sub(1) as u16;
        if self.scroll < son {
            self.scroll += 1;
        }
    }

    pub fn yukari(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

/// Komutu çalıştırır (platforma göre kabuk seçer).
fn komut_calistir(komut: &str, cwd: &PathBuf) -> KomutSonucu {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(komut);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(komut);
        c
    };

    let out = cmd.current_dir(cwd).output();
    match out {
        Ok(o) => {
            let mut satirlar: Vec<String> = Vec::new();
            let mut ekle = |metin: &str| {
                for l in metin.lines() {
                    if satirlar.len() >= MAX_CIKTI {
                        break;
                    }
                    let t = if l.chars().count() > MAX_KARAKTER {
                        let k: String = l.chars().take(MAX_KARAKTER).collect();
                        format!("{k} ...")
                    } else {
                        l.to_string()
                    };
                    satirlar.push(t);
                }
            };
            ekle(&String::from_utf8_lossy(&o.stdout));
            ekle(&String::from_utf8_lossy(&o.stderr));
            KomutSonucu {
                komut: komut.to_string(),
                cikti: satirlar,
                basarili: o.status.success(),
            }
        }
        Err(e) => KomutSonucu {
            komut: komut.to_string(),
            cikti: vec![format!("[!] Komut başlatılamadı: {e}")],
            basarili: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> TerminalPanel {
        TerminalPanel::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    #[test]
    fn baslangicta_kapali_ve_bos() {
        let t = panel();
        assert!(!t.open);
        assert!(t.input.is_empty());
        assert!(t.cikti.is_empty());
        assert!(!t.calisiyor);
    }

    #[test]
    fn bos_komut_calismaz() {
        let mut t = panel();
        t.input = "   ".into();
        t.calistir();
        assert!(!t.calisiyor, "boş komut çalıştırılmamalı");
        assert!(t.gecmis.is_empty());
    }

    #[test]
    fn komut_gecmise_eklenir_ve_tekrar_edilmez() {
        let mut t = panel();
        t.input = "echo a".into();
        // calistir() arka plan acar; gecmis kaydi senkron
        t.calistir();
        assert_eq!(t.gecmis, vec!["echo a".to_string()]);
        // ayni komut tekrar eklenmez (calisiyor olsa da gecmis kurali test edilir)
        t.calisiyor = false;
        t.input = "echo a".into();
        t.calistir();
        assert_eq!(t.gecmis.len(), 1, "aynı komut iki kez eklenmemeli");
        t.sonuc_yokla();
    }

    #[test]
    fn gecmis_gezinme() {
        let mut t = panel();
        t.gecmis = vec!["ilk".into(), "ikinci".into()];
        t.gecmis_yukari();
        assert_eq!(t.input, "ikinci");
        t.gecmis_yukari();
        assert_eq!(t.input, "ilk");
        t.gecmis_yukari(); // sınırda kalmalı
        assert_eq!(t.input, "ilk");
        t.gecmis_asagi();
        assert_eq!(t.input, "ikinci");
        t.gecmis_asagi();
        assert!(t.input.is_empty(), "en aşağıda girdi temizlenir");
    }

    #[test]
    fn gercek_komut_cikti_uretir() {
        let mut t = panel();
        #[cfg(windows)]
        let k = "echo merhaba";
        #[cfg(not(windows))]
        let k = "echo merhaba";
        t.input = k.into();
        t.calistir();
        // arka plan isini bitirmesi icin kisa bekleme
        for _ in 0..50 {
            t.sonuc_yokla();
            if !t.calisiyor {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!t.calisiyor, "komut bitmeliydi");
        let hepsi = t.cikti.join("\n");
        assert!(hepsi.contains("merhaba"), "çıktıda 'merhaba' olmalı: {hepsi}");
    }
}
