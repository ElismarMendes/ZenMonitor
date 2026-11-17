//! Regras de alerta. Função pura: recebe o estado e o "agora" do CENTRAL.

use crate::ZEN_LIMIT;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlertKind {
    // A ordem da declaração É a prioridade de exibição.
    Offline,
    Dc,
    Limit2B,
    Stopped,
    Dead,
    Near2B,
}

impl AlertKind {
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Offline => "SEM SINAL",
            AlertKind::Dc => "DC",
            AlertKind::Limit2B => "2 BI",
            AlertKind::Stopped => "PAROU",
            AlertKind::Dead => "MORTO",
            AlertKind::Near2B => "QUASE 2 BI",
        }
    }

    /// Críticos vão para o celular.
    pub fn is_critical(self) -> bool {
        !matches!(self, AlertKind::Near2B)
    }
}

#[derive(Debug, Clone)]
pub struct AlertConfig {
    pub offline_after_s: u64,
    pub stopped_after_s: u64,
    pub limit_threshold: u64,
    pub near_limit_eta_s: u64,
    pub dead_window_s: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            offline_after_s: 20,
            stopped_after_s: 180,
            limit_threshold: 1_999_000_000,
            near_limit_eta_s: 30 * 60,
            dead_window_s: 10 * 60,
        }
    }
}

/// Tudo que a regra precisa saber de um char, já calculado.
#[derive(Debug, Clone, Default)]
pub struct CharSnapshot {
    /// Hora (do central) do último relatório da máquina deste char.
    pub last_report_at: Option<u64>,
    /// O agente viu a conexão TCP cair por 2 ciclos.
    pub disconnected: bool,
    /// Zen confiável (trava de RAM ok). `None` = desconhecido: nenhuma regra de Zen dispara.
    pub zen: Option<u64>,
    pub zen_per_hour: f64,
    /// Segundos sem subida de farm (de `ZenTracker::idle_secs`).
    pub idle_secs: Option<u64>,
    /// Hora (do central) da última morte registrada deste char.
    pub last_death_at: Option<u64>,
}

