# Elytra — Ultra-Hizli Terminal AI Asistani & Otonom Ajan

> Sifirdan **Rust** ile gelistirildi. Hicbir mevcut CLI'yi fork etmez.  
> Glitch Code ve OpenCode seviyesinde otonom ajan yeteneklerini, sifir gecikme ve minimum donanim yukuyle sunar.  
> Sifir emoji — saf vektor blok estetik tasarimi ve OpenCode ergonomisi.

---

## Neden Glitch Code'dan Daha Guclu & Hafif?

| Kriter | Glitch Code (TypeScript/Bun) | **Elytra CLI (Rust)** |
|---|---|---|
| **Bellek Tuketimi (RAM)** | ~250 - 600 MB+ | **10 - 15 MB** (20x daha hafif) |
| **Baslangic Suresi** | ~200 - 400 ms | **< 5 ms** (aninda acilir) |
| **Islemci / CPU Yuku** | Agir JS runtime ve daemon'lar | **%0.0 bosta CPU**, PC'yi asla dondurmaz |
| **Ajan Motoru** | Node/Bun Effect-TS ReAct | **Yerel Tokio Async ReAct Dongusu** |
| **Bagimliliklar** | Yuzlerce node_modules paketi | **Tek bagimsiz binary** (Zero external runtime) |
| **Arayuz (TUI)** | Ink / TS TUI | **OpenCode Tarzi Orantili Kart Duzeni & Vektor Kanat Animasyonu** |

---

## Gorsel Kimlik & OpenCode Tarzi Arayuz

**Elytra** = aerodinamik, iridesan (zumrut, turkuaz, mor) vektor kanat kilifi.
- **Birlesik Model & Saglayici Paleti (`Tab`)**: 16 saglayiciyi sol sutunda baglanti/ucretsizlik rozetleriyle, o saglayicinin modellerini ise sag sutunda token baglami ve detaylariyla gosteren bolunmus cift-panelli palet (`Left/Right/Tab` ile gecis, `Up/Down` ile gezinme, `Enter` ile aninda aktivasyon).
- **Canli Slash (/) Komut Menusu**: Giris kutusuna `/` yazildiginda komut kutusunun hemen ustunde acilan, `Tab` ile otomatik tamamlanan akilli floating autocomplete menusu (`/model`, `/agent`, `/tools`, `/test`, `/key`, `/config`, `/help`, `/clear`, `/demo`, `/exit`).
- **OpenCode Tam Ekran Duzen**: Sol seritli dikey kartlar (`▌ You`, `▌ Elytra`, `▌ Tool`, `▌ System`), ferah sohbet akisi.
- **Hizli Komut Menusu (`F1`)**: Model & Baglanti, Ajan & Otomasyon, Sistem olmak uzere 3 temiz kategoriye ayrilmis eylem merkezi.
- **Sifir Emoji**: Terminalde piksel kaymalarini onleyen saf monospaced vektor blok tasarimi (`◆`, `●`, `►`, `✓`, `[OK]`, `[Key]`, `[Ucr.]`).

---

## Temel Yetenekler

### 1. 16 Model Saglayicisi & Dahili Baglanti Testi (`/test`)
- **Ucretsiz Katman Modelleri**:
  - **Elytra Cevrimdisi (`demo`)**: Sifir-key, yerel aninda calisan dahili motor (1ms).
  - **Google Gemini (`google`)**: Gunde 1.500 istek bedava (Gemini 2.0 Flash, Gemini 1.5 Pro).
  - **Groq Cloud (`groq`)**: Gunde 14.400 istek bedava (Llama 3.3 70B, Qwen 2.5 Coder 32B).
  - **Cerebras Cloud (`cerebras`)**: Ultra-hizli ucretsiz GPU cikarimi (2000+ tok/sn).
  - **SambaNova (`sambanova`)**: %100 Ucretsiz Qwen 2.5 Coder 32B ve Llama 70B.
  - **Mistral AI (`mistral`)**: Gelistirici bedava kotasi (Codestral kod uzmani).
  - **OpenRouter (`openrouter`)**: DeepSeek V3, Claude 3.5 Sonnet, GPT-4o Mini ve ucretsiz modeller.
  - **Ollama (`local`)**: Yerel cevrimdisi modeller (`localhost:11434`).
  - **Nous Research (`nous`)**: Stealth ve Space Bunny modelleri.
- **Populer Bulut & Akil Yurutme Modelleri**:
  - **DeepSeek Resmi (`deepseek`)**: DeepSeek-V3 ve DeepSeek-R1 (Thinking).
  - **OpenAI (`openai`)**: GPT-4o, GPT-4o Mini, o1, o3-mini.
  - **Anthropic (`anthropic`)**: Resmi Messages API ile Claude 3.5 Sonnet & Haiku.
  - **Together AI (`together`)**: Hizli Llama 3.3 70B Turbo ve DeepSeek R1.
  - **xAI Grok (`xai`)**: Grok 2 ve Grok Beta.
  - **Perplexity AI (`perplexity`)**: Gercek zamanli internet aramali Sonar Pro.
  - **Cohere (`cohere`)**: Command R+ ve Command R.

