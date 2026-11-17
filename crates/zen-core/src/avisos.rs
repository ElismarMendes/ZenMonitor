//! Avisos amarelos do servidor (texto que o servidor manda para todos: invasões, eventos,
//! itens criados, cupons). O agente lê da memória e manda o texto; a central junta, grava em
//! `logs\avisos.log` e manda para o celular os que batem com `avisos_celular`.
//!
//! O mesmo aviso chega em TODOS os clientes (10 chars em 5 PCs): a central só aceita um texto
//! igual uma vez por janela.

use std::collections::HashMap;

/// Texto de aviso aceitável: ASCII visível, 6..=200 letras, sem lixo de memória.
pub fn clean(raw: &[u8]) -> Option<String> {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let s = &raw[..end];
    if s.len() < 6 || s.len() > 200 {
        return None;
    }
    // Latin-1: o servidor escreve em português sem acento ou com acento ANSI.
    if s.iter().any(|&b| b < 32 || b == 127) {
        return None;
    }
    let letters = s.iter().filter(|b| b.is_ascii_alphabetic()).count();
    if letters * 2 < s.len() {
        return None;
    }
    Some(s.iter().map(|&b| b as char).collect::<String>().trim().to_string())
}

/// Bate com alguma palavra-chave (sem diferenciar maiúsculas).
pub fn matches(text: &str, keywords: &[String]) -> bool {
    let t = text.to_lowercase();
    keywords.iter().map(|k| k.trim().to_lowercase()).any(|k| !k.is_empty() && t.contains(&k))
}

/// Palavras padrão para o celular: invasões de goldens.
pub fn default_keywords() -> Vec<String> {
    ["golden", "invasion", "invasão", "invasao"].iter().map(|s| s.to_string()).collect()
}

/// Lembra os textos já vistos para não repetir o mesmo aviso vindo de vários chars/PCs.
#[derive(Debug, Default)]
pub struct Dedupe {
    seen: HashMap<String, u64>,
}

impl Dedupe {
    /// true = texto novo (ou visto há mais de `window_s`).
    pub fn accept(&mut self, text: &str, now: u64, window_s: u64) -> bool {
        self.seen.retain(|_, t| now.saturating_sub(*t) < window_s);
        if self.seen.contains_key(text) {
            return false;
        }
        self.seen.insert(text.to_string(), now);
        true
    }
}

/// Fila de avisos do cliente: `slots` textos de `stride` bytes seguidos. Devolve os textos
/// válidos na ordem da memória (o mais novo primeiro).
pub fn parse_slots(buf: &[u8], stride: usize) -> Vec<String> {
    if stride == 0 {
        return Vec::new();
    }
    buf.chunks(stride).filter_map(clean).collect()
}

/// Do lado do agente. A tela mostra 6 linhas de aviso; cada aviso novo entra embaixo e sobe
/// uma linha a cada ~10 s até sumir (~1 min na tela). Aviso novo = texto que está na fila agora
/// e não estava na leitura anterior. Assim o mesmo texto repetido mais tarde (ex.: o quiz de
/// toda hora) volta a ser avisado. A primeira leitura só guarda a referência.
#[derive(Debug, Default)]
pub struct Watcher {
    prev: Vec<String>,
    started: bool,
}

impl Watcher {
    pub fn update(&mut self, current: Vec<String>) -> Vec<String> {
        let mut fresh: Vec<String> = Vec::new();
        if self.started {
            for t in &current {
                if !self.prev.contains(t) && !fresh.contains(t) {
                    fresh.push(t.clone());
                }
            }
        }
        self.started = true;
        self.prev = current;
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limpa_texto() {
        assert_eq!(clean(b"ShakaMG criou um Volcano Gloves +10\0lixo").as_deref(), Some("ShakaMG criou um Volcano Gloves +10"));
        assert_eq!(clean(b"abc\0"), None);
        assert_eq!(clean(b"\x01\x02\x03\x04\x05\x06\x07"), None);
        assert_eq!(clean(b"12345678+-*/"), None);
    }

    #[test]
    fn palavra_chave() {
        let k = default_keywords();
        assert!(matches("[Golden Invasion] started in Lorencia", &k));
        assert!(matches("Invasão dos goldens!", &k));
        assert!(!matches("Use the 10% bonus coupon", &k));
        assert!(!matches("qualquer coisa", &["".into()]));
    }

    #[test]
    fn nao_repete() {
        let mut d = Dedupe::default();
        assert!(d.accept("a b c d e f", 100, 600));
        assert!(!d.accept("a b c d e f", 200, 600));
        assert!(d.accept("a b c d e f", 800, 600));
    }

    #[test]
    fn fila() {
        let mut buf = vec![0u8; 0x108 * 3];
        buf[..12].copy_from_slice(b"Aviso novo A");
        buf[0x108..0x108 + 13].copy_from_slice(b"Aviso velho B");
        let v = parse_slots(&buf, 0x108);
        assert_eq!(v, vec!["Aviso novo A".to_string(), "Aviso velho B".to_string()]);
    }

    #[test]
    fn watcher_ignora_o_que_ja_estava() {
        let mut w = Watcher::default();
        assert!(w.update(vec!["velho aviso".into()]).is_empty());
        assert_eq!(w.update(vec!["novo aviso".into(), "velho aviso".into()]), vec!["novo aviso".to_string()]);
        assert!(w.update(vec!["novo aviso".into(), "velho aviso".into()]).is_empty());
        // Sumiu da tela e voltou depois: é um aviso novo (quiz de toda hora).
        assert!(w.update(vec![]).is_empty());
        assert_eq!(w.update(vec!["velho aviso".into()]), vec!["velho aviso".to_string()]);
    }
}
