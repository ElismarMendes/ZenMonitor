use crate::tokens::{TokenEntry, TokenStore};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub machine: String,
    pub token: String,
    /// Preenchidos pelo pareamento automático. Entrada escrita à mão fica sem eles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_token: Option<String>,
    #[serde(default)]
    pub issued_at: u64,
}

/// `central.toml`, ao lado do .exe. Criado na primeira execução.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CentralConfig {
    /// Só a rede de casa. Para ver de fora: VPN (Tailscale), nunca porta no roteador.
    pub bind: String,
    /// Protege /api/command e /api/auto_pairing até o login com senha da fase 4.
    pub admin_token: String,
    /// AUTOMAÇÃO DE TOKEN. true: agentes sem token se cadastram sozinhos pela rede local e
    /// os tokens são renovados a cada `token_max_age_days`. false: só tokens escritos à mão.
    /// Pode ser trocado no painel (aba MÁQUINAS) ou por ZEN_AUTO_PAIRING=0/1 no .env.
    pub auto_pairing: bool,
    pub token_max_age_days: u64,
    /// Máquinas que nunca podem se conectar nem se cadastrar.
    pub blocked_machines: Vec<String>,
    /// Canal de TESTE de atualização (v4, 10/11/2026): essas máquinas recebem o agente
    /// publicado com `publicar-agente <exe> --teste`; as outras ficam no canal estável.
    pub test_machines: Vec<String>,
    /// v4 (mapas): pasta do jogo NESTE PC, de onde o painel lê os terrenos (Data\\WorldN).
    pub game_dir: String,
    pub stopped_after_s: u64,
    pub db_path: String,
    /// Ex.: "https://ntfy.sh/zen-<algo-secreto>". Vazio = desligado.
    pub ntfy_url: String,
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    /// Dead man's switch (ex.: healthchecks.io). Recebe um GET por minuto.
    pub healthcheck_url: String,
    /// Não repetir o mesmo alerta no celular antes disso.
    pub notify_cooldown_s: u64,
    pub agents: Vec<AgentEntry>,
}

pub fn random_token(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

impl Default for CentralConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8787".into(),
            admin_token: random_token(32),
            auto_pairing: true,
            token_max_age_days: 30,
            blocked_machines: vec![],
            test_machines: vec![],
            game_dir: r"C:\The Classic Games\The Classic MU".into(),
            stopped_after_s: 180,
            db_path: "central.db".into(),
            ntfy_url: String::new(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            healthcheck_url: String::new(),
            notify_cooldown_s: 600,
            agents: vec![],
        }
    }
}

impl CentralConfig {
    /// Pasta do central: ZEN_CENTRAL_DIR, ou a pasta do .exe.
    pub fn dir() -> PathBuf {
        if let Ok(d) = std::env::var("ZEN_CENTRAL_DIR") {
            if !d.trim().is_empty() {
                return PathBuf::from(d);
            }
        }
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Lê o `.env` da pasta (sem sobrescrever o que já está no ambiente).
    pub fn load_env_file() {
        if let Ok(txt) = std::fs::read_to_string(Self::dir().join(".env")) {
            for (k, v) in zen_core::envfile::parse(&txt) {
                if std::env::var_os(&k).is_none() {
                    std::env::set_var(k, v);
                }
            }
        }
    }

    pub fn load_or_create() -> anyhow::Result<Self> {
        let p = Self::dir().join("central.toml");
        let mut c: Self = if p.exists() {
            toml::from_str(&std::fs::read_to_string(&p)?)
                .map_err(|e| anyhow::anyhow!("erro de sintaxe no central.toml: {e}"))?
        } else {
            let c = Self::default();
            c.save()?;
            println!("central.toml criado em {}.", p.display());
            c
        };
        // Variáveis de ambiente (ou .env) mandam sobre o arquivo.
        if let Some(v) = std::env::var("ZEN_AUTO_PAIRING")
            .ok()
            .and_then(|v| zen_core::envfile::parse_bool(&v))
        {
            c.auto_pairing = v;
        }
        if let Ok(v) = std::env::var("ZEN_NTFY_URL") {
            if !v.trim().is_empty() {
                c.ntfy_url = v.trim().to_string();
            }
        }
        if let Ok(v) = std::env::var("ZEN_BIND") {
            if !v.trim().is_empty() {
                c.bind = v;
            }
        }
        Ok(c)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let p = Self::dir().join("central.toml");
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn token_store(&self) -> TokenStore {
        TokenStore {
            entries: self
                .agents
                .iter()
                .map(|a| TokenEntry {
                    machine: a.machine.clone(),
                    token: a.token.clone(),
                    prev_token: a.prev_token.clone(),
                    install_id: a.install_id.clone(),
                    issued_at: a.issued_at,
                })
                .collect(),
            blocked: self.blocked_machines.clone(),
        }
    }

    pub fn set_from_store(&mut self, s: &TokenStore) {
        self.blocked_machines = s.blocked.clone();
        self.agents = s
            .entries
            .iter()
            .map(|e| AgentEntry {
                machine: e.machine.clone(),
                token: e.token.clone(),
                install_id: e.install_id.clone(),
                prev_token: e.prev_token.clone(),
                issued_at: e.issued_at,
            })
            .collect();
    }
}

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
