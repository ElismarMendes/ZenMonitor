//! Zen/h e "há quanto tempo parou de catar".
//!
//! Regras:
//! - Só subidas entre 1 e `MAX_FARM_STEP` contam como farm. Salto maior = baú/troca.
//! - Descida (gasto, depósito) não conta e não zera nada.
//! - Leitura ausente (`None`) não quebra a série; só não soma.
//! - Zen/h nunca é negativo.

use crate::MAX_FARM_STEP;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct ZenTracker {
    window_s: u64,
    /// (tempo, ganho de farm naquele passo)
    gains: VecDeque<(u64, u64)>,
    last: Option<(u64, u64)>,
    first_seen: Option<u64>,
    last_increase_at: Option<u64>,
}

impl ZenTracker {
    pub fn new(window_s: u64) -> Self {
        Self {
            window_s: window_s.max(60),
            gains: VecDeque::new(),
            last: None,
            first_seen: None,
            last_increase_at: None,
        }
    }

    /// Registra uma leitura. `now` deve ser monotônico; leituras fora de ordem são ignoradas.
    pub fn push(&mut self, now: u64, zen: u64) {
        if let Some((t, _)) = self.last {
            if now < t {
                return;
            }
        }
        self.first_seen.get_or_insert(now);
        if let Some((_, prev)) = self.last {
            if zen > prev {
                let step = zen - prev;
                if step <= MAX_FARM_STEP {
                    self.gains.push_back((now, step));
                    self.last_increase_at = Some(now);
                } else {
                    // Baú/troca: não é farm, mas o char continua "vivo" — não marcamos
                    // como subida de farm para não esconder um PAROU real.
                }
            }
        }
        self.last = Some((now, zen));
        self.prune(now);
    }

    fn prune(&mut self, now: u64) {
        let cut = now.saturating_sub(self.window_s);
        while matches!(self.gains.front(), Some((t, _)) if *t <= cut) {
            self.gains.pop_front();
        }
    }

    /// Zen por hora na janela. Até a janela encher, divide pelo tempo observado
    /// (mínimo 60 s, para não explodir no começo).
    pub fn per_hour(&self, now: u64) -> f64 {
        let Some(first) = self.first_seen else { return 0.0 };
        let cut = now.saturating_sub(self.window_s);
        let sum: u64 = self.gains.iter().filter(|(t, _)| *t > cut).map(|(_, g)| *g).sum();
        let span = now.saturating_sub(first).min(self.window_s).max(60);
        sum as f64 * 3600.0 / span as f64
    }

    /// Segundos desde a última subida de farm. Se nunca subiu, conta desde a primeira leitura.
    pub fn idle_secs(&self, now: u64) -> Option<u64> {
        let since = self.last_increase_at.or(self.first_seen)?;
        Some(now.saturating_sub(since))
    }

    pub fn last_zen(&self) -> Option<u64> {
        self.last.map(|(_, z)| z)
    }
}

/// Segundos até chegar ao limite no ritmo atual. `None` se não sobe.
pub fn eta_to_limit(zen: u64, per_hour: f64) -> Option<u64> {
    if per_hour <= 0.0 || !per_hour.is_finite() {
        return None;
    }
    let left = crate::ZEN_LIMIT.saturating_sub(zen) as f64;
    Some((left / per_hour * 3600.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ritmo_constante() {
        let mut z = ZenTracker::new(3600);
        let mut zen = 100_000_000;
        for i in 0..=720 {
            // 5 s por ciclo, +500k por ciclo = 360M/h
            z.push(i * 5, zen);
            zen += 500_000;
        }
        let r = z.per_hour(3600);
        assert!((r - 360_000_000.0).abs() < 1_000_000.0, "{r}");
        assert_eq!(z.idle_secs(3600), Some(0));
    }

    #[test]
    fn bau_nao_e_farm() {
        let mut z = ZenTracker::new(3600);
        z.push(0, 100_000_000);
        z.push(5, 600_000_000); // tirou 500M do baú
        assert_eq!(z.per_hour(5), 0.0);
        assert_eq!(z.idle_secs(185), Some(185));
    }

    #[test]
    fn descida_nao_soma_nem_zera() {
        let mut z = ZenTracker::new(3600);
        z.push(0, 100);
        z.push(5, 200);
        z.push(10, 50);
        z.push(15, 150);
        // ganhos: 100 + 100
        assert!((z.per_hour(60) - 200.0 * 60.0).abs() < 1e-6);
    }

    #[test]
    fn fora_de_ordem_ignorado() {
        let mut z = ZenTracker::new(3600);
        z.push(100, 10);
        z.push(50, 1_000);
        assert_eq!(z.last_zen(), Some(10));
    }

    #[test]
    fn eta() {
        assert_eq!(eta_to_limit(1_900_000_000, 200_000_000.0), Some(1800));
        assert_eq!(eta_to_limit(1, 0.0), None);
        assert_eq!(eta_to_limit(3_000_000_000, 1.0), Some(0));
    }
}
