//! Dosya ağacı paneli — Elytra IDE'nin ilk taşı.
//!
//! Sol tarafta proje dosyalarını gösterir, klavyeyle gezinilir.
//! `Ctrl+B` ile açılıp kapanır (VS Code alışkanlığı).
//!
//! ## Tasarım notları
//! - Ağır klasörler atlanır (`target`, `node_modules`, `.git`, `__pycache__`).
//! - Görünen liste **düz** tutulur (`entries`), girinti `depth` ile verilir —
//!   çizim ve gezinme basitleşir.
//! - Glif güvenliği: Windows ConHost'ta bozulan simgeler yerine ASCII
//!   kullanılır (`[+]` açık, `[-]` kapalı, `·` dosya).

use std::fs;
use std::path::{Path, PathBuf};

/// Atlanacak klasör adları (performans + gürültü azaltma).
const ATLANAN: &[&str] = &[
    "target", "node_modules", ".git", "__pycache__", ".venv", "venv",
    "dist", "build", ".next", ".cache", "vendor",
];

/// Ağaçta tek bir satır.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    /// Klasör açık mı (dosyalar için anlamsız)
    pub expanded: bool,
}

/// Dosya ağacı durumu.
pub struct FileTree {
    /// Panel görünür mü?
    pub open: bool,
    /// Kök dizin
    pub root: PathBuf,
    /// Görünen (düzleştirilmiş) satırlar
    pub entries: Vec<TreeEntry>,
    /// Seçili satır indeksi
    pub selected: usize,
    /// Kaydırma
    pub scroll: u16,
    /// Açık klasörlerin yolları (kapansa da hatırlanır)
    acik: Vec<PathBuf>,
}

impl FileTree {
    /// Yeni ağaç — kök dizini tarar.
    pub fn new(root: PathBuf) -> Self {
        let mut t = Self {
            open: false,
            root,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            acik: Vec::new(),
        };
        t.yenile();
        t
    }

