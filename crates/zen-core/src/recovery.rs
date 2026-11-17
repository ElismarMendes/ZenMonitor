//! Quando o agente deve levar um char de volta ao spot. Puro: o agente informa o que viu na
//! tela e executa a recuperação se esta regra pedir. Ver docs/decisoes/0012.
//!
//! Medido em 08/11/2026: depois de DC o cliente reconecta sozinho e o char aparece em
//! Lorencia com o helper desligado; depois de morrer/teleportar para a zona segura o helper
//! também desliga ("If move to SafeZone it will be necessary to reactivate").
//!
//! Gatilhos (todos exigem spot salvo, conexão ok e a situação durar `wait_s`):
//! - coordenadas legíveis e o mapa NÃO é Kanturu -> "fora do Kanturu";
//! - Kanturu, helper DESLIGADO -> "helper desligado" (longe ou perto do spot).
//! - Kanturu, helper LIGADO mas a mais de `far_tiles` do spot por `far_wait_s` -> "longe do
//!   spot" (visto em 08/11: recuperação desistiu no meio e o helper ficou ligado num canto
//!   sem mobs, 0 Zen/h). Perto do spot com o helper ligado = farmando: nunca mexe.
//! Tela ilegível = não sabe: nunca mexe.
//! No máximo `max_per_hour` tentativas por hora; depois de falha espera `retry_s`.

use std::collections::VecDeque;

/// Espera depois de morrer num spot que já era dele (só o tempo de a tela assentar).
pub const RUSH_WAIT_S: u64 = 5;

/// Tempo farmando no spot (helper ligado, perto) para o spot contar como SEU (pedido em 08/11:
/// "1 minuto lá já é meu"). Morreu antes disso = acabou de chegar ou voltou e morreu de novo:
/// não volta sozinho (trava contra ciclo de morte).
pub const OWN_SPOT_S: u64 = 60;

#[derive(Debug, Clone, Copy)]
pub struct RecoveryConfig {
    pub wait_s: u64,
    pub far_tiles: i32,
    pub far_wait_s: u64,
    pub retry_s: u64,
    /// No máximo `max_per_hour` recuperações dentro de `window_s` (painel: "N voltas em M min").
    pub max_per_hour: usize,
    pub window_s: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self { wait_s: 30, far_tiles: 10, far_wait_s: 40, retry_s: 120, max_per_hour: 3, window_s: 600 }
    }
}

/// O que o agente leu da tela do char neste ciclo.
#[derive(Debug, Clone, Copy, Default)]
pub struct Observation {
    pub in_kanturu: Option<bool>,
    pub pos: Option<(i32, i32)>,
    pub helper_on: Option<bool>,
    pub disconnected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    OutsideKanturu,
    HelperOff,
    FarFromSpot,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::OutsideKanturu => "fora do Kanturu",
            Trigger::HelperOff => "helper desligado",
            Trigger::FarFromSpot => "longe do spot",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RecoveryMachine {
    bad_since: Option<(u64, Option<Trigger>)>,
    attempts: VecDeque<u64>,
    wait_until: u64,
    pub gave_up: bool,
    /// Próxima recuperação espera exatamente isto (s) em vez de `wait_s`: morreu no spot que já
    /// era dele; o agente sorteia o valor entre o mínimo e o máximo do painel.
    pub rush_wait: Option<u64>,
}

impl RecoveryMachine {
    pub fn situation(o: &Observation, spot: Option<(i32, i32)>, c: &RecoveryConfig) -> Option<Trigger> {
        if o.disconnected {
            return None;
        }
        match (o.in_kanturu, o.pos, o.helper_on) {
            (Some(false), Some(_), _) => Some(Trigger::OutsideKanturu),
            (Some(true), Some(_), Some(false)) => Some(Trigger::HelperOff),
            (Some(true), Some(p), Some(true)) => {
                let s = spot?;
                ((p.0 - s.0).abs().max((p.1 - s.1).abs()) > c.far_tiles).then_some(Trigger::FarFromSpot)
            }
            _ => None,
        }
    }

