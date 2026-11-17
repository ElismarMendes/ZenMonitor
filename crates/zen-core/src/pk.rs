//! Deduplicação de PKs.
//!
//! A mesma linha `[PK] : X killed Y` aparece em várias cópias na RAM e em vários PCs
//! (é linha global). Sem horário, não dá para provar que duas linhas iguais são dois
//! eventos. Regra adotada: a mesma dupla (quem matou, vítima) conta UMA vez a cada
//! `pair_window_s` (padrão 30 min), venha de qual PC vier.
//!
//! Nota crítica: a regra de "90 s entre PCs" do prompt mestre é coberta por esta
//! (30 min > 90 s), então não existe à parte. O custo conhecido: se X matar Y duas vezes
//! em 30 min, conta 1. É o preço de não ter horário na linha.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PkDeduper {
    pair_window_s: u64,
    seen: HashMap<(String, String), u64>,
}

impl PkDeduper {
    pub fn new(pair_window_s: u64) -> Self {
        Self {
            pair_window_s,
            seen: HashMap::new(),
        }
    }

    /// `true` se este PK é novo e deve ser registrado.
    pub fn accept(&mut self, killer: &str, victim: &str, now: u64) -> bool {
        self.prune(now);
        let key = (killer.to_ascii_lowercase(), victim.to_ascii_lowercase());
        match self.seen.get(&key) {
            Some(&t) if now.saturating_sub(t) < self.pair_window_s => false,
            _ => {
                self.seen.insert(key, now);
                true
            }
        }
    }

    fn prune(&mut self, now: u64) {
        if self.seen.len() > 4096 {
            let w = self.pair_window_s;
            self.seen.retain(|_, t| now.saturating_sub(*t) < w);
        }
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesma_dupla_conta_uma_vez() {
        let mut d = PkDeduper::new(1800);
        assert!(d.accept("X", "Y", 0));
        for t in 1..20 {
            assert!(!d.accept("X", "Y", t * 5));
        }
        assert!(d.accept("X", "Y", 1800));
    }

    #[test]
    fn duplas_diferentes_contam() {
        let mut d = PkDeduper::new(1800);
        assert!(d.accept("X", "Y", 0));
        assert!(d.accept("X", "Z", 0));
        assert!(d.accept("Y", "X", 0));
    }

    #[test]
    fn maiusculas_nao_enganam() {
        let mut d = PkDeduper::new(1800);
        assert!(d.accept("Xx", "Yy", 0));
        assert!(!d.accept("XX", "yy", 10));
    }
}
