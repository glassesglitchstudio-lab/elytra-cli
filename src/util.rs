//! Elystra Ortak Yardımcıları — UTF-8 güvenli metin işleme
//!
//! Türkçe karakter (çok baytlı UTF-8) içeren çıktılarda byte-slicing
//! (`&text[..150]`) panic'e yol açar. Tüm kırpma işlemleri buradaki
//! karakter-bazlı güvenli fonksiyonlar üzerinden yapılır.

/// Metni en fazla `max_chars` karaktere kırpar, gerekirse sonuna "..." ekler.
/// Çok baytlı karakterlerin ortasında asla kesmez; her zaman geçerli UTF-8 döner.
pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

/// Metni en fazla `max_chars` karaktere kırpar, kırpma olduğunda not ekler.
pub fn truncate_chars_with_note(text: &str, max_chars: usize, note: &str) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str(note);
    out
}

/// Tek satırlık önizleme üret (satır sonlarını boşluğa çevirir ve kırpar).
pub fn one_line_preview(text: &str, max_chars: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
        .collect();
    truncate_chars(&flat, max_chars)
}

/// Bir dosyanın muhtemelen ikili (binary) olup olmadığını tespit eder.
/// İlk 8 KB'nin içinde NUL baytı varsa kabul edilir.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Dosyayı güvenle metin olarak okur; ikili veya çok büyük dosyaları reddeder.
/// `max_bytes` sınırını aşan dosyalar için hata döner (ajanı korur).
pub fn read_text_file_safe(path: &std::path::Path, max_bytes: u64) -> anyhow::Result<String> {
    use std::io::Read;
    let meta = std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("Dosya bilgisi okunamadi ({}): {e}", path.display()))?;
    if !meta.is_file() {
        anyhow::bail!("Dizin veya ozel dosya: {}", path.display());
    }
    if meta.len() > max_bytes {
        anyhow::bail!(
            "Dosya cok buyuk ({} bayt > {} bayt siniri): {}",
            meta.len(),
            max_bytes,
            path.display()
        );
    }
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("Dosya acilamadi ({}): {e}", path.display()))?;
    let mut buf = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut buf)
        .map_err(|e| anyhow::anyhow!("Dosya okunamadi ({}): {e}", path.display()))?;
    if looks_binary(&buf) {
        anyhow::bail!("Ikili (binary) dosya atlandi: {}", path.display());
    }
    String::from_utf8(buf)
        .map_err(|_| anyhow::anyhow!("Dosya gecerli UTF-8 degil: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn troncates_multibyte_without_panic() {
        // 150. bayt "ı" harfinin ortasina denk gelsin: 149 x 'a' + 'ı' + masa
        let text = format!("{}{}", "a".repeat(149), "ıüğüüüüüüü");
        let out = truncate_chars(&text, 150);
        assert!(out.chars().count() <= 153);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn short_text_untouched() {
        assert_eq!(truncate_chars("merhaba", 50), "merhaba");
        assert_eq!(truncate_chars("merhaba", 7), "merhaba");
    }

    #[test]
    fn detects_binary_and_size_limit() {
        let dir = std::env::temp_dir().join("elytra_util_test");
        let _ = std::fs::create_dir_all(&dir);
        let bin = dir.join("bin.dat");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        let txt = dir.join("t.txt");
        std::fs::write(&txt, "selam").unwrap();
        let big = dir.join("big.txt");
        std::fs::write(&big, vec![b'x'; 4096]).unwrap();

        assert!(read_text_file_safe(&bin, 1_000_000).is_err(), "binary reddedilmeli");
        assert_eq!(read_text_file_safe(&txt, 1_000_000).unwrap(), "selam");
        assert!(read_text_file_safe(&big, 1024).is_err(), "buyuk dosya reddedilmeli");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_line_preview_flattens() {
        let p = one_line_preview("satir1\nsatir2\tsekme", 100);
        assert!(!p.contains('\n') && !p.contains('\t'));
    }
}
