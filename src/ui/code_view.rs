//! Kod görüntüleyici paneli — Elytra IDE'nin ikinci taşı.
//!
//! Dosya ağacından seçilen dosyayı satır numaralı, kaydırmalı biçimde gösterir.
//! `Ctrl+E` ile açılır/kapanır.
//!
//! ## Tasarım notları
//! - Çok büyük dosyalar kırpılır (`MAX_SATIR`) — terminal donmasın.
//! - Sekme karakterleri boşluğa çevrilir (hizalama bozulmasın).
//! - İkili (binary) dosyalar reddedilir.
//! - Glif güvenliği: yalnız ASCII çerçeve karakterleri kullanılır.

use std::path::{Path, PathBuf};

/// Görüntülenecek en fazla satır.
pub const MAX_SATIR: usize = 5000;
/// Tek satırda gösterilecek en fazla karakter.
pub const MAX_KARAKTER: usize = 400;

/// Kod görüntüleyici durumu.
pub struct CodeView {
    /// Panel görünür mü?
    pub open: bool,
    /// Açık dosyanın yolu
    pub path: Option<PathBuf>,
    /// Dosya satırları (kırpılmış)
    pub lines: Vec<String>,
    /// Dikey kaydırma
    pub scroll: u16,
    /// Dosya kırpıldı mı?
    pub kirpildi: bool,
    /// Yüklenirken oluşan hata
    pub hata: Option<String>,
}

impl Default for CodeView {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeView {
    pub fn new() -> Self {
        Self {
            open: false,
            path: None,
            lines: Vec::new(),
            scroll: 0,
            kirpildi: false,
            hata: None,
        }
    }

    /// Dosyayı yükler ve paneli açar.
    pub fn yukle(&mut self, yol: &Path) {
        self.path = Some(yol.to_path_buf());
        self.scroll = 0;
        self.hata = None;
        self.kirpildi = false;

        let ham = match std::fs::read(yol) {
            Ok(b) => b,
            Err(e) => {
                self.hata = Some(format!("Okunamadı: {e}"));
                self.lines.clear();
                self.open = true;
                return;
            }
        };

        // İkili dosya kontrolü (NUL baytı veya geçersiz UTF-8)
        if ham.contains(&0) {
            self.hata = Some("İkili dosya — metin değil".into());
            self.lines.clear();
            self.open = true;
            return;
        }
        let metin = match String::from_utf8(ham) {
            Ok(m) => m,
            Err(_) => {
                self.hata = Some("UTF-8 değil — gösterilemiyor".into());
                self.lines.clear();
                self.open = true;
                return;
            }
        };

        let toplam = metin.lines().count();
        if toplam > MAX_SATIR {
            self.kirpildi = true;
        }
        self.lines = metin
            .lines()
            .take(MAX_SATIR)
            .map(|l| {
                let temiz = l.replace('\t', "    ");
                if temiz.chars().count() > MAX_KARAKTER {
                    let k: String = temiz.chars().take(MAX_KARAKTER).collect();
                    format!("{k} ...")
                } else {
                    temiz
                }
            })
            .collect();
        self.open = true;
    }

    /// Paneli kapatır (dosya bilgisi kalır).
    pub fn kapat(&mut self) {
        self.open = false;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn asagi(&mut self) {
        let son = self.lines.len().saturating_sub(1) as u16;
        if self.scroll < son {
            self.scroll += 1;
        }
    }

    pub fn yukari(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn sayfa_asagi(&mut self, adim: u16) {
        let son = self.lines.len().saturating_sub(1) as u16;
        self.scroll = (self.scroll + adim).min(son);
    }

    pub fn sayfa_yukari(&mut self, adim: u16) {
        self.scroll = self.scroll.saturating_sub(adim);
    }

    /// Başlıkta gösterilecek dosya adı.
    pub fn dosya_adi(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "(dosya yok)".into())
    }

    /// Satır numarası genişliği (hizalama için).
    pub fn numara_genisligi(&self) -> usize {
        let n = self.lines.len().max(1);
        n.to_string().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn gecici_dosya(ad: &str, icerik: &[u8]) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SAYAC: AtomicUsize = AtomicUsize::new(0);
        let n = SAYAC.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("elytra_cv_{}_{n}_{ad}", std::process::id()));
        fs::write(&p, icerik).unwrap();
        p
    }

    #[test]
    fn metin_dosyasi_yuklenir() {
        let p = gecici_dosya("a.rs", b"fn main() {\n    println!(\"hi\");\n}\n");
        let mut cv = CodeView::new();
        cv.yukle(&p);
        assert!(cv.open);
        assert_eq!(cv.lines.len(), 3);
        assert!(cv.hata.is_none());
        assert_eq!(cv.dosya_adi(), p.file_name().unwrap().to_string_lossy());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn ikili_dosya_reddedilir() {
        let p = gecici_dosya("b.bin", &[0u8, 1, 2, 0, 255]);
        let mut cv = CodeView::new();
        cv.yukle(&p);
        assert!(cv.hata.is_some(), "ikili dosya hata vermeli");
        assert!(cv.lines.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn olmayan_dosya_hata_verir() {
        let mut cv = CodeView::new();
        cv.yukle(Path::new("/kesinlikle/yok/bu/dosya.txt"));
        assert!(cv.hata.is_some());
        assert!(cv.open, "hata olsa da panel açılmalı");
    }

    #[test]
    fn kaydirma_sinirlari() {
        let p = gecici_dosya("c.txt", b"a\nb\nc\n");
        let mut cv = CodeView::new();
        cv.yukle(&p);
        cv.yukari();
        assert_eq!(cv.scroll, 0, "yukarıdan öteye gitmemeli");
        for _ in 0..100 {
            cv.asagi();
        }
        assert_eq!(cv.scroll, 2, "son satırda durmalı");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn sekme_bosluga_cevrilir() {
        let p = gecici_dosya("d.txt", b"\tX\n");
        let mut cv = CodeView::new();
        cv.yukle(&p);
        assert_eq!(cv.lines[0], "    X", "sekme 4 boşluk olmalı");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cok_uzun_satir_kirpilir() {
        let uzun = "a".repeat(MAX_KARAKTER + 100);
        let p = gecici_dosya("e.txt", uzun.as_bytes());
        let mut cv = CodeView::new();
        cv.yukle(&p);
        assert!(cv.lines[0].ends_with("..."), "uzun satır kırpılmalı");
        let _ = fs::remove_file(&p);
    }
}
