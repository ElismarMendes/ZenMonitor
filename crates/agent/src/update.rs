//! Atualização automática do agente: baixa do central, confere a assinatura, troca o exe e
//! volta atrás sozinho se a versão nova não subir. Regras em zen_core::update; decisão 0015.
//!
//! Como a troca funciona no Windows: um exe em uso não pode ser apagado, mas PODE ser
//! renomeado. O trabalhador renomeia `zen-agent.exe` -> `zen-agent.anterior.exe`, põe o novo no
//! lugar e sai; o vigia (que guardou o caminho `zen-agent.exe` ao subir) relança o trabalhador
//! em 5 s, já com o exe novo. O vigia continua sendo o antigo até o próximo logon (ele é mínimo).

use crate::config::AgentConfig;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;
use zen_core::update::{self as up, LocalState, Pending, StartAction, UpdateMeta};

pub const STATE_FILE: &str = "atualizacao.json";
pub const KEY_FILE: &str = "chave-atualizacao.txt";
const NEW_FILE: &str = "zen-agent.novo.exe";

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn state_path() -> PathBuf {
    AgentConfig::dir().join(STATE_FILE)
}

pub fn load_state() -> LocalState {
    std::fs::read_to_string(state_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_state(st: &LocalState) -> Result<(), String> {
    let p = state_path();
    let tmp = p.with_extension("json.tmp");
    let txt = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, txt).and_then(|_| std::fs::rename(&tmp, &p)).map_err(|e| format!("{}: {e}", p.display()))
}

/// Chave pública (32 bytes) do arquivo ao lado do agente. Sem ela, não atualiza.
pub fn public_key() -> Option<[u8; 32]> {
    let txt = std::fs::read_to_string(AgentConfig::dir().join(KEY_FILE)).ok()?;
    let b = up::from_hex(txt.lines().next()?.trim())?;
    b.try_into().ok()
}

/// Versão que o agente informa ao central: "3.0.0-alpha.1+7" (7 = publicação instalada).
pub fn version_label(st: &LocalState) -> String {
    let seq = st.pending.as_ref().map(|p| p.seq).unwrap_or(st.installed_seq);
    format!("{}+{seq}", env!("CARGO_PKG_VERSION"))
}

/// PRIMEIRA coisa do trabalhador. Conta a tentativa da versão em teste; se passou do limite,
/// desfaz a troca e sai (o vigia relança com o exe anterior).
pub fn startup() -> (LocalState, Option<String>) {
    let mut st = load_state();
    match up::on_start(&mut st, now()) {
        StartAction::Normal => (st, None),
        StartAction::Trial(n) => {
            let _ = save_state(&st);
            let seq = st.pending.as_ref().map(|p| p.seq).unwrap_or(0);
            (st, Some(format!("versão nova (publicação {seq}) em teste, tentativa {n}")))
        }
        StartAction::Rollback => {
            let msg = do_rollback(&mut st);
            // Grava o motivo para o próximo trabalhador (o antigo) avisar no log.
            let _ = std::fs::write(AgentConfig::dir().join("rollback.txt"), &msg);
            std::process::exit(3);
        }
    }
}

/// Volta o exe anterior. Usada no início (quedas demais) e no ciclo (não conectou a tempo).
pub fn do_rollback(st: &mut LocalState) -> String {
    let Some(p) = up::rollback(st) else { return "nada a desfazer".into() };
    let exe = AgentConfig::dir().join("zen-agent.exe");
    let bad = AgentConfig::dir().join(format!("zen-agent.ruim-{}.exe", p.seq));
    let r1 = std::fs::rename(&exe, &bad);
    let r2 = std::fs::rename(&p.backup, &exe);
    let _ = save_state(st);
    format!(
        "ROLLBACK: publicação {} não subiu; voltou para a {} (renomear: {:?} / {:?})",
        p.seq,
        p.from_seq,
        r1.err().map(|e| e.to_string()),
        r2.err().map(|e| e.to_string())
    )
}

/// Conectou ao central: a versão em teste vira a instalada.
pub fn confirm(st: &mut LocalState) -> Option<u64> {
    let seq = up::confirm(st)?;
    let _ = save_state(st);
    Some(seq)
}

/// GET simples em HTTP/1.1 (rede local, sem TLS: o que importa é a assinatura).
fn http_get(host: &str, path: &str, max: u64) -> Result<Vec<u8>, String> {
    let addr = std::net::ToSocketAddrs::to_socket_addrs(host).map_err(|e| e.to_string())?.next().ok_or("endereço inválido")?;
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: zen-agent\r\n\r\n").map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.take(max + 64 * 1024).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("resposta sem cabeçalho")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let body = &raw[split + 4..];
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("HTTP {status}"));
    }
    if head.contains("transfer-encoding: chunked") {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let eol = body[i..].windows(2).position(|w| w == b"\r\n").ok_or("chunk inválido")? + i;
            let n = usize::from_str_radix(String::from_utf8_lossy(&body[i..eol]).split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "tamanho de chunk inválido")?;
            if n == 0 {
                break;
            }
            let start = eol + 2;
            out.extend_from_slice(body.get(start..start + n).ok_or("chunk cortado")?);
            i = start + n + 2;
        }
        Ok(out)
    } else {
        Ok(body.to_vec())
    }
}

