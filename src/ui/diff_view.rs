//! Diff görünümü paneli — Elytra IDE'nin üçüncü taşı.
//!
//! Dosyalardaki değişiklikleri (git diff) renkli olarak gösterir:
//! eklenen satırlar `+`, silinenler `-`, bağlam satırları nötr.
//!
//! `Ctrl+D` ile açılır/kapanır.
//!
//! ## Tasarım notları
//! - Ayrıştırma **saf fonksiyon**tur ([`diff_ayristir`]) — test edilebilir.
//! - Git yoksa veya depo değilse dürüst hata mesajı gösterilir (sahte veri yok).
//! - Çok uzun diff'ler kırpılır (`MAX_SATIR`).

use std::path::Path;
use std::process::Command;

/// Görüntülenecek en fazla satır.
pub const MAX_SATIR: usize = 4000;

/// Bir diff satırının türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffTur {
    /// `@@ ... @@` başlığı
    Baslik,
    /// Eklenen satır (`+`)
    Ekle,
    /// Silinen satır (`-`)
    Sil,
    /// Değişmeyen bağlam satırı
    Baglam,
    /// Dosya başlığı (`diff --git`, `+++`, `---`)
    Dosya,
}

/// Tek bir diff satırı.
#[derive(Debug, Clone)]
pub struct DiffSatir {
    pub tur: DiffTur,
    pub metin: String,
}

/// Diff görünümü durumu.
pub struct DiffView {
    pub open: bool,
    /// Başlık (dosya adı veya "tüm değişiklikler")
    pub baslik: String,
    pub satirlar: Vec<DiffSatir>,
    pub scroll: u16,
    pub hata: Option<String>,
    pub kirpildi: bool,
}

impl Default for DiffView {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffView {
    pub fn new() -> Self {
        Self {
            open: false,
            baslik: String::new(),
            satirlar: Vec::new(),
            scroll: 0,
            hata: None,
            kirpildi: false,
        }
    }

    /// Git diff çıktısını ayrıştırır (saf fonksiyon — test edilebilir).
    pub fn ayristir(ham: &str) -> (Vec<DiffSatir>, bool) {
        let mut out = Vec::new();
        let mut kirpildi = false;
        for satir in ham.lines() {
            if out.len() >= MAX_SATIR {
                kirpildi = true;
                break;
            }
            let tur = if satir.starts_with("diff --git")
                || satir.starts_with("+++")
                || satir.starts_with("---")
                || satir.starts_with("index ")
                || satir.starts_with("new file")
                || satir.starts_with("deleted file")
            {
                DiffTur::Dosya
            } else if satir.starts_with("@@") {
                DiffTur::Baslik
            } else if satir.starts_with('+') {
                DiffTur::Ekle
            } else if satir.starts_with('-') {
                DiffTur::Sil
            } else {
                DiffTur::Baglam
            };
            out.push(DiffSatir { tur, metin: satir.to_string() });
        }
        (out, kirpildi)
    }

    /// Depodaki değişiklikleri yükler. `dosya` verilirse yalnız o dosya.
    pub fn yukle(&mut self, dosya: Option<&Path>, calisma_dizini: &Path) {
        self.scroll = 0;
        self.hata = None;
        self.kirpildi = false;
        self.baslik = match dosya {
            Some(p) => p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "dosya".into()),
            None => "tüm değişiklikler".into(),
        };

        let mut cmd = Command::new("git");
        cmd.arg("diff").arg("--no-color").current_dir(calisma_dizini);
        if let Some(p) = dosya {
            cmd.arg("--").arg(p);
        }

        match cmd.output() {
            Ok(c) => {
                let stdout = String::from_utf8_lossy(&c.stdout).to_string();
                if stdout.trim().is_empty() {
                    let stderr = String::from_utf8_lossy(&c.stderr);
                    self.hata = Some(if stderr.trim().is_empty() {
                        "Değişiklik yok (çalışma ağacı temiz)".into()
                    } else {
                        format!("git: {}", stderr.trim())
                    });
                    self.satirlar.clear();
                } else {
                    let (s, k) = Self::ayristir(&stdout);
                    self.satirlar = s;
                    self.kirpildi = k;
                }
            }
            Err(e) => {
                self.hata = Some(format!("git çalıştırılamadı: {e}"));
                self.satirlar.clear();
            }
        }
        self.open = true;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn kapat(&mut self) {
        self.open = false;
    }