### 2. Canli Saglayici Taramasi (`/test` ve CLI `elytra test`) — Devrim
- **Paralel tarama**: 16 saglayici ayni anda taranir (varsayilan 8 es zamanli, tokio JoinSet).
- **Iki kademeli dogrulama**: once ucuz baglanti kontrolu (GET /models), istege bagli tam chat dogrulamasi.
- **Sifir maliyet**: API anahtari tanimli olmayan saglayiciya hic ag istegi atilmaz (aninda `[KEY YOK]`).
- **Akan sonuclar**: her saglayicinin sonucu hazir oldugunda aninda raporlanir (sirali bekleme yok).
- **Akilli siniflandirma**: `[OK]`, `[KEY YOK]`, `[YETKISIZ]` (401/403), `[KOTA]` (429), `[ERISILEMEZ]`, `[HATA]`.
- **Sert zaman asimi**: her saglayici icin ayri timeout (varsayilan 6 saniye).
- **Ozet istatistikler**: calisan/anahtarsiz/kota dagilimi ve en hizli saglayici.
- CLI: `cargo run -- test`, `cargo run -- test all --full`, `cargo run -- test groq`, `cargo run -- test --keys`, `cargo run -- test --timeout 3000 --concurrency 16`, `cargo run -- test --json` (makine okunabilir cikti).

### 3. Otonom Ajan (ReAct Dongusu — Sablon Degil, Gercek Calisan Motor)
Model, verilen yazilim gorevini adim adim cozer. Dosyalari okur, analiz eder, kod bloklarini degistirir, projeyi derler, testleri kosturur, web'de arama yapar ve sonucu bildirir.

**Surdurulebilirlik Guvenceleri:**
- 429/5xx ve ag hatalarinda otomatik yeniden deneme (ussel geri cekilme, varsayilan 3 deneme).
- Context butcesi: araci ciktilari biriktikce eski ciktilar kırpılır (API 400 ve maliyet korumasi).
- Adim siniri: `ELYTRA_MAX_STEPS` env veya `agent_max_steps` config ile ayarlanabilir (varsayilan 12).
- Deneme siniri: `ELYTRA_MAX_RETRIES` env veya `agent_max_retries` config (varsayilan 3).

### 4. 15 Dahili Ajan Araci (Glitch Code & Claude Code Seviyesi)
1. **`read_file`**: Satir numarali, satir araligi sinirli guvenli dosya okuma.
2. **`write_file`**: Otomatik ust dizin olusturan atomik dosya yazma.
3. **`edit_file`**: Dosyada cerrahi hedef blok arama ve degistirme.
4. **`apply_patch`**: Unified diff ve degisiklik yamalarini dogrudan uygulama.
5. **`list_dir`**: `.git`, `node_modules` ve `target` klasorlerini atlayan hizli dizin agaci.
6. **`grep_search`**: Tum calisma alaninda regex ve metin aramasi (glob filtresi, baglam satirlari, binary/buyuk dosya korumali).
7. **`file_search`**: Glob desenli dosya adi aramasi (`*.rs`, `Cargo.*`, `main.rs`).
8. **`git_status`**: Aktif dal, staged/unstaged degisiklikler ve git durumu.
9. **`git_diff`**: Dosya veya calisma alanindaki git farklarini inceleme.
10. **`diagnostics`**: Proje derleme ve linter hatalarini kontrol etme (`cargo check`, `npm run lint`).
11. **`bash_exec`**: 30s zaman asimi korumali guvenli terminal komutu calistirma (PowerShell / sh).
12. **`web_fetch`**: HTTP GET ile URL icerigini ve dokumantasyonu cekme.
13. **`web_search`**: Canli internet aramasi (DuckDuckGo motoru, sifir harici anahtar).
14. **`task_tracker`**: Cok adimli gorev ve checklist yonetimi (`add`, `list`, `complete`, `clear`).
15. **`repo_map`**: Kod tabani mimarisini, proje tipini, temel dosyalari ve dil dagilimini cikarma.

---

## Calistirma

```bash
# OpenCode TUI Arayuzunu Baslat
cargo run

# CLI Otonom Ajan ile Tek Seferlik Soru / Gorev
cargo run -- "Cargo.toml dosyasini oku ve paket adini soyle"
cargo run -- "projenin repo haritasini cikar"
cargo run -- "web'de rust async ara"

# Dahili Araclari Listele
cargo run -- tools

# Saglayicilari Test Et / Tara
cargo run -- test
cargo run -- test all --full
cargo run -- test --keys
cargo run -- test groq
cargo run -- test --timeout 3000 --concurrency 16
```

---

## Kullanim Kilavuzu

### 1. Kisayol Tuslari (OpenCode Stili)
- `Tab`: Model Secici penceresini acar.
- `Shift + Tab`: Saglayici Secici penceresini acar.
- `F1`: Hizli Komut Menusunu acar.
- `Yukari / Asagi / PgUp / PgDn`: Sohbeti kaydirir.
- `Esc`: Pencereleri kapatir / Cikis yapar.

### 2. Komutlar
- `/tools` — 15 dahili kod ve terminal aracini listeler.
- `/test` — Aktif saglayicinin baglantisini ve gecikmesini test eder.
- `/test all` — 16 saglayiciyi birden test eder.
- `/key <saglayici> <anahtar>` — API anahtari kaydeder (`/key groq gsk_...`).
- `/demo` — Cevrimdisi yerel motora gecer.
- `/agent <gorev>` — Otonom ajan ReAct oturumu baslatir.
- `/clear` — Ekrani temizler.
- `/exit` — Elytra'dan ayrilir.
- `/demo` — Elytra Dahili Cevrimdisi moduna gecer (Anahtarsiz).
- `/key <saglayici> <anahtar>` — API anahtarini kaydeder (Orn: `/key groq gsk_...`, `/key openrouter sk-or-...`, `/key google AIzaSy...`).
- `/model <ad>` — Ozel model belirler (Orn: `/model deepseek/deepseek-chat`, `/model gpt-4o`).
- `/agent <gorev>` — ReAct otonom kod ajanini calistirir.
- `/clear` — Sohbeti temizler.
- `/exit` — Cikis yapar.