    /// Devolve o gatilho quando é hora de recuperar. `spot` vazio = este char não tem spot.
    pub fn decide(&mut self, now: u64, o: &Observation, spot: Option<(i32, i32)>, c: &RecoveryConfig) -> Option<Trigger> {
        let t = if spot.is_some() { Self::situation(o, spot, c) } else { None };
        let Some(t) = t else {
            self.bad_since = None;
            // Voltou a farmar perto do spot: libera nova rodada de tentativas.
            if spot.is_some() && o.helper_on == Some(true) && o.in_kanturu == Some(true) {
                self.gave_up = false;
            }
            return None;
        };
        // Relógio do PC acertado para trás (DESKTOP-E1GLBEF, 12/11: 5 h; o AMUMU ficou 46 min
        // parado no spot com o helper desligado esperando a hora antiga passar): horas no futuro
        // não valem.
        if self.wait_until > now + 86_400 {
            self.wait_until = 0;
        }
        self.attempts.retain(|&a| a <= now);
        let since = match self.bad_since {
            Some((s, Some(prev))) if prev == t && s <= now => s,
            _ => {
                self.bad_since = Some((now, Some(t)));
                now
            }
        };
        while self.attempts.front().is_some_and(|&a| now.saturating_sub(a) >= c.window_s) {
            self.attempts.pop_front();
        }
        if self.attempts.len() >= c.max_per_hour {
            self.gave_up = true;
            return None;
        }
        let wait = if let (Some(w), true) = (self.rush_wait, t != Trigger::FarFromSpot) {
            w
        } else if t == Trigger::FarFromSpot {
            c.far_wait_s
        } else {
            c.wait_s
        };
        if now.saturating_sub(since) >= wait && now >= self.wait_until {
            self.attempts.push_back(now);
            self.bad_since = None;
            self.rush_wait = None;
            return Some(t);
        }
        None
    }

