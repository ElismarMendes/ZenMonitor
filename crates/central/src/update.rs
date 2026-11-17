//! Publicação de atualizações do agente (decisão 0015).
//!
//!   zen-central.exe chave-nova                     cria a chave de assinatura (uma vez)
//!   zen-central.exe publicar-agente <zen-agent.exe> assina e publica para todos os agentes
//!   zen-central.exe publicar-agente <zen-agent.exe> --teste   só para as `test_machines`
//!
//! A chave PRIVADA fica fora da pasta do projeto: %USERPROFILE%\ZenMonitor-chave\assinatura.key
//! (ou ZEN_SIGN_KEY). Quem tem essa chave consegue instalar programa nos agentes: não copie,
//! não mande por chat, não coloque no pacote. A pública vai em chave-atualizacao.txt.

use crate::config::CentralConfig;
use std::path::PathBuf;
use zen_core::update::{self as up, UpdateMeta};

pub fn key_path() -> PathBuf {
    if let Ok(p) = std::env::var("ZEN_SIGN_KEY") {
        if !p.trim().is_empty() {
            return PathBuf::from(p.trim());
        }
    }
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("ZenMonitor-chave").join("assinatura.key")
}

pub fn dir() -> PathBuf {
    CentralConfig::dir().join("atualizacoes")
}

pub fn meta_path() -> PathBuf {
    dir().join("agente.json")
}

pub fn bin_path() -> PathBuf {
    dir().join("agente.exe")
}

pub fn pub_key_path() -> PathBuf {
    CentralConfig::dir().join("chave-atualizacao.txt")
}

pub fn current_meta() -> Option<UpdateMeta> {
    serde_json::from_str(&std::fs::read_to_string(meta_path()).ok()?).ok()
}

/// Canal de teste: mesma estrutura numa subpasta. O número (seq) é contado junto com o
/// estável, então quem muda de canal sempre recebe a publicação mais nova do seu canal.
pub fn test_dir() -> PathBuf {
    dir().join("teste")
}
pub fn test_meta() -> Option<UpdateMeta> {
    serde_json::from_str(&std::fs::read_to_string(test_dir().join("agente.json")).ok()?).ok()
}
pub fn meta_for(test: bool) -> Option<UpdateMeta> {
    if test {
        // Sem nada no canal de teste (ou mais antigo que o estável): segue o estável.
        match (test_meta(), current_meta()) {
            (Some(t), Some(s)) if t.seq > s.seq => Some(t),
            (_, s) => s,
        }
    } else {
        current_meta()
    }
}
pub fn bin_for(test: bool) -> PathBuf {
    match (test, test_meta(), current_meta()) {
        (true, Some(t), Some(s)) if t.seq > s.seq => test_dir().join("agente.exe"),
        (true, Some(_), None) => test_dir().join("agente.exe"),
        _ => bin_path(),
    }
}

fn load_key() -> anyhow::Result<ed25519_dalek::SigningKey> {
    let p = key_path();
    let txt = std::fs::read_to_string(&p)
        .map_err(|e| anyhow::anyhow!("chave não encontrada em {} ({e}). Rode: zen-central.exe chave-nova", p.display()))?;
    let b: [u8; 32] = up::from_hex(txt.trim()).and_then(|v| v.try_into().ok()).ok_or_else(|| anyhow::anyhow!("chave mal formada em {}", p.display()))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&b))
}

fn write_pub(k: &ed25519_dalek::SigningKey) -> anyhow::Result<String> {
    let hex = up::to_hex(k.verifying_key().as_bytes());
    std::fs::write(pub_key_path(), format!("{hex}\n"))?;
    Ok(hex)
}

/// Trata os subcomandos. `Some(código)` = era um subcomando (não sobe o servidor).
pub fn cli(args: &[String]) -> Option<i32> {
    let r = match args.get(1).map(String::as_str) {
        Some("chave-nova") => new_key(args.iter().any(|a| a == "--substituir")),
        Some("publicar-agente") => match args.get(2) {
            Some(p) => publish(p, args.iter().any(|a| a == "--teste")),
            None => Err(anyhow::anyhow!("uso: zen-central.exe publicar-agente <caminho do zen-agent.exe>")),
        },
        _ => return None,
    };
    Some(match r {
        Ok(m) => {
            println!("{m}");
            0
        }
        Err(e) => {
            eprintln!("ERRO: {e:#}");
            1
        }
    })
}

fn new_key(replace: bool) -> anyhow::Result<String> {
    use rand::RngCore;
    let p = key_path();
    if p.exists() && !replace {
        let k = load_key()?;
        let hex = write_pub(&k)?;
        return Ok(format!("a chave já existe em {} (não troquei). Pública: {hex}", p.display()));
    }
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&p, format!("{}\n", up::to_hex(&b)))?;
    let k = ed25519_dalek::SigningKey::from_bytes(&b);
    let hex = write_pub(&k)?;
    Ok(format!(
        "chave criada em {}\npública (vai no pacote dos agentes): {hex}\ngravada em {}\n{}",
        p.display(),
        pub_key_path().display(),
        if replace { "ATENÇÃO: chave trocada. Os agentes já instalados só aceitam a antiga: reinstale com o pacote novo." } else { "" }
    ))
}

fn publish(exe_path: &str, test: bool) -> anyhow::Result<String> {
    use ed25519_dalek::Signer;
    let key = load_key()?;
    let exe = std::fs::read(exe_path)?;
    if !exe.starts_with(b"MZ") || exe.len() as u64 > up::MAX_SIZE {
        anyhow::bail!("{exe_path} não parece o exe do agente");
    }
    let seq = current_meta().map(|m| m.seq).unwrap_or(0).max(test_meta().map(|m| m.seq).unwrap_or(0)) + 1;
    let (meta_p, bin_p) = if test { (test_dir().join("agente.json"), test_dir().join("agente.exe")) } else { (meta_path(), bin_path()) };
    let sig = key.sign(&up::signed_message(seq, &exe));
    let meta = UpdateMeta {
        seq,
        size: exe.len() as u64,
        sig_hex: up::to_hex(&sig.to_bytes()),
        published_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    };
    std::fs::create_dir_all(bin_p.parent().unwrap_or(&dir()))?;
    // exe primeiro, meta por último: quem lê a meta nova já acha o exe novo.
    let tmp = bin_p.with_extension("exe.tmp");
    std::fs::write(&tmp, &exe)?;
    std::fs::rename(&tmp, &bin_p)?;
    let tmp = meta_p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&meta)?)?;
    std::fs::rename(&tmp, &meta_p)?;
    write_pub(&key)?;
    let quem = if test { "só as máquinas de TESTE (test_machines do central.toml)" } else { "todos os agentes do canal estável" };
    Ok(format!("publicada a atualização {seq} ({} bytes) para {quem}. Trocam em até ~10 min, quando a automação estiver parada.", exe.len()))
}