    pub fn asagi(&mut self) {
        let son = self.satirlar.len().saturating_sub(1) as u16;
        if self.scroll < son {
            self.scroll += 1;
        }
    }

    pub fn yukari(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn sayfa_asagi(&mut self, adim: u16) {
        let son = self.satirlar.len().saturating_sub(1) as u16;
        self.scroll = (self.scroll + adim).min(son);
    }

    pub fn sayfa_yukari(&mut self, adim: u16) {
        self.scroll = self.scroll.saturating_sub(adim);
    }

    /// Özet: (eklenen, silinen) satır sayısı.
    pub fn ozet(&self) -> (usize, usize) {
        let ekle = self.satirlar.iter().filter(|s| s.tur == DiffTur::Ekle).count();
        let sil = self.satirlar.iter().filter(|s| s.tur == DiffTur::Sil).count();
        (ekle, sil)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORNEK: &str = "\
diff --git a/src/main.rs b/src/main.rs
index 1234567..89abcde 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@
 fn main() {
-    println!(\"eski\");
+    println!(\"yeni\");
+    println!(\"eklendi\");
 }
";

    #[test]
    fn diff_satirlari_dogru_siniflanir() {
        let (s, kirpildi) = DiffView::ayristir(ORNEK);
        assert!(!kirpildi);
        assert_eq!(s.iter().filter(|x| x.tur == DiffTur::Dosya).count(), 4); // diff --git, index, ---, +++
        assert_eq!(s.iter().filter(|x| x.tur == DiffTur::Baslik).count(), 1);
        assert_eq!(s.iter().filter(|x| x.tur == DiffTur::Ekle).count(), 2);
        assert_eq!(s.iter().filter(|x| x.tur == DiffTur::Sil).count(), 1);
        assert!(s.iter().any(|x| x.tur == DiffTur::Baglam));
    }

    #[test]
    fn ozet_dogru_sayar() {
        let (s, _) = DiffView::ayristir(ORNEK);
        let mut d = DiffView::new();
        d.satirlar = s;
        assert_eq!(d.ozet(), (2, 1));
    }

    #[test]
    fn bos_diff_bos_liste() {
        let (s, kirpildi) = DiffView::ayristir("");
        assert!(s.is_empty());
        assert!(!kirpildi);
    }

    #[test]
    fn kirpma_calisiyor() {
        let uzun: String = (0..MAX_SATIR + 50).map(|i| format!("+satir {i}\n")).collect();
        let (s, kirpildi) = DiffView::ayristir(&uzun);
        assert!(kirpildi, "kırpılma bayrağı set olmalı");
        assert_eq!(s.len(), MAX_SATIR);
    }

    #[test]
    fn kaydirma_sinirlari() {
        let mut d = DiffView::new();
        d.satirlar = vec![
            DiffSatir { tur: DiffTur::Ekle, metin: "+a".into() },
            DiffSatir { tur: DiffTur::Sil, metin: "-b".into() },
        ];
        d.yukari();
        assert_eq!(d.scroll, 0);
        for _ in 0..50 {
            d.asagi();
        }
        assert_eq!(d.scroll, 1);
    }

    #[test]
    fn git_deposu_olmayan_dizinde_durust_hata() {
        // Sahte veri üretilmemeli: depo değilse hata gösterilir.
        let bos = std::env::temp_dir().join(format!("elytra_dv_bos_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&bos);
        let mut d = DiffView::new();
        d.yukle(None, &bos);
        assert!(d.open);
        assert!(d.hata.is_some(), "depo olmayan dizinde hata beklenir");
        let _ = std::fs::remove_dir_all(&bos);
    }
}
