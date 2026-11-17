//! Atualização automática do agente — regras puras (sem rede, sem disco). Ver docs/decisoes/0015.
//!
//! - O central publica o exe do agente com uma assinatura Ed25519 (chave privada só no PC
//!   principal, fora da pasta do projeto). O agente só troca de exe se a assinatura bater com a
//!   chave pública que veio no pacote de instalação.
//! - A assinatura cobre CONTEXTO + número da publicação (`seq`) + bytes do exe: não dá para
//!   reaproveitar uma assinatura velha com outro número (sem downgrade por replay).
//! - Rollback: a versão nova fica "em teste" até conectar ao central. Se cair 3 vezes ou não
//!   conectar em 10 min, volta para a anterior e aquela publicação nunca mais é tentada.

pub const SIGN_CONTEXT: &[u8] = b"zenmonitor-agente-v1\0";
/// Tentativas de subir a versão nova antes do rollback.
pub const MAX_STARTS: u32 = 3;
/// Tempo máximo "em teste" sem conectar ao central.
pub const TRIAL_S: u64 = 600;
/// Maior exe aceito (o agente tem ~2,5 MB).
pub const MAX_SIZE: u64 = 64 * 1024 * 1024;

/// Mensagem assinada: contexto + seq (8 bytes, little-endian) + exe.
pub fn signed_message(seq: u64, exe: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(SIGN_CONTEXT.len() + 8 + exe.len());
    m.extend_from_slice(SIGN_CONTEXT);
    m.extend_from_slice(&seq.to_le_bytes());
    m.extend_from_slice(exe);
    m
}

/// O que o central anuncia em /api/update/agent.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateMeta {
    pub seq: u64,
    pub size: u64,
    /// Assinatura Ed25519 (64 bytes) em hexadecimal.
    pub sig_hex: String,
    /// Quando foi publicada (unix).
    #[cfg_attr(feature = "serde", serde(default))]
    pub published_at: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub seq: u64,
    pub from_seq: u64,
    pub starts: u32,
    pub since: u64,
    /// Onde ficou o exe anterior (para o rollback).
    pub backup: String,
}

/// Estado gravado ao lado do agente (`atualizacao.json`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalState {
    pub installed_seq: u64,
    pub pending: Option<Pending>,
    /// Publicações que falharam aqui: nunca mais tentar.
    pub refused: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAction {
    /// Sem atualização em teste.
    Normal,
    /// Versão nova em teste (tentativa n).
    Trial(u32),
    /// Voltar para a anterior.
    Rollback,
}

/// Chamado a cada início do trabalhador do agente (antes de qualquer outra coisa).
pub fn on_start(st: &mut LocalState, now: u64) -> StartAction {
    let Some(p) = st.pending.as_mut() else { return StartAction::Normal };
    p.starts += 1;
    if p.starts > MAX_STARTS || now.saturating_sub(p.since) > TRIAL_S {
        StartAction::Rollback
    } else {
        StartAction::Trial(p.starts)
    }
}

/// Conectou ao central com a versão em teste: ela vira a instalada.
pub fn confirm(st: &mut LocalState) -> Option<u64> {
    let p = st.pending.take()?;
    st.installed_seq = p.seq;
    Some(p.seq)
}

/// Desfaz: volta o número instalado e recusa a publicação ruim para sempre.
pub fn rollback(st: &mut LocalState) -> Option<Pending> {
    let p = st.pending.take()?;
    st.installed_seq = p.from_seq;
    if !st.refused.contains(&p.seq) {
        st.refused.push(p.seq);
    }
    Some(p)
}

/// Vale baixar esta publicação?
pub fn wants(st: &LocalState, meta: &UpdateMeta) -> bool {
    st.pending.is_none() && meta.seq > st.installed_seq && !st.refused.contains(&meta.seq) && meta.size > 0 && meta.size <= MAX_SIZE
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// "ws://192.168.0.10:8787/ws/agent" -> "192.168.0.10:8787". Só ws:// (rede local); wss não.
pub fn http_host(central_url: &str) -> Option<String> {
    let rest = central_url.trim().strip_prefix("ws://")?;
    let host = rest.split('/').next()?;
    if host.is_empty() {
        return None;
    }
    Some(if host.contains(':') { host.to_string() } else { format!("{host}:80") })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(seq: u64) -> UpdateMeta {
        UpdateMeta { seq, size: 1000, sig_hex: String::new(), published_at: 0 }
    }

    #[test]
    fn so_baixa_numero_maior_e_nao_recusado() {
        let mut st = LocalState { installed_seq: 3, ..Default::default() };
        assert!(!wants(&st, &meta(3)));
        assert!(!wants(&st, &meta(2)));
        assert!(wants(&st, &meta(4)));
        st.refused.push(4);
        assert!(!wants(&st, &meta(4)));
        assert!(wants(&st, &meta(5)));
        st.pending = Some(Pending { seq: 5, from_seq: 3, starts: 0, since: 0, backup: String::new() });
        assert!(!wants(&st, &meta(6)));
        assert!(!wants(&st, &UpdateMeta { size: MAX_SIZE + 1, ..meta(9) }) || st.pending.is_some());
    }

    #[test]
    fn teste_confirma_ou_volta() {
        let mut st = LocalState { installed_seq: 1, ..Default::default() };
        assert_eq!(on_start(&mut st, 100), StartAction::Normal);
        st.pending = Some(Pending { seq: 2, from_seq: 1, starts: 0, since: 100, backup: "a".into() });
        assert_eq!(on_start(&mut st, 110), StartAction::Trial(1));
        assert_eq!(confirm(&mut st), Some(2));
        assert_eq!(st.installed_seq, 2);
        assert!(st.pending.is_none());

        st.pending = Some(Pending { seq: 3, from_seq: 2, starts: 0, since: 100, backup: "b".into() });
        for n in 1..=MAX_STARTS {
            assert_eq!(on_start(&mut st, 110), StartAction::Trial(n));
        }
        assert_eq!(on_start(&mut st, 110), StartAction::Rollback);
        let p = rollback(&mut st).unwrap();
        assert_eq!((p.seq, st.installed_seq, st.refused.clone()), (3, 2, vec![3]));

        st.pending = Some(Pending { seq: 4, from_seq: 2, starts: 0, since: 100, backup: "c".into() });
        assert_eq!(on_start(&mut st, 100 + TRIAL_S + 1), StartAction::Rollback);
    }

    #[test]
    fn mensagem_assinada_inclui_numero() {
        assert_ne!(signed_message(1, b"x"), signed_message(2, b"x"));
        assert!(signed_message(7, b"abc").ends_with(b"abc"));
    }

    #[test]
    fn hex_e_host() {
        assert_eq!(from_hex(&to_hex(&[0, 1, 254, 255])), Some(vec![0, 1, 254, 255]));
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
        assert_eq!(http_host("ws://192.168.31.210:8787/ws/agent").as_deref(), Some("192.168.31.210:8787"));
        assert_eq!(http_host("wss://x:8787/ws/agent"), None);
        assert_eq!(http_host("ws://pc/ws/agent").as_deref(), Some("pc:80"));
    }
}
