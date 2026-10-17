//! Sekmeler — Elytra IDE'nin beşinci taşı.
//!
//! Aynı anda birden çok dosya açık tutulur; aralarında geçiş yapılır.
//!
//! ## Kısayollar
//! - `Ctrl+Tab` → sonraki sekme
//! - `Ctrl+W` → aktif sekmeyi kapat
//! - `Ctrl+1..9` → numaralı sekmeye geç
//!
//! ## Tasarım notları
//! - Aynı dosya iki kez açılmaz (var olan sekmeye odaklanır).
//! - Sekme sırası açılış sırasıdır; kapatınca komşuya geçilir.

use std::path::PathBuf;

/// En fazla açık sekme.
pub const MAX_SEKME: usize = 12;

/// Açık sekmeler durumu.
pub struct Sekmeler {
    pub dosyalar: Vec<PathBuf>,
    pub aktif: usize,
}

impl Default for Sekmeler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sekmeler {
    pub fn new() -> Self {
        Self { dosyalar: Vec::new(), aktif: 0 }
    }

    /// Dosyayı açar (zaten açıksa ona odaklanır).
    pub fn ac(&mut self, yol: PathBuf) {
        if let Some(i) = self.dosyalar.iter().position(|p| *p == yol) {
            self.aktif = i;
            return;
        }
        if self.dosyalar.len() >= MAX_SEKME {
            // Sınır dolduysa en eskisini düşür
            self.dosyalar.remove(0);
            if self.aktif > 0 {
                self.aktif -= 1;
            }
        }
        self.dosyalar.push(yol);
        self.aktif = self.dosyalar.len() - 1;
    }

    /// Aktif sekmeyi kapatır.
    pub fn kapat_aktif(&mut self) {
        if self.dosyalar.is_empty() {
            return;
        }
        self.dosyalar.remove(self.aktif);
        if self.aktif >= self.dosyalar.len() && !self.dosyalar.is_empty() {
            self.aktif = self.dosyalar.len() - 1;
        }
    }

    /// Belirtilen sekmeyi kapatır.
    pub fn kapat(&mut self, idx: usize) {
        if idx >= self.dosyalar.len() {
            return;
        }
        self.dosyalar.remove(idx);
        if self.dosyalar.is_empty() {
            self.aktif = 0;
        } else if self.aktif >= self.dosyalar.len() {
            self.aktif = self.dosyalar.len() - 1;
        }
    }

    /// Sonraki sekmeye geç (döngüsel).
    pub fn sonraki(&mut self) {
        if !self.dosyalar.is_empty() {
            self.aktif = (self.aktif + 1) % self.dosyalar.len();
        }
    }

    /// Önceki sekmeye geç (döngüsel).
    pub fn onceki(&mut self) {
        if !self.dosyalar.is_empty() {
            self.aktif = if self.aktif == 0 {
                self.dosyalar.len() - 1
            } else {
                self.aktif - 1
            };
        }
    }

    /// Numarayla sekme seç (0 tabanlı).
    pub fn sec(&mut self, idx: usize) {
        if idx < self.dosyalar.len() {
            self.aktif = idx;
        }
    }

    /// Aktif sekmenin yolu.
    pub fn aktif_dosya(&self) -> Option<&PathBuf> {
        self.dosyalar.get(self.aktif)
    }

    /// Aktif sekmenin adı.
    pub fn aktif_ad(&self) -> Option<String> {
        self.aktif_dosya()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
    }

    pub fn bos(&self) -> bool {
        self.dosyalar.is_empty()
    }

    pub fn len(&self) -> usize {
        self.dosyalar.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yol(ad: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/{ad}"))
    }

    #[test]
    fn baslangicta_bos() {
        let s = Sekmeler::new();
        assert!(s.bos());
        assert_eq!(s.aktif, 0);
        assert!(s.aktif_dosya().is_none());
    }

    #[test]
    fn dosya_acilir_ve_aktif_olur() {
        let mut s = Sekmeler::new();
        s.ac(yol("a.rs"));
        assert_eq!(s.len(), 1);
        assert_eq!(s.aktif, 0);
        s.ac(yol("b.rs"));
        assert_eq!(s.len(), 2);
        assert_eq!(s.aktif, 1, "yeni açılan aktif olmalı");
        assert_eq!(s.aktif_ad().unwrap(), "b.rs");
    }

    #[test]
    fn ayni_dosya_iki_kez_acilmaz() {
        let mut s = Sekmeler::new();
        s.ac(yol("a.rs"));
        s.ac(yol("b.rs"));
        s.ac(yol("a.rs"));
        assert_eq!(s.len(), 2, "aynı dosya tekrar eklenmemeli");
        assert_eq!(s.aktif, 0, "var olan sekmeye odaklanmalı");
    }

    #[test]
    fn gecis_dongusel() {
        let mut s = Sekmeler::new();
        s.ac(yol("a"));
        s.ac(yol("b"));
        s.ac(yol("c"));
        s.aktif = 2;
        s.sonraki();
        assert_eq!(s.aktif, 0, "sonuncudan sonra başa dönmeli");
        s.onceki();
        assert_eq!(s.aktif, 2, "ilkten önce sona dönmeli");
    }

    #[test]
    fn kapatma_komsuya_gecer() {
        let mut s = Sekmeler::new();
        s.ac(yol("a"));
        s.ac(yol("b"));
        s.ac(yol("c"));
        s.aktif = 1; // b
        s.kapat_aktif();
        assert_eq!(s.len(), 2);
        assert!(s.aktif < s.len(), "aktif sınır içinde kalmalı");
        // hepsini kapat
        s.kapat_aktif();
        s.kapat_aktif();
        assert!(s.bos());
        assert!(s.aktif_dosya().is_none());
    }

    #[test]
    fn numarayla_secim() {
        let mut s = Sekmeler::new();
        s.ac(yol("a"));
        s.ac(yol("b"));
        s.sec(0);
        assert_eq!(s.aktif_ad().unwrap(), "a");
        s.sec(99); // geçersiz — değişmemeli
        assert_eq!(s.aktif, 0);
    }

    #[test]
    fn sekme_siniri_asilmaz() {
        let mut s = Sekmeler::new();
        for i in 0..(MAX_SEKME + 5) {
            s.ac(yol(&format!("d{i}")));
        }
        assert_eq!(s.len(), MAX_SEKME, "sekme sayısı sınırı aşılmamalı");
        assert!(s.aktif < s.len());
    }
}
