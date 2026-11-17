//! Núcleo do ZenMonitor 3.0.
//!
//! Regra de ouro: nada aqui toca Windows, rede, disco ou relógio. Todo tempo chega
//! de fora como segundos Unix (`u64`). Isso deixa cada regra testável em milissegundos
//! e garante que agente e central decidem igual.

pub mod spots;
pub mod ks;
pub mod actions;
pub mod alerts;
pub mod avisos;
pub mod chat;
pub mod combat;
pub mod envfile;
pub mod fmt;
pub mod glyphs;
pub mod hp;
pub mod hplock;
pub mod pk;
pub mod protocol;
pub mod objects;
pub mod petbar;
pub mod ramlock;
pub mod skillicon;
pub mod recovery;
pub mod rate;
pub mod terrain;
pub mod title;
pub mod vault;
pub mod update;

/// Limite de Zen no inventário do MU.
pub const ZEN_LIMIT: u64 = 2_000_000_000;

/// Subida maior que isso num único ciclo é baú/troca, não farm.
pub const MAX_FARM_STEP: u64 = 30_000_000;

/// Remove caracteres de controle e corta no tamanho máximo. Todo texto vindo do jogo
/// (nomes de jogadores) passa por aqui antes de ir para qualquer lugar.
pub fn sanitize(s: &str, max_chars: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_remove_controle_e_corta() {
        assert_eq!(sanitize("  ab\u{0}c\n ", 10), "abc");
        assert_eq!(sanitize("abcdef", 3), "abc");
        assert_eq!(sanitize("<script>", 20), "<script>"); // escapar é papel do painel
    }
}
