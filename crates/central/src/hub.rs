//! Estado do central. Sem I/O e sem serde: testável sozinho. Efeitos colaterais
//! (gravar no banco) saem como `HubOut` para o `main` executar.

use std::collections::{BTreeMap, VecDeque};
use zen_core::alerts::{evaluate, AlertConfig, AlertKind, CharSnapshot, NotifyGate};
use zen_core::chat::{ChatEvent, KillerKind};
use zen_core::pk::PkDeduper;
use zen_core::protocol::{CharReport, Health, PanelChar, ZenSource};
use zen_core::rate::{eta_to_limit, ZenTracker};

const SPARK_POINTS: usize = 180; // 3 h, 1 por minuto
const FORGET_CHAR_S: u64 = 3600;
const RECENT_PK: usize = 50;

pub struct CharState {
    tracker: ZenTracker,
    pub last: CharReport,
    pub last_seen_at: u64,
    pub last_death_at: Option<u64>,
    spark: VecDeque<(u64, u64)>,
}

#[derive(Default)]
pub struct Machine {
    pub protocol: u32,
    pub version: String,
    pub channel: String,
    pub connected: bool,
    pub last_report_at: Option<u64>,
    pub health: Health,
    pub chars: BTreeMap<String, CharState>,
    /// Spots catalogados no Kanturu 1 (F2), na ordem.
    pub catalog: Vec<(i32, i32)>,
    /// Spots catalogados nos outros mapas: (World, x, y).
    pub catalog_other: Vec<(u8, i32, i32)>,
    /// Menor (agora_central - sent_at) visto desde o último hello = atraso de rede + diferença de relógio.
    skew: Option<i64>,
    last_ts: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PkRow {
    pub ts: u64,
    pub killer: String,
    pub monster: bool,
    pub victim: String,
    pub machine: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HubOut {
    ZenPoint {
        machine: String,
        char_name: String,
        ts: u64,
        zen: u64,
    },
    Pk(PkRow),
}

pub struct Hub {
    pub machines: BTreeMap<String, Machine>,
    pk: PkDeduper,
    pub recent_pk: VecDeque<PkRow>,
    notify: NotifyGate,
    /// Alertas de cada char no ciclo anterior ("PC1/Teste" -> alertas), para registrar no log
    /// quando um alerta começa e quando termina.
    last_alerts: std::collections::HashMap<String, Vec<AlertKind>>,
    pub alert_cfg: AlertConfig,
}

impl Hub {
    pub fn new(alert_cfg: AlertConfig) -> Self {
        Self {
            machines: BTreeMap::new(),
            pk: PkDeduper::new(30 * 60),
            recent_pk: VecDeque::new(),
            notify: NotifyGate::default(),
            last_alerts: std::collections::HashMap::new(),
            alert_cfg,
        }
    }

    pub fn on_hello(&mut self, machine: &str, version: &str, channel: &str) {
        let m = self.machines.entry(machine.to_string()).or_default();
        m.version = version.to_string();
        m.channel = channel.to_string();
        m.connected = true;
        m.skew = None;
    }

    /// Converte a hora do PC para a do central (mesma correção dos relatórios), sem passar de `now`.
    pub fn central_ts(&self, machine: &str, sent_at: u64, now: u64) -> u64 {
        match self.machines.get(machine).and_then(|m| m.skew) {
            Some(skew) => ((sent_at as i64 + skew).max(0) as u64).min(now),
            None => now,
        }
    }

    /// Morte vinda do agente (vida 0 na memória, conferida na tela). Liga o alerta MORTO.
    pub fn on_death(&mut self, machine: &str, char_name: &str, ts: u64) -> bool {
        match self.machines.get_mut(machine).and_then(|m| m.chars.get_mut(char_name)) {
            Some(c) => {
                c.last_death_at = Some(c.last_death_at.map_or(ts, |t| t.max(ts)));
                true
            }
            None => false,
        }
    }

    /// Tira a máquina do painel (removida pelo admin).
    pub fn forget_machine(&mut self, machine: &str) {
        self.machines.remove(machine);
    }

    pub fn on_disconnect(&mut self, machine: &str) {
        if let Some(m) = self.machines.get_mut(machine) {
            m.connected = false;
        }
    }

    /// `now` = relógio do CENTRAL. `sent_at` = relógio do PC (só usado para ordenar
    /// relatórios que chegam atrasados do buffer, corrigido pela diferença medida).
    pub fn on_report(
        &mut self,
        machine: &str,
        now: u64,
        sent_at: u64,
        chars: Vec<CharReport>,
        events: Vec<ChatEvent>,
        health: Health,
        catalog: Vec<(i32, i32)>,
    ) -> Vec<HubOut> {
        let mut out = Vec::new();
        let m = self.machines.entry(machine.to_string()).or_default();

        let d = now as i64 - sent_at as i64;
        let mut skew = *m.skew.get_or_insert(d);
        // Relógio do PC acertado para trás (DESKTOP-E1GLBEF, 12/11 04:08: 5 h): o atraso salta
        // muito além do que a fila de relatórios guarda (1 h). Sem isto, a hora dos relatórios
        // ficava presa por 5 h e o painel dizia "PAROU" com o char farmando.
        if d > skew + 7200 {
            skew = d;
        }
        let skew = skew.min(d);
        m.skew = Some(skew);
        let ts = ((sent_at as i64 + skew).max(0) as u64).clamp(m.last_ts, now);
        m.last_ts = ts;

        m.last_report_at = Some(now);
        m.health = health;
        m.catalog = catalog;

        for c in chars {
            let name = c.title.name.clone().unwrap_or_else(|| format!("pid {}", c.pid));
            let st = m.chars.entry(name.clone()).or_insert_with(|| CharState {
                tracker: ZenTracker::new(3600),
                last: c.clone(),
                last_seen_at: ts,
                last_death_at: None,
                spark: VecDeque::new(),
            });
            if let (Some(z), true) = (c.zen, c.zen_source != ZenSource::Unknown) {
                st.tracker.push(ts, z);
                let minute = ts / 60;
                if st.spark.back().is_none_or(|(t, _)| t / 60 != minute) {
                    st.spark.push_back((ts, z));
                    if st.spark.len() > SPARK_POINTS {
                        st.spark.pop_front();
                    }
                    out.push(HubOut::ZenPoint {
                        machine: machine.to_string(),
                        char_name: name.clone(),
                        ts,
                        zen: z,
                    });
                }
            }
            st.last = c;
            st.last_seen_at = now;
        }
        m.chars
            .retain(|_, c| now.saturating_sub(c.last_seen_at) < FORGET_CHAR_S);

        for ev in events {
            if let ChatEvent::Pk {
                killer,
                killer_kind,
                victim,
            } = ev
            {
                if !self.pk.accept(&killer, &victim, ts) {
                    continue;
                }
                let row = PkRow {
                    ts,
                    killer,
                    monster: killer_kind == KillerKind::Monster,
                    victim: victim.clone(),
                    machine: machine.to_string(),
                };
                // Linha global: a vítima pode estar em OUTRO PC.
                for mm in self.machines.values_mut() {
                    if let Some(c) = mm.chars.get_mut(&victim) {
                        c.last_death_at = Some(ts);
                    }
                }
                self.recent_pk.push_front(row.clone());
                self.recent_pk.truncate(RECENT_PK);
                out.push(HubOut::Pk(row));
            }
        }
        out
    }

    pub fn snapshot(&self, now: u64) -> Vec<PanelChar> {
        let mut out = Vec::new();
        for (mname, m) in &self.machines {
            for (cname, c) in &m.chars {
                let zen = if c.last.zen_source == ZenSource::Unknown {
                    None
                } else {
                    c.last.zen
                };
                let rate = c.tracker.per_hour(now);
                let snap = CharSnapshot {
                    last_report_at: Some(c.last_seen_at),
                    disconnected: c.last.disconnected,
                    zen,
                    zen_per_hour: rate,
                    idle_secs: c.tracker.idle_secs(now),
                    // "Morto" só enquanto o char não voltou: relatório com vida > 0 chegando 8 s
                    // ou mais depois da morte = renasceu (ReDMooN, 11/11: voltou ao spot e o
                    // painel seguiu "Morto" pelos 10 min da janela).
                    last_death_at: c.last_death_at.filter(|&d| !(c.last.hp.is_some_and(|h| h > 0) && c.last_seen_at >= d + 8)),
                };
                out.push(PanelChar {
                    farm: c.last.farm.clone(),
                    home_spot: c.last.home_spot,
                    primary_retry_at: c.last.primary_retry_at,
                    machine: mname.clone(),
                    name: cname.clone(),
                    level: c.last.title.level,
                    resets: c.last.title.resets,
                    zen,
                    zen_source: c.last.zen_source,
                    zen_per_hour: rate,
                    idle_secs: snap.idle_secs,
                    eta_limit_s: zen.and_then(|z| eta_to_limit(z, rate)),
                    alerts: evaluate(&snap, now, &self.alert_cfg),
                    spark: c.spark.iter().copied().collect(),
                    hp: c.last.hp,
                    last_death_at: c.last_death_at,
                    auto_status: c.last.auto_status.clone(),
                    pos: c.last.pos,
                    map: c.last.map.clone(),
                    spot_map: c.last.spot_map.clone(),
                    spot: c.last.spot,
                    return_on_death: c.last.return_on_death,
                    auto_deposit: c.last.auto_deposit,
                    spot_locked: c.last.spot_locked,
                    skill_key: c.last.skill_key,
                    pet_hp: c.last.pet_hp,
                    nearby: c.last.nearby.clone(),
                    automation: c.last.automation,
                });
            }
        }
        out
    }

    /// Alertas críticos que acabaram de aparecer, prontos para o celular.
    /// Mudanças de alerta neste ciclo: o que começou, o que terminou (para o log) e o que vai
    /// para o celular (só críticos novos, com o filtro de repetição).
    pub fn alert_events(&mut self, now: u64, cooldown_s: u64) -> AlertEvents {
        let snap = self.snapshot(now);
        let mut ev = AlertEvents::default();
        let mut keys = std::collections::HashSet::new();
        for c in snap {
            let key = format!("{}/{}", c.machine, c.name);
            keys.insert(key.clone());
            let prev = self.last_alerts.get(&key).cloned().unwrap_or_default();
            for a in &c.alerts {
                if !prev.contains(a) {
                    ev.started.push(format!("{key}: {}{}", a.label(), detail(*a, &c)));
                }
            }
            for a in &prev {
                if !c.alerts.contains(a) {
                    ev.ended.push(format!("{key}: {}", a.label()));
                }
            }
            self.last_alerts.insert(key.clone(), c.alerts.clone());
            for a in self.notify.update(&key, &c.alerts, now, cooldown_s) {
                ev.phone.push(format!("{key}: {}{}", a.label(), detail(a, &c)));
            }
        }
        // Char esquecido ou máquina removida: os alertas dele somem sem registro.
        self.last_alerts.retain(|k, _| keys.contains(k));
        ev
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct AlertEvents {
    pub started: Vec<String>,
    pub ended: Vec<String>,
    pub phone: Vec<String>,
}

fn detail(a: AlertKind, c: &PanelChar) -> String {
    match a {
        AlertKind::Stopped => c.idle_secs.map(|s| format!(" há {}", zen_core::fmt::dur_br(s))),
        AlertKind::Limit2B | AlertKind::Near2B => c.zen.map(|z| format!(" ({})", zen_core::fmt::int_br(z))),
        _ => None,
    }
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zen_core::title::TitleInfo;

    fn rep(name: &str, zen: u64, disconnected: bool) -> CharReport {
        CharReport {
                farm: None,
                primary_retry_at: None,
                home_spot: None,
            pid: 1,
            title: TitleInfo {
                name: Some(name.into()),
                level: Some(400),
                ..Default::default()
            },
            zen: Some(zen),
            zen_source: ZenSource::Ram,
            lock_status: "travado".into(),
            tcp_connected: !disconnected,
            disconnected,
            hp: None,
            hp_status: String::new(),
            auto_status: String::new(),
            pos: None,
            map: None,
            spot_map: None,
            spot: None,
            return_on_death: None,
            auto_deposit: None,
            spot_locked: None,
            skill_key: None,
            pet_hp: None,
            nearby: None,
            automation: None,
        }
    }

    fn pk(k: &str, kind: KillerKind, v: &str) -> ChatEvent {
        ChatEvent::Pk {
            killer: k.into(),
            killer_kind: kind,
            victim: v.into(),
        }
    }

    #[test]
    fn farm_normal_sem_alerta() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_hello("PC1", "3.0", "stable");
        for i in 0..100u64 {
            h.on_report(
                "PC1",
                1000 + i * 5,
                1000 + i * 5,
                vec![rep("A", 100_000_000 + i * 400_000, false)],
                vec![],
                Health::default(),
                vec![],
            );
        }
        let s = h.snapshot(1495);
        assert_eq!(s.len(), 1);
        assert!(s[0].alerts.is_empty(), "{:?}", s[0].alerts);
        assert!(s[0].zen_per_hour > 200_000_000.0);
    }

    #[test]
    fn parou_e_notifica_uma_vez() {
        let mut h = Hub::new(AlertConfig::default());
        for i in 0..60u64 {
            h.on_report(
                "PC1",
                1000 + i * 5,
                1000 + i * 5,
                vec![rep("A", 5_000, false)],
                vec![],
                Health::default(),
                vec![],
            );
        }
        let s = h.snapshot(1295);
        assert!(s[0].alerts.contains(&AlertKind::Stopped));
        let ev = h.alert_events(1295, 600);
        assert_eq!(ev.phone.len(), 1);
        assert!(ev.phone[0].starts_with("PC1/A: PAROU"));
        assert_eq!(ev.started, ev.phone);
        let ev = h.alert_events(1296, 600);
        assert!(ev.phone.is_empty() && ev.started.is_empty() && ev.ended.is_empty());
    }

    #[test]
    fn morte_em_outro_pc_e_pk_deduplicado() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("A", 1, false)], vec![], Health::default(), vec![]);
        h.on_report("PC2", 1000, 1000, vec![rep("B", 1, false)], vec![], Health::default(), vec![]);
        // A linha global aparece nos dois PCs.
        let o1 = h.on_report(
            "PC1",
            1005,
            1005,
            vec![rep("A", 1, false)],
            vec![pk("Mau", KillerKind::Player, "B")],
            Health::default(),
            vec![],
        );
        let o2 = h.on_report(
            "PC2",
            1006,
            1006,
            vec![rep("B", 1, false)],
            vec![pk("Mau", KillerKind::Player, "B")],
            Health::default(),
            vec![],
        );
        assert_eq!(o1.iter().filter(|o| matches!(o, HubOut::Pk(_))).count(), 1);
        assert_eq!(o2.iter().filter(|o| matches!(o, HubOut::Pk(_))).count(), 0);
        let s = h.snapshot(1006);
        let b = s.iter().find(|c| c.name == "B").unwrap();
        assert!(b.alerts.contains(&AlertKind::Dead));
    }