pub fn evaluate(s: &CharSnapshot, now: u64, cfg: &AlertConfig) -> Vec<AlertKind> {
    let mut out = Vec::new();

    let offline = match s.last_report_at {
        None => true,
        Some(t) => now.saturating_sub(t) > cfg.offline_after_s,
    };
    if offline {
        // Sem sinal, os outros dados estão velhos: não inventar alerta em cima deles.
        out.push(AlertKind::Offline);
        return out;
    }
    if s.disconnected {
        out.push(AlertKind::Dc);
    }
    if let Some(zen) = s.zen {
        let at_limit = zen >= cfg.limit_threshold;
        if at_limit {
            out.push(AlertKind::Limit2B);
        }
        if !s.disconnected && !at_limit {
            if let Some(idle) = s.idle_secs {
                if idle >= cfg.stopped_after_s {
                    out.push(AlertKind::Stopped);
                }
            }
        }
        if !at_limit {
            if let Some(eta) = crate::rate::eta_to_limit(zen.min(ZEN_LIMIT), s.zen_per_hour) {
                if eta < cfg.near_limit_eta_s {
                    out.push(AlertKind::Near2B);
                }
            }
        }
    }
    if let Some(d) = s.last_death_at {
        if now.saturating_sub(d) < cfg.dead_window_s {
            out.push(AlertKind::Dead);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Evita repetir a mesma notificação no celular a cada 5 s.
/// Notifica quando o alerta APARECE; de novo só se sumir e voltar depois de `cooldown_s`.
#[derive(Debug, Default, Clone)]
pub struct NotifyGate {
    active: std::collections::HashMap<(String, AlertKind), u64>,
    last_sent: std::collections::HashMap<(String, AlertKind), u64>,
}

impl NotifyGate {
    /// Recebe os alertas atuais do char e devolve os que devem ser notificados agora.
    pub fn update(&mut self, key: &str, current: &[AlertKind], now: u64, cooldown_s: u64) -> Vec<AlertKind> {
        let mut send = Vec::new();
        for &k in current {
            let id = (key.to_string(), k);
            if self.active.contains_key(&id) {
                continue;
            }
            self.active.insert(id.clone(), now);
            let ok = match self.last_sent.get(&id) {
                Some(&t) => now.saturating_sub(t) >= cooldown_s,
                None => true,
            };
            if ok && k.is_critical() {
                self.last_sent.insert(id, now);
                send.push(k);
            }
        }
        self.active.retain(|(c, k), _| c != key || current.contains(k));
        send
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AlertKind::*;

    fn base() -> CharSnapshot {
        CharSnapshot {
            last_report_at: Some(1000),
            disconnected: false,
            zen: Some(500_000_000),
            zen_per_hour: 200_000_000.0,
            idle_secs: Some(0),
            last_death_at: None,
        }
    }

    fn ev(s: &CharSnapshot, now: u64) -> Vec<AlertKind> {
        evaluate(s, now, &AlertConfig::default())
    }

    #[test]
    fn tudo_bem() {
        assert!(ev(&base(), 1005).is_empty());
    }

    #[test]
    fn offline_esconde_o_resto() {
        let mut s = base();
        s.idle_secs = Some(9999);
        assert_eq!(ev(&s, 1021), vec![Offline]);
        assert!(ev(&s, 1020).contains(&Stopped));
    }

    #[test]
    fn parou_na_fronteira() {
        let mut s = base();
        s.idle_secs = Some(179);
        assert!(!ev(&s, 1000).contains(&Stopped));
        s.idle_secs = Some(180);
        assert!(ev(&s, 1000).contains(&Stopped));
    }

    #[test]
    fn no_limite_nao_e_parou() {
        let mut s = base();
        s.zen = Some(1_999_000_000);
        s.idle_secs = Some(10_000);
        assert_eq!(ev(&s, 1000), vec![Limit2B]);
    }

    #[test]
    fn quase_2bi() {
        let mut s = base();
        s.zen = Some(1_950_000_000);
        s.zen_per_hour = 200_000_000.0; // 50M faltando = 15 min
        assert_eq!(ev(&s, 1000), vec![Near2B]);
        s.zen_per_hour = 50_000_000.0; // 1 h
        assert!(ev(&s, 1000).is_empty());
    }

    #[test]
    fn dc_nao_vira_parou() {
        let mut s = base();
        s.disconnected = true;
        s.idle_secs = Some(9999);
        assert_eq!(ev(&s, 1000), vec![Dc]);
    }

    #[test]
    fn zen_desconhecido_nao_inventa() {
        let mut s = base();
        s.zen = None;
        s.idle_secs = Some(9999);
        assert!(ev(&s, 1000).is_empty());
    }

    #[test]
    fn morte_expira() {
        let mut s = base();
        s.last_death_at = Some(1000);
        assert_eq!(ev(&s, 1000), vec![Dead]);
        assert!(ev(
            &CharSnapshot {
                last_report_at: Some(1600),
                ..s.clone()
            },
            1600
        )
        .is_empty());
    }

    #[test]
    fn prioridade() {
        let mut s = base();
        s.disconnected = true;
        s.zen = Some(1_999_500_000);
        s.last_death_at = Some(999);
        assert_eq!(ev(&s, 1000), vec![Dc, Limit2B, Dead]);
    }

    #[test]
    fn notificacao_nao_repete() {
        let mut g = NotifyGate::default();
        assert_eq!(g.update("a", &[Stopped], 0, 600), vec![Stopped]);
        assert!(g.update("a", &[Stopped], 5, 600).is_empty());
        assert!(g.update("a", &[], 10, 600).is_empty());
        assert!(g.update("a", &[Stopped], 15, 600).is_empty()); // voltou cedo demais
        assert!(g.update("a", &[], 20, 600).is_empty());
        assert_eq!(g.update("a", &[Stopped], 700, 600), vec![Stopped]);
        assert!(g.update("a", &[Near2B], 800, 600).is_empty()); // não crítico
    }
}
