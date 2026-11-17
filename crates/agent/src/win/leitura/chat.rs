//! Leitura de PK pelo chat — DESLIGADA por padrão (`chat_enabled = false`).
//!
//! Por que desligada: exige varrer a memória (o que o prompt mestre proíbe no ciclo
//! normal) até acharmos a lista ordenada do chat com o `diag listachat`. Se você ligar,
//! a varredura roda a cada 60 s, não a cada ciclo.
//!
//! Limites conhecidos (sem horário na linha, não há como fazer melhor):
//! - Na primeira varredura, tudo o que já está na memória vira "visto" e NÃO é reportado.
//! - Uma linha com o mesmo texto vista nas últimas 2 h não é reportada de novo. Se X matar
//!   Y duas vezes em 2 h, a segunda morte não vira alerta.

use crate::win::memory::Proc;
use std::collections::HashMap;
use zen_core::chat::{parse_line, ChatConfig, ChatEvent};

const NEEDLE: &[u8] = b"[PK] : ";
const SEEN_TTL_S: u64 = 2 * 3600;

#[derive(Default)]
pub struct ChatReader {
    seen: HashMap<String, u64>,
    baseline_done: bool,
    cfg: ChatConfig,
}

impl ChatReader {
    pub fn scan(&mut self, pids: &[u32], now: u64) -> Vec<ChatEvent> {
        self.seen.retain(|_, t| now.saturating_sub(*t) < SEEN_TTL_S);
        let mut texts = Vec::new();
        for &pid in pids {
            if let Some(p) = Proc::open(pid) {
                collect_lines(&p, &mut texts);
            }
        }
        let mut out = Vec::new();
        for t in texts {
            if self.seen.contains_key(&t) {
                continue;
            }
            self.seen.insert(t.clone(), now);
            if !self.baseline_done {
                continue;
            }
            if let Some(ev) = parse_line(&t, &self.cfg) {
                out.push(ev);
            }
        }
        self.baseline_done = true;
        out
    }
}

fn collect_lines(p: &Proc, out: &mut Vec<String>) {
    const CHUNK: usize = 1 << 20;
    let mut buf = vec![0u8; CHUNK + 256];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            // Lê 256 bytes a mais para não cortar uma linha na borda do bloco.
            let n = (CHUNK + 256).min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                let hay = &buf[..n];
                let mut i = 0;
                while let Some(pos) = find(&hay[i..], NEEDLE) {
                    let s = i + pos;
                    if s >= CHUNK.min(n) {
                        break; // pertence ao próximo bloco
                    }
                    let end = hay[s..].iter().take(160).position(|&b| b == 0).map(|e| s + e);
                    if let Some(e) = end {
                        // Cliente antigo: texto em ANSI/Latin-1.
                        let txt: String = hay[s..e].iter().map(|&b| b as char).collect();
                        out.push(txt);
                    }
                    i = s + NEEDLE.len();
                }
            }
            off += CHUNK.min(size - off);
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Todas as linhas `[PK] : ...` do processo, com o endereço (o texto começa no `[PK]`).
/// Varre a memória inteira (~10 s num cliente): só é chamada quando um char MORRE, para saber
/// quem matou — nunca no ciclo normal. Ver docs/decisoes/0013.
pub fn pk_lines(pid: u32) -> Vec<(u64, String)> {
    lines_with(pid, NEEDLE)
}

/// Todas as linhas do chat que começam com `needle`, com o endereço. Varre a memória inteira
/// (vários segundos): só para conferências pontuais (morte, Pick do Zen), nunca no ciclo.
pub fn lines_with(pid: u32, needle: &[u8]) -> Vec<(u64, String)> {
    let Some(p) = Proc::open(pid) else { return Vec::new() };
    const CHUNK: usize = 1 << 20;
    let mut out = Vec::new();
    let mut buf = vec![0u8; CHUNK + 256];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = (CHUNK + 256).min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                let hay = &buf[..n];
                let mut i = 0;
                while let Some(pos) = find(&hay[i..], needle) {
                    let s = i + pos;
                    if s >= CHUNK.min(n) {
                        break;
                    }
                    if let Some(e) = hay[s..].iter().take(160).position(|&b| b == 0).map(|e| s + e) {
                        let txt: String = hay[s..e].iter().map(|&b| b as char).collect();
                        out.push((base + off as u64 + s as u64, txt));
                    }
                    i = s + needle.len();
                }
            }
            off += CHUNK.min(size - off);
        }
    }
    out.sort();
    out.dedup();
    out
}