    #[test]
    fn buffer_atrasado_usa_ordem_do_pc_sem_passar_do_agora() {
        let mut h = Hub::new(AlertConfig::default());
        // Relógio do PC 1 h adiantado. Relatórios guardados chegam juntos em t=5000.
        h.on_report(
            "PC1",
            4000,
            7600,
            vec![rep("A", 1_000, false)],
            vec![],
            Health::default(),
            vec![],
        );
        let outs: Vec<_> = (1..=3u64)
            .flat_map(|i| {
                h.on_report(
                    "PC1",
                    5000,
                    7600 + i * 60,
                    vec![rep("A", 1_000 + i * 100, false)],
                    vec![],
                    Health::default(),
                    vec![],
                )
            })
            .collect();
        let ts: Vec<u64> = outs
            .iter()
            .filter_map(|o| {
                if let HubOut::ZenPoint { ts, .. } = o {
                    Some(*ts)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ts, vec![4060, 4120, 4180]);
    }

    #[test]
    fn registra_inicio_e_fim_do_alerta() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("A", 5, false)], vec![], Health::default(), vec![]);
        assert_eq!(h.alert_events(1000, 600), AlertEvents::default());
        h.on_report("PC1", 1005, 1005, vec![rep("A", 6, true)], vec![], Health::default(), vec![]);
        let ev = h.alert_events(1005, 600);
        assert_eq!(ev.started, vec!["PC1/A: DC".to_string()]);
        assert_eq!(ev.phone, vec!["PC1/A: DC".to_string()]);
        h.on_report("PC1", 1010, 1010, vec![rep("A", 7, false)], vec![], Health::default(), vec![]);
        let ev = h.alert_events(1010, 600);
        assert_eq!(ev.ended, vec!["PC1/A: DC".to_string()]);
        assert!(ev.started.is_empty() && ev.phone.is_empty());
    }

    #[test]
    fn maquina_removida_some_do_painel() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("A", 1, false)], vec![], Health::default(), vec![]);
        h.forget_machine("PC1");
        assert!(h.snapshot(1001).is_empty());
        h.on_disconnect("PC1"); // não recria
        assert!(h.machines.is_empty());
    }

    #[test]
    fn sem_sinal_quando_o_char_some() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("A", 1, false)], vec![], Health::default(), vec![]);
        h.on_report("PC1", 1030, 1030, vec![], vec![], Health::default(), vec![]);
        assert_eq!(h.snapshot(1030)[0].alerts, vec![AlertKind::Offline]);
    }

    #[test]
    fn relogio_do_pc_acertado_para_tras_nao_prende_a_hora() {
        let mut h = Hub::new(AlertConfig::default());
        // PC 5 h adiantado; depois o relógio é acertado.
        h.on_report("PC1", 100_000, 118_000, vec![rep("A", 1, false)], vec![], Health::default(), vec![]);
        h.on_report("PC1", 100_010, 100_010, vec![rep("A", 2, false)], vec![], Health::default(), vec![]);
        h.on_report("PC1", 100_400, 100_400, vec![rep("A", 3, false)], vec![], Health::default(), vec![]);
        assert_eq!(h.central_ts("PC1", 100_400, 100_400), 100_400);
        assert!(!h.snapshot(100_401)[0].alerts.contains(&AlertKind::Stopped));
    }

    #[test]
    fn morto_some_quando_o_char_renasce() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("Vitima", 100, false)], vec![], Health::default(), vec![]);
        assert!(h.on_death("PC1", "Vitima", 1001));
        let mut r = rep("Vitima", 100, false);
        r.hp = Some(0);
        h.on_report("PC1", 1003, 1003, vec![r.clone()], vec![], Health::default(), vec![]);
        assert!(h.snapshot(1003)[0].alerts.contains(&AlertKind::Dead), "ainda morto");
        r.hp = Some(6400);
        h.on_report("PC1", 1012, 1012, vec![r], vec![], Health::default(), vec![]);
        let s = h.snapshot(1012);
        assert!(!s[0].alerts.contains(&AlertKind::Dead), "renasceu: não é mais Morto");
        assert_eq!(s[0].last_death_at, Some(1001), "a hora da morte continua no painel");
    }

    #[test]
    fn morte_do_agente_liga_morto_e_some_depois_da_janela() {
        let mut h = Hub::new(AlertConfig::default());
        h.on_report("PC1", 1000, 1000, vec![rep("Vitima", 100, false)], vec![], Health::default(), vec![]);
        assert!(!h.on_death("PC1", "Outro", 1001), "char de outro PC/inexistente não");
        assert!(h.on_death("PC1", "Vitima", 1001));
        let s = h.snapshot(1002);
        assert!(s[0].alerts.contains(&AlertKind::Dead));
        assert_eq!(s[0].last_death_at, Some(1001));
        h.on_report("PC1", 1001 + 601, 1001 + 601, vec![rep("Vitima", 100, false)], vec![], Health::default(), vec![]);
        assert!(!h.snapshot(1001 + 602)[0].alerts.contains(&AlertKind::Dead));
        assert_eq!(h.central_ts("PC1", 1500, 1400), 1400, "não passa do agora");
        assert_eq!(h.central_ts("PCX", 1500, 1400), 1400);
    }
}