    pub fn finished(&mut self, now: u64, ok: bool, c: &RecoveryConfig) {
        self.wait_until = now + if ok { 60 } else { c.retry_s };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: RecoveryConfig = RecoveryConfig { wait_s: 30, far_tiles: 10, far_wait_s: 180, retry_s: 120, max_per_hour: 3, window_s: 3600 };
    const SPOT: Option<(i32, i32)> = Some((123, 233));

    fn town() -> Observation {
        Observation { in_kanturu: Some(false), pos: Some((125, 121)), helper_on: Some(false), disconnected: false }
    }
    fn farming() -> Observation {
        Observation { in_kanturu: Some(true), pos: Some((123, 233)), helper_on: Some(true), disconnected: false }
    }
    fn safezone() -> Observation {
        Observation { in_kanturu: Some(true), pos: Some((20, 218)), helper_on: Some(false), disconnected: false }
    }

    #[test]
    fn dc_em_lorencia_recupera_depois_de_30s() {
        let mut m = RecoveryMachine::default();
        assert_eq!(m.decide(0, &town(), SPOT, &C), None);
        assert_eq!(m.decide(29, &town(), SPOT, &C), None);
        assert_eq!(m.decide(30, &town(), SPOT, &C), Some(Trigger::OutsideKanturu));
    }

    #[test]
    fn farmando_ou_tela_ilegivel_ou_sem_spot_nunca_mexe() {
        let mut m = RecoveryMachine::default();
        for t in 0..1000 {
            assert_eq!(m.decide(t, &farming(), SPOT, &C), None);
            assert_eq!(m.decide(t, &Observation::default(), SPOT, &C), None);
            assert_eq!(m.decide(t, &town(), None, &C), None);
        }
    }

    #[test]
    fn helper_ligado_longe_do_spot_por_3_min_recupera() {
        let mut m = RecoveryMachine::default();
        let far = Observation { in_kanturu: Some(true), pos: Some((138, 242)), helper_on: Some(true), disconnected: false };
        assert_eq!(m.decide(0, &far, SPOT, &C), None);
        assert_eq!(m.decide(179, &far, SPOT, &C), None);
        assert_eq!(m.decide(180, &far, SPOT, &C), Some(Trigger::FarFromSpot));
        // perto (até 10 tiles) com helper ligado: farmando, nunca mexe
        let mut m = RecoveryMachine::default();
        let near = Observation { pos: Some((130, 240)), ..far };
        for t in 0..1000 {
            assert_eq!(m.decide(t, &near, SPOT, &C), None);
        }
    }

    #[test]
    fn caido_nao_mexe() {
        let mut m = RecoveryMachine::default();
        let mut o = town();
        o.disconnected = true;
        for t in 0..100 {
            assert_eq!(m.decide(t, &o, SPOT, &C), None);
        }
    }

    #[test]
    fn situacao_tem_de_durar_e_reinicia_se_mudar() {
        let mut m = RecoveryMachine::default();
        m.decide(0, &town(), SPOT, &C);
        m.decide(20, &farming(), SPOT, &C); // voltou a farmar: zera
        assert_eq!(m.decide(40, &safezone(), SPOT, &C), None);
        assert_eq!(m.decide(70, &safezone(), SPOT, &C), Some(Trigger::HelperOff));
    }

    #[test]
    fn falha_espera_e_desiste_depois_de_3_por_hora() {
        let mut m = RecoveryMachine::default();
        m.decide(0, &town(), SPOT, &C);
        assert!(m.decide(30, &town(), SPOT, &C).is_some());
        m.finished(90, false, &C);
        assert_eq!(m.decide(150, &town(), SPOT, &C), None, "espera 2 min depois da falha");
        assert!(m.decide(210, &town(), SPOT, &C).is_some());
        m.finished(260, false, &C);
        m.decide(300, &town(), SPOT, &C);
        assert!(m.decide(400, &town(), SPOT, &C).is_some());
        m.finished(450, false, &C);
        for t in 600..1200 {
            assert_eq!(m.decide(t, &town(), SPOT, &C), None, "3 por hora: desistiu");
        }
        assert!(m.gave_up);
        // uma hora depois da 1ª tentativa, libera de novo (a situação já durava desde 600)
        assert!(m.decide(3700, &town(), SPOT, &C).is_some());
    }

    #[test]
    fn rush_espera_so_5s_uma_vez() {
        let mut m = RecoveryMachine::default();
        m.rush_wait = Some(RUSH_WAIT_S);
        assert_eq!(m.decide(1000, &town(), SPOT, &C), None);
        assert_eq!(m.decide(1005, &town(), SPOT, &C), Some(Trigger::OutsideKanturu));
        assert!(m.rush_wait.is_none());
        m.finished(1060, true, &C);
        assert_eq!(m.decide(1200, &town(), SPOT, &C), None);
        assert_eq!(m.decide(1205, &town(), SPOT, &C), None); // sem rush: 30 s de novo
        assert_eq!(m.decide(1230, &town(), SPOT, &C), Some(Trigger::OutsideKanturu));
    }

    #[test]
    fn janela_configuravel() {
        let c = RecoveryConfig { window_s: 600, ..C };
        let mut m = RecoveryMachine::default();
        let mut t = 0;
        for _ in 0..3 {
            m.decide(t, &town(), SPOT, &c);
            assert!(m.decide(t + 30, &town(), SPOT, &c).is_some());
            m.finished(t + 31, true, &c);
            t += 100;
        }
        // 3 em 10 min: a 4ª espera a primeira sair da janela.
        m.decide(t, &town(), SPOT, &c);
        assert!(m.decide(t + 30, &town(), SPOT, &c).is_none());
        assert!(m.gave_up);
        // 10 min depois da primeira, ela sai da janela e libera mais uma.
        assert!(m.decide(629, &town(), SPOT, &c).is_none());
        assert!(m.decide(631, &town(), SPOT, &c).is_some());
    }

    #[test]
    fn relogio_para_tras_nao_trava_a_volta() {
        let mut m = RecoveryMachine::default();
        let off = Observation { in_kanturu: Some(true), pos: Some(SPOT.unwrap()), helper_on: Some(false), disconnected: false };
        assert_eq!(m.decide(50_000, &off, SPOT, &C), None);
        // o relógio volta 5 h: conta os 30 s a partir de agora, não espera 5 h
        assert_eq!(m.decide(32_000, &off, SPOT, &C), None);
        assert_eq!(m.decide(32_030, &off, SPOT, &C), Some(Trigger::HelperOff));
    }
}
