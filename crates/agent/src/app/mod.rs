use base64::Engine;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use zen_agent::config::AgentConfig;
use zen_agent::monitor::Monitor;
use zen_agent::win::{chat::ChatReader, health, input, WinAccess};
use zen_core::actions::ActionKind;
use zen_agent::{net, GameAccess};
use zen_core::protocol::{AgentMsg, Command, Health};

mod ciclo;
mod recuperacao;
mod spots;
mod comandos;
#[cfg(test)]
mod testes;

use ciclo::*;
use recuperacao::*;
use spots::*;
use comandos::*;


const CHAT_EVERY: Duration = Duration::from_secs(60);

/// Hora local aproximada (fuso fixo do Brasil, UTC-3) para mensagens curtas do painel.
fn chrono_hhmm(ts: u64) -> String {
    let t = ts.saturating_sub(3 * 3600) % 86400;
    format!("{:02}:{:02}", t / 3600, (t % 3600) / 60)
}

fn spot_has_issue(o: &zen_core::recovery::Observation, spot: (i32, i32), c: &zen_core::recovery::RecoveryConfig) -> bool {
    zen_core::recovery::RecoveryMachine::situation(o, Some(spot), c).is_some()
}

fn recovery_can_take_priority(cfg: &AgentConfig, rec: &Recovery, name: &str) -> bool {
    cfg.recovery_enabled && !cfg.no_automation.iter().any(|n| n == name)
        && !rec.hold.contains_key(name) && !rec.pending.contains(name)
        && !rec.machines.get(name).is_some_and(|m| m.gave_up)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn log_dir() -> std::path::PathBuf {
    AgentConfig::path().with_file_name("logs")
}

/// Vigia: sem `--worker`, este processo só sobe o trabalhador e o reinicia se ele cair.
/// Não confiamos no "reiniciar se falhar" do Agendador (ele não dispara em todo tipo de saída).
/// A fase 4 usa a contagem de quedas para o rollback automático (3 quedas em 10 min).
pub fn main() -> anyhow::Result<()> {
    AgentConfig::load_env_file();
    let test_mode = std::env::args().any(|a| a == "--teste");
    if std::env::args().any(|a| a == "--worker") {
        return worker(test_mode);
    }
    // Uma cópia só por sessão. Sem isso, cada duplo-clique deixava mais um agente
    // rodando escondido, cada um com a configuração antiga.
    if already_running() {
        message(
            "O ZenMonitor Agent já está rodando neste PC.\n\n\
             Para parar (por exemplo, depois de editar o agent.toml), rode no PowerShell:\n\
             taskkill /IM zen-agent.exe /F",
        );
        return Ok(());
    }
    // Confere a configuração ANTES de subir, e avisa na tela: este .exe não tem console.
    let mut cfg = AgentConfig::load_or_create()?;
    if test_mode {
        cfg.apply_test_mode();
    }
    if let Err(e) = cfg.validate() {
        message(&format!(
            "Configuração inválida em\n{}\n\n{e}\n\nCorrija o arquivo e abra o zen-agent.exe de novo.",
            AgentConfig::path().display()
        ));
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let mut crashes: Vec<Instant> = Vec::new();
    loop {
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("--worker");
        if test_mode {
            cmd.arg("--teste");
        }
        let status = cmd.status();
        crashes.retain(|t| t.elapsed() < Duration::from_secs(600));
        crashes.push(Instant::now());
        let wait = if crashes.len() >= 3 { 60 } else { 5 };
        let _ = std::fs::write(
            AgentConfig::path().with_file_name("vigia.txt"),
            format!("trabalhador saiu: {status:?}; quedas em 10 min: {}\n", crashes.len()),
        );
        std::thread::sleep(Duration::from_secs(wait));
    }
}

fn already_running() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    // O handle fica aberto até o processo sair: é ele que marca "estou rodando".
    unsafe {
        match CreateMutexW(None, true, w!("Local\\ZenMonitorAgent")) {
            Ok(_handle) => GetLastError() == ERROR_ALREADY_EXISTS,
            Err(_) => false,
        }
    }
}

fn message(text: &str) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    let t: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = MessageBoxW(None, PCWSTR(t.as_ptr()), w!("ZenMonitor Agent"), MB_OK | MB_ICONWARNING);
    }
}
