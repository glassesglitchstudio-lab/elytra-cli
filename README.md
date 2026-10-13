# Elytra — Terminal AI Asistanı

Sıfırdan Rust ile yazılıyor. Hiçbir mevcut CLI fork DEĞİL.

## Görsel Kimlik

**Elytra** = böceklerin iridesan (yağsı renkli) kanat kılıfları.
- Açılışta iki yarı-kanat yana kayar, arayüz ortaya çıkar
- Renk paleti: yeşil-turkuaz-mor geçişli iridesan tonlar
- Standart kutu çerçeveleri yok — gradient çubuk ayırıcılar

## Kurulum

```bash
cargo build --release
./target/release/elytra-cli
```

## Kullanım

```bash
elytra                # İnteraktif TUI
elytra "merhaba"      # Tek seferlik mesaj
elytra --model stealth "selam"
```

## Yol Haritası

- [x] v0.1 — TUI iskeleti (kanat açılışı + ana ekran)
- [ ] v0.2 — Gerçek API bağlantısı (Nous Inference)
- [ ] v0.3 — Sohbet akışı + model yanıtları
- [ ] v0.4 — PlusThinking / çoklu model
- [ ] v1.0 — Tam özellikli asistan

## Geliştirme

```bash
cargo run             # Debug modda çalıştır
cargo build --release # Optimize derleme
cargo fmt             # Kodu düzenle
cargo clippy          # İpuçları
```
