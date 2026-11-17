//! zen-agent.exe — sobe pelo Agendador de Tarefas no logon, "com privilégios mais altos".
//! Não é serviço do Windows: serviço roda na sessão 0 e não enxerga a janela do jogo.
// Sem janela de console: os logs vão para logs\agent.*.log ao lado do .exe.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("zen-agent só roda no Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    app::main()
}

#[cfg(windows)]
mod app;
