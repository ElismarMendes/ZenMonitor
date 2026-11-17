//! Conexão de SAÍDA do agente para o central (nenhuma porta aberta no PC).
//! Reconecta sozinho; enquanto o central estiver fora, guarda relatórios num buffer.
//!
//! Token automático (`auto_token = true`):
//! - sem token -> pareia em /ws/pair e grava o token no agent.toml;
//! - central recusou o token (401) -> apaga e pareia de novo (mesmo install_id = mesmo PC);
//! - central mandou `NewToken` (renovação) -> grava e usa na próxima conexão.
//!
//! Token manual (`auto_token = false`): usa o do arquivo; se recusado, só registra o erro.

use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use zen_core::protocol::{pair_url, AgentMsg, CentralMsg, Command, PROTOCOL_VERSION};

pub struct NetConfig {
    pub url: String,
    pub token: String,
    pub machine: String,
    pub channel: String,
    pub buffer: usize,
    pub auto_token: bool,
    pub install_id: String,
    /// Versão informada ao central ("3.0.0-alpha.1+7": 7 = atualização instalada).
    pub version: String,
}

enum Outcome {
    /// Conexão terminou normalmente.
    Closed,
    /// O central recusou o token.
    Unauthorized,
}

/// Já conectou ao central pelo menos uma vez (confirma uma atualização em teste).
pub static CONNECTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Estado atual, separado da confirmação histórica usada pelo atualizador legado.
pub static ONLINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Roda para sempre. `outgoing`: mensagens do agente. `commands`: comandos recebidos.
pub async fn run(mut cfg: NetConfig, mut outgoing: mpsc::Receiver<AgentMsg>, commands: mpsc::Sender<(u64, Command)>) {
    let mut pending: VecDeque<AgentMsg> = VecDeque::new();
    let mut backoff = 1u64;
    loop {
        if cfg.token.is_empty() {
            if cfg.auto_token {
                match pair(&cfg).await {
                    Ok(token) => {
                        tracing::info!(maquina = %cfg.machine, "token obtido automaticamente do central");
                        store_token(&mut cfg, token);
                        backoff = 1;
                        continue;
                    }
                    Err(e) => tracing::warn!("pareamento automático falhou: {e:#}; tentando de novo em {backoff}s"),
                }
            } else {
                tracing::error!("sem token e com auto_token = false: escreva o token no agent.toml");
            }
        } else {
            match connect_and_serve(&mut cfg, &mut outgoing, &commands, &mut pending).await {
                Ok(Outcome::Closed) => backoff = 1,
                Ok(Outcome::Unauthorized) if cfg.auto_token => {
                    tracing::warn!("central recusou o token; pedindo um novo (auto_token ligado)");
                    store_token(&mut cfg, String::new());
                    continue;
                }
                Ok(Outcome::Unauthorized) => tracing::error!(
                    "central RECUSOU o token deste PC (401) e auto_token está desligado. Confira o token no agent.toml"
                ),
                Err(e) => tracing::warn!("central indisponível: {e:#}; tentando de novo em {backoff}s"),
            }
        }
        // Enquanto espera, continua aceitando relatórios para o buffer.
        let deadline = tokio::time::sleep(Duration::from_secs(backoff));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => break,
                Some(m) = outgoing.recv() => push_bounded(&mut pending, m, cfg.buffer),
            }
        }
        backoff = (backoff * 2).min(30);
    }
}

fn store_token(cfg: &mut NetConfig, token: String) {
    if let Err(e) = crate::config::AgentConfig::save_token(&token) {
        tracing::error!("não consegui gravar o token no agent.toml: {e:#} (vale só até reiniciar)");
    }
    cfg.token = token;
}

/// Pede um token em /ws/pair. O central só aceita da rede local e com auto_pairing ligado.
async fn pair(cfg: &NetConfig) -> anyhow::Result<String> {
    let url = pair_url(&cfg.url).ok_or_else(|| anyhow::anyhow!("central_url deve terminar em /ws/agent"))?;
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(url)).await??;
    let msg = AgentMsg::Pair {
        protocol: PROTOCOL_VERSION,
        machine: cfg.machine.clone(),
        install_id: cfg.install_id.clone(),
    };
    ws.send(Message::Text(serde_json::to_string(&msg)?)).await?;
    let reply = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(m) = ws.next().await {
            if let Message::Text(t) = m? {
                return Ok::<_, anyhow::Error>(serde_json::from_str::<CentralMsg>(&t)?);
            }
        }
        anyhow::bail!("central fechou sem responder")
    })
    .await??;
    match reply {
        CentralMsg::Paired { token, .. } if token.chars().all(|c| c.is_ascii_alphanumeric()) && token.len() >= 16 => {
            Ok(token)
        }
        CentralMsg::Reject { reason } => anyhow::bail!("central recusou: {reason}"),
        _ => anyhow::bail!("resposta inesperada do central"),
    }
}