    /// Geçerli dizinden ağaç kurar.
    pub fn from_cwd() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(root)
    }

    /// Panel aç/kapa.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open && self.entries.is_empty() {
            self.yenile();
        }
    }

    /// Görünen listeyi yeniden kurar.
    pub fn yenile(&mut self) {
        let mut out = Vec::new();
        self.dizin_ekle(&self.root.clone(), 0, &mut out, 0);
        self.entries = out;
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    /// Bir dizini (açıksa içeriğiyle) listeye ekler.
    fn dizin_ekle(&self, dizin: &Path, depth: usize, out: &mut Vec<TreeEntry>, seviye: usize) {
        // Aşırı derinliğe ve aşırı satır sayısına karşı koruma
        if seviye > 6 || out.len() > 2000 {
            return;
        }
        let mut girdiler: Vec<(PathBuf, String, bool)> = Vec::new();
        let oku = match fs::read_dir(dizin) {
            Ok(r) => r,
            Err(_) => return,
        };
        for g in oku.flatten() {
            let p = g.path();
            let ad = g.file_name().to_string_lossy().to_string();
            if ad.starts_with('.') && ad != ".env.example" {
                continue;
            }
            let d = p.is_dir();
            if d && ATLANAN.contains(&ad.as_str()) {
                continue;
            }
            girdiler.push((p, ad, d));
        }
        // Klasörler önce, sonra alfabetik
        girdiler.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));

        for (p, ad, d) in girdiler {
            let acik_mi = d && self.acik.contains(&p);
            out.push(TreeEntry {
                path: p.clone(),
                name: ad,
                depth,
                is_dir: d,
                expanded: acik_mi,
            });
            if acik_mi {
                self.dizin_ekle(&p, depth + 1, out, seviye + 1);
            }
        }
    }

    /// Seçili satırı döner.
    pub fn secili(&self) -> Option<&TreeEntry> {
        self.entries.get(self.selected)
    }

    /// Seçili dosyanın yolu (klasörse `None`).
    pub fn secili_dosya(&self) -> Option<PathBuf> {
        self.secili().filter(|e| !e.is_dir).map(|e| e.path.clone())
    }

    pub fn yukari(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn asagi(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Seçili klasörü aç/kapa (Enter).
    pub fn ac_kapa(&mut self) {
        let Some(e) = self.secili().cloned() else { return };
        if !e.is_dir {
            return;
        }
        if let Some(i) = self.acik.iter().position(|p| *p == e.path) {
            self.acik.remove(i);
        } else {
            self.acik.push(e.path);
        }
        self.yenile();
    }

    /// Kök dizini değiştirir.
    pub fn kok_degistir(&mut self, yeni: PathBuf) {
        self.root = yeni;
        self.acik.clear();
        self.selected = 0;
        self.scroll = 0;
        self.yenile();
    }

    /// Üst dizine çık.
    pub fn ust_dizine_cik(&mut self) {
        if let Some(ust) = self.root.parent() {
            let u = ust.to_path_buf();
            self.kok_degistir(u);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gecici_agac() -> PathBuf {
        // Her test KENDİ klasörünü almalı: aynı process id ile 4 test paralel
        // koşunca birbirlerinin klasörünü siliyordu (yarış durumu).
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SAYAC: AtomicUsize = AtomicUsize::new(0);
        let n = SAYAC.fetch_add(1, Ordering::SeqCst);
        let kok = std::env::temp_dir().join(format!("elytra_ft_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&kok);
        fs::create_dir_all(kok.join("src")).unwrap();
        fs::create_dir_all(kok.join("target")).unwrap();
        fs::write(kok.join("README.md"), "selam").unwrap();
        fs::write(kok.join("src").join("main.rs"), "fn main() {}").unwrap();
        kok
    }

    #[test]
    fn agac_kurulur_ve_agir_klasorler_atlanir() {
        let kok = gecici_agac();
        let t = FileTree::new(kok.clone());
        let adlar: Vec<&str> = t.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(adlar.contains(&"src"), "src bulunamadı: {adlar:?}");
        assert!(adlar.contains(&"README.md"), "README bulunamadı");
        assert!(!adlar.contains(&"target"), "target atlanmalıydı");
        let _ = fs::remove_dir_all(&kok);
    }

    #[test]
    fn klasor_acilinca_icerik_gorunur() {
        let kok = gecici_agac();
        let mut t = FileTree::new(kok.clone());
        // src klasörünü bul ve aç
        let i = t.entries.iter().position(|e| e.name == "src").unwrap();
        t.selected = i;
        t.ac_kapa();
        let adlar: Vec<&str> = t.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(adlar.contains(&"main.rs"), "açılınca main.rs görünmeli: {adlar:?}");
        // tekrar kapat
        let i2 = t.entries.iter().position(|e| e.name == "src").unwrap();
        t.selected = i2;
        t.ac_kapa();
        let adlar2: Vec<&str> = t.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!adlar2.contains(&"main.rs"), "kapanınca gizlenmeli");
        let _ = fs::remove_dir_all(&kok);
    }

    #[test]
    fn gezinme_sinirlari() {
        let kok = gecici_agac();
        let mut t = FileTree::new(kok.clone());
        t.yukari();
        assert_eq!(t.selected, 0, "ilk satırdan yukarı çıkmamalı");
        for _ in 0..1000 {
            t.asagi();
        }
        assert!(t.selected < t.entries.len(), "seçim listenin dışına çıkmamalı");
        let _ = fs::remove_dir_all(&kok);
    }

    #[test]
    fn secili_dosya_klasorde_none() {
        let kok = gecici_agac();
        let mut t = FileTree::new(kok.clone());
        let i = t.entries.iter().position(|e| e.name == "src").unwrap();
        t.selected = i;
        assert!(t.secili_dosya().is_none(), "klasör seçilince dosya dönmemeli");
        let j = t.entries.iter().position(|e| e.name == "README.md").unwrap();
        t.selected = j;
        assert!(t.secili_dosya().is_some(), "dosya seçilince yol dönmeli");
        let _ = fs::remove_dir_all(&kok);
    }
}