/// Atualização baixada e conferida, pronta para trocar.
pub struct Ready {
    pub seq: u64,
    pub path: PathBuf,
}

/// Procura publicação nova (60 s depois de subir e a cada 10 min). Baixa, confere tamanho e
/// assinatura e grava `zen-agent.novo.exe`. Uma vez pronta, para de procurar.
pub fn spawn_checker(central_url: String, key: [u8; 32], tx: std::sync::mpsc::Sender<Ready>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            match check_once(&central_url, &key) {
                Ok(Some(r)) => {
                    tracing::warn!(publicacao = r.seq, "ATUALIZAÇÃO baixada e assinatura conferida; troca quando a automação estiver parada");
                    let _ = tx.send(r);
                    return;
                }
                Ok(None) => {}
                Err(e) => tracing::info!("atualização: {e}"),
            }
            std::thread::sleep(Duration::from_secs(600));
        }
    });
}

fn check_once(central_url: &str, key: &[u8; 32]) -> Result<Option<Ready>, String> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let host = up::http_host(central_url).ok_or("central_url não é ws:// (atualização só na rede local)")?;
    let meta: UpdateMeta = match http_get(&host, "/api/update/agent", 64 * 1024) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("meta inválida: {e}"))?,
        Err(e) if e == "HTTP 404" => return Ok(None), // nada publicado
        Err(e) => return Err(e),
    };
    let st = load_state();
    if !up::wants(&st, &meta) {
        return Ok(None);
    }
    let exe = http_get(&host, "/api/update/agent/bin", up::MAX_SIZE)?;
    if exe.len() as u64 != meta.size {
        return Err(format!("tamanho errado: {} != {}", exe.len(), meta.size));
    }
    let sig: [u8; 64] = up::from_hex(&meta.sig_hex).and_then(|v| v.try_into().ok()).ok_or("assinatura mal formada")?;
    let vk = VerifyingKey::from_bytes(key).map_err(|_| "chave pública inválida")?;
    vk.verify(&up::signed_message(meta.seq, &exe), &Signature::from_bytes(&sig))
        .map_err(|_| format!("ASSINATURA NÃO CONFERE na publicação {}: recusada", meta.seq))?;
    if !exe.starts_with(b"MZ") {
        return Err("não é um exe".into());
    }
    let path = AgentConfig::dir().join(NEW_FILE);
    std::fs::write(&path, &exe).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(Ready { seq: meta.seq, path }))
}

/// Troca o exe e marca "em teste". Depois disso o chamador SAI do processo.
pub fn apply(r: &Ready) -> Result<(), String> {
    let dir = AgentConfig::dir();
    let exe = dir.join("zen-agent.exe");
    let mut backup = dir.join("zen-agent.anterior.exe");
    if backup.exists() && std::fs::remove_file(&backup).is_err() {
        // O vigia pode estar rodando a partir dele: usa outro nome.
        backup = dir.join(format!("zen-agent.anterior-{}.exe", now()));
    }
    std::fs::rename(&exe, &backup).map_err(|e| format!("renomear o exe atual: {e}"))?;
    if let Err(e) = std::fs::rename(&r.path, &exe) {
        let _ = std::fs::rename(&backup, &exe);
        return Err(format!("pôr o exe novo no lugar: {e}"));
    }
    let mut st = load_state();
    st.pending = Some(Pending { seq: r.seq, from_seq: st.installed_seq, starts: 0, since: now(), backup: backup.to_string_lossy().into() });
    if let Err(e) = save_state(&st) {
        // Sem o estado não há rollback: desfaz a troca.
        let _ = std::fs::rename(&exe, &r.path);
        let _ = std::fs::rename(&backup, &exe);
        return Err(format!("gravar o estado: {e}"));
    }
    Ok(())
}