fn push_bounded(q: &mut VecDeque<AgentMsg>, m: AgentMsg, cap: usize) {
    // Só relatórios e mortes vão para o buffer; respostas de comando antigas não têm valor.
    if matches!(m, AgentMsg::Report { .. } | AgentMsg::Death { .. }) {
        if q.len() >= cap {
            q.pop_front();
        }
        q.push_back(m);
    }
}

async fn connect_and_serve(
    cfg: &mut NetConfig,
    outgoing: &mut mpsc::Receiver<AgentMsg>,
    commands: &mpsc::Sender<(u64, Command)>,
    pending: &mut VecDeque<AgentMsg>,
) -> anyhow::Result<Outcome> {
    let mut req = cfg.url.as_str().into_client_request()?;
    req.headers_mut()
        .insert("authorization", format!("Bearer {}", cfg.token).parse()?);
    let connected = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(req)).await?;
    let (ws, _) = match connected {
        Ok(ok) => ok,
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) if resp.status().as_u16() == 401 => {
            return Ok(Outcome::Unauthorized);
        }
        Err(e) => return Err(e.into()),
    };
    let (mut tx, mut rx) = ws.split();

    let hello = AgentMsg::Hello {
        protocol: PROTOCOL_VERSION,
        agent_version: cfg.version.clone(),
        machine: cfg.machine.clone(),
        channel: cfg.channel.clone(),
    };
    tx.send(Message::Text(serde_json::to_string(&hello)?)).await?;
    CONNECTED.store(true, std::sync::atomic::Ordering::SeqCst);
    ONLINE.store(true,std::sync::atomic::Ordering::SeqCst);
    struct Online;impl Drop for Online{fn drop(&mut self){ONLINE.store(false,std::sync::atomic::Ordering::SeqCst);}}
    let _online=Online;
    tracing::info!(url = %cfg.url, "conectado ao central; reenviando {} relatórios guardados", pending.len());
    while let Some(m) = pending.pop_front() {
        if let Err(e) = tx.send(Message::Text(serde_json::to_string(&m)?)).await {
            pending.push_front(m);
            return Err(e.into());
        }
    }

    let mut ping = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            m = outgoing.recv() => {
                let Some(m) = m else { return Ok(Outcome::Closed) };
                let txt = serde_json::to_string(&m)?;
                if let Err(e) = tx.send(Message::Text(txt)).await {
                    push_bounded(pending, m, cfg.buffer);
                    return Err(e.into());
                }
            }
            incoming = rx.next() => {
                let Some(incoming) = incoming else { anyhow::bail!("conexão fechada pelo central") };
                match incoming? {
                    Message::Text(t) => match serde_json::from_str::<CentralMsg>(&t) {
                        Ok(CentralMsg::SpotDirectory { directory }) => crate::spot_service::receive(directory),
                        Ok(CentralMsg::SpotLeaseResult { request_id, accepted, reason }) => crate::spot_service::acknowledge(request_id,accepted,reason),
                        Ok(CentralMsg::Command { id, command }) => { let _ = commands.send((id, command)).await; }
                        Ok(CentralMsg::NewToken { token }) if token.chars().all(|c| c.is_ascii_alphanumeric()) && token.len() >= 16 => {
                            tracing::info!("token renovado pelo central; gravado no agent.toml");
                            store_token(cfg, token);
                        }
                        Ok(CentralMsg::Reject { reason }) => anyhow::bail!("central recusou: {reason}"),
                        Ok(_) => {}
                        // Mensagem desconhecida é ignorada, nunca executada.
                        Err(e) => tracing::warn!("mensagem do central ignorada: {e}"),
                    },
                    Message::Close(_) => anyhow::bail!("central fechou a conexão"),
                    _ => {}
                }
            }
            _ = ping.tick() => { tx.send(Message::Ping(Vec::new())).await?; }
        }
    }
}
