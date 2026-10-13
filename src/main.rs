//! Elytra — terminal AI asistanı
//!
//! Bu proje sıfırdan Rust ile yazılıyor. Hiçbir mevcut CLI'yi fork ETMİYOR.

use clap::Parser;

mod api;
mod ui;

/// Elytra komut satırı arayüzü
#[derive(Parser, Debug)]
#[command(name = "elytra", version, about = "Elytra — terminal AI asistanı")]
struct Cli {
    /// Başlangıç mesajı (opsiyonel): elytra "merhaba"
    #[arg(trailing_var_arg = true)]
    prompt: Vec<String>,

    /// Kullanılacak model (örn: nous/stealth)
    #[arg(short, long)]
    model: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Tek seferlik mesaj modu: elytra "selam"
    if !cli.prompt.is_empty() {
        let prompt = cli.prompt.join(" ");
        println!("{}", api::ask_once(&prompt, cli.model.as_deref()).await?);
        return Ok(());
    }

    // İnteraktif TUI modu
    ui::run(cli.model.as_deref()).await
}
