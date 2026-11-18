use super::*;

/// Recuperação até o spot: uma por vez, numa thread (os relatórios continuam saindo).
pub(super) struct Recovery {
    pub(super) machines: std::collections::HashMap<String, zen_core::recovery::RecoveryMachine>,
    pub(super) busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) status: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    pub(super) done_tx: std::sync::mpsc::Sender<(String, Result<String, String>)>,
    pub(super) done_rx: std::sync::mpsc::Receiver<(String, Result<String, String>)>,
    pub(super) terrain: Option<std::sync::Arc<zen_core::terrain::Terrain>>,
    /// Terreno dos outros mapas de spot (Arena, Lost Tower, Dungeon), por World.
    pub(super) other_terrain: std::collections::HashMap<u8, std::sync::Arc<zen_core::terrain::Terrain>>,
    /// Dark Raven: próxima ida ao Trainer só depois deste horário (evita laço NPC <-> spot).
    pub(super) raven_block: std::collections::HashMap<String, u64>,
    /// Dark Raven: hora da primeira leitura baixa (a ida só depois da segunda).
    pub(super) raven_low: std::collections::HashMap<String, u64>,
    /// Depósito falhou: não tenta de novo antes deste horário.
    pub(super) deposit_block: std::collections::HashMap<String, u64>,
    /// v5 (dono, 18/11 03:47): todos os spots lotados = espera na safe até esta hora (~1 h, sorteada)
    /// antes de outra volta pelo catálogo. Antes ele rodava a lista a cada ~6 min, horas a fio.
    pub(super) full_until: std::collections::HashMap<String, u64>,
    /// "Ir até" que chegou com o PC ocupado por outra automação: (char, world, x, y, quando).
    /// Sai assim que o PC liberar (até 3 min). Antes falhava calado (golden das 13:00, 18/11).
    pub(super) goto_queue: Vec<(String, u8, i32, i32, u64)>,
    pub(super) deposit_wait_reason: std::collections::HashMap<String, String>,
    /// Chars cuja causa da morte ainda está sendo lida.
    pub(super) pending: std::collections::HashSet<String>,
    /// Desde quando cada char está farmando perto do spot com o helper ligado.
    pub(super) farm_since: std::collections::HashMap<String, u64>,
    /// Tempo de farm confirmado antes da morte (None = sem amostra contínua suficiente).
    pub(super) death_farmed: std::collections::HashMap<String, Option<u64>>,
    /// Horários das mortes para monstros (3 em 30 min = para de voltar sozinho).
    pub(super) monster_deaths: std::collections::HashMap<String, Vec<u64>>,
    /// Chars que não voltam sozinhos (PK ou causa desconhecida) -> motivo.
    pub(super) hold: std::collections::HashMap<String, String>,
    pub(super) return_owned: std::collections::HashSet<String>,
    /// Automação do PC religada pelo painel: estes chars são conferidos (mapa, spot,
    /// helper) e voltam ao spot se preciso, um por vez.
    pub(super) relaunch: Vec<String>,
    /// Linhas "[PK] ... killed <char>" já vistas (mortes anteriores).
    pub(super) known: std::collections::HashMap<String, std::collections::HashSet<(u64, String)>>,
    pub(super) cause_tx: std::sync::mpsc::Sender<DeathCause>,
    pub(super) cause_rx: std::sync::mpsc::Receiver<DeathCause>,
}

/// v5: libera a automação do PC mesmo se a rotina der pânico no meio. Antes, um pânico matava
/// a thread antes do `busy.store(false)` e a automação daquele PC ficava parada para todos os
/// chars até o agente reiniciar. No caminho normal a rotina libera e responde como sempre; a
/// guarda só age durante o desenrolar de um pânico (a reserva de entrada já se solta sozinha).
pub(super) struct GuardaDaRotina {
    pub(super) busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) tx: std::sync::mpsc::Sender<(String, Result<String, String>)>,
    pub(super) name: String,
}
impl Drop for GuardaDaRotina {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::error!(char = %self.name, "rotina interrompida por erro interno (pânico); automação do PC liberada");
            self.busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = self.tx.send((self.name.clone(), Err("rotina interrompida por erro interno; automação liberada".into())));
        }
    }
}

pub(super) type DeathCause = (String, Vec<(u64, String)>, Option<(String, zen_core::chat::KillerKind)>);

impl Recovery {
    pub(super) fn new() -> Self {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (cause_tx, cause_rx) = std::sync::mpsc::channel();
        Self {
            pending: Default::default(),
            deposit_block: Default::default(),
            full_until: Default::default(),
            goto_queue: Default::default(),
            deposit_wait_reason: Default::default(),
            farm_since: Default::default(),
            death_farmed: Default::default(),
            monster_deaths: Default::default(),
            hold: Default::default(),
            return_owned: Default::default(),
            relaunch: Vec::new(),
            raven_block: Default::default(),
            raven_low: Default::default(),
            known: Default::default(),
            cause_tx,
            cause_rx,
            machines: Default::default(),
            busy: Default::default(),
            status: Default::default(),
            done_tx,
            done_rx,
            terrain: None,
            other_terrain: Default::default(),
        }
    }

    /// Lê no chat (memória do cliente da vítima) quem matou. Numa thread: ~10 s por varredura,
    /// até 2 varreduras (a linha pode chegar um pouco depois do HP zerar).
    pub(super) fn scan_death_cause(&mut self, name: &str, pid: u32, block: bool) {
        if block {
            self.pending.insert(name.to_string());
        }
        let known = self.known.get(name).cloned().unwrap_or_default();
        let (tx, name) = (self.cause_tx.clone(), name.to_string());
        std::thread::spawn(move || {
            let cfg = zen_core::chat::ChatConfig::default();
            let suffix = format!(" killed {}", name.to_ascii_lowercase());
            let mut last = Vec::new();
            for wait in [1500u64, 3000] {
                std::thread::sleep(Duration::from_millis(wait));
                let t = Instant::now();
                let lines: Vec<(u64, String)> = zen_agent::win::chat::pk_lines(pid)
                    .into_iter()
                    .filter(|(_, l)| l.to_ascii_lowercase().trim_end().ends_with(&suffix))
                    .collect();
                let cause = zen_core::chat::death_cause(&lines, &name, &known, &cfg);
                tracing::info!(char = %name, linhas = lines.len(), causa = ?cause, tempo = ?t.elapsed(), "chat lido depois da morte");
                if cause.is_some() {
                    let _ = tx.send((name, lines, cause));
                    return;
                }
                last = lines;
            }
            let _ = tx.send((name, last, None));
        });
    }

    pub(super) fn deposit_wait(&mut self, name: &str, reason: String) {
        if self.deposit_wait_reason.get(name) != Some(&reason) {
            tracing::warn!(char = name, motivo = %reason, "Fortuna automática: aguardando");
            self.deposit_wait_reason.insert(name.to_string(), reason);
        }
    }

    pub(super) fn is_busy(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(super) fn set_status(&self, name: &str, s: String) {
        if let Ok(mut m) = self.status.lock() {
            m.insert(name.to_string(), s);
        }
    }

    pub(super) fn status_of(&self, name: &str) -> Option<String> {
        self.status.lock().ok().and_then(|m| m.get(name).cloned())
    }

    /// Dispara a recuperação de um char numa thread. Falha na hora se já houver uma rodando.
    /// Baú: M -> Kanturu, deposita tudo menos 40 mi (conferido na tela) e volta ao spot.
    pub(super) fn start_deposit(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, all: bool) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma automação em andamento".into());
        }
        if !dest.can_teleport() {
            return Err(format!("spot na {}: a Fortuna só sai de mapas com teleporte calibrado (Kanturu, Arena)", dest.name()));
        }
        let _terrain = self.spot_terrain(pid, hwnd, dest)?;
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        if let Ok(mut m) = status.lock() {
            m.insert(name.clone(), "Fortuna: indo para Lorencia".into());
        }
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            let fmt = zen_core::fmt::int_br;
            // 09/11: o Zen virou Fortuna do Imperador em Lorencia (Lugard), não vai mais para o baú.
            let r = match zen_agent::win::nav::world_terrain_of(hwnd, zen_agent::win::nav::LORENCIA_WORLD)
                .and_then(|lor| zen_agent::win::nav::fortune(hwnd, &lor, all, &cb))
            {
                Ok(rep) => {
                    let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                    tracing::warn!(char = %name, fortunas = rep.converted, para_o_j = rep.to_bank, zen_antes = rep.zen_before, zen_depois = rep.zen_after, volta = ?back, "AUTOMAÇÃO: Fortuna do Imperador");
                    let msg = format!("depósito: {} Fortuna(s) do Imperador, {} guardada(s) no J, ficou {} no char", rep.converted, rep.to_bank, fmt(rep.zen_after));
                    match back {
                        Ok(_) => Ok(format!("{msg}, de volta ao spot")),
                        Err(e) => Ok(format!("{msg}; a volta ao spot falhou: {e}")),
                    }
                }
                Err(e) => {
                    tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: Fortuna falhou");
                    let _ = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                    Err(format!("depósito (Fortuna) falhou: {e}"))
                }
            };
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }

    /// Dark Raven: Trainer de Lorencia (restaura com Zen) e volta ao spot.
    pub(super) fn start_raven(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, manual: bool) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma automação em andamento".into());
        }
        let _terrain = self.spot_terrain(pid, hwnd, dest)?;
        let lor = zen_agent::win::nav::world_terrain_of(hwnd, zen_agent::win::nav::LORENCIA_WORLD)?;
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        tracing::warn!(char = %name, spot = ?spot, "AUTOMAÇÃO: Dark Raven com pouca vida: indo ao Trainer");
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            let r = match zen_agent::win::nav::restore_raven(hwnd, &lor, manual, &cb) {
                Ok(rep) => {
                    let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                    tracing::warn!(char = %name, antes = ?rep.before, depois = rep.after, volta = ?back, "AUTOMAÇÃO: Dark Raven restaurado");
                    let msg = format!("Raven restaurado: {}% -> {}%", rep.before.map_or("?".into(), |v| v.to_string()), rep.after);
                    match back {
                        Ok(_) => Ok(format!("{msg}, de volta ao spot")),
                        Err(e) => Err(format!("{msg}, mas a volta ao spot falhou: {e}")),
                    }
                }
                Err(e) => {
                    tracing::warn!(char = %name, erro = %e, "AUTOMAÇÃO: Dark Raven não restaurado");
                    let back = zen_agent::win::spots::recover(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, &cb);
                    Err(format!("Raven NÃO restaurado: {e}{}", if back.is_ok() { " (voltou ao spot)" } else { "" }))
                }
            };
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }

    /// Elpis: Gemstone -> Harmony, todas as do inventário, o mais rápido possível (dono, 16/11
    /// 20:38). Ocupa a automação do PC até acabar.
    pub(super) fn start_elpis(&mut self, name: &str, hwnd: isize) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma automação em andamento".into());
        }
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            // Ação pedida no painel: roda mesmo com a automação do PC desligada (explicit_move);
            // desligar a automação no meio cancela no próximo passo.
            let r = match zen_agent::win::input::explicit_move(|| zen_agent::win::nav::elpis_refine(hwnd, &name, u32::MAX, &cb)) {
                Ok(rep) => {
                    tracing::warn!(char = %name, refinos = rep.refined, harmonies = rep.ok, falhas = rep.failed, "ELPIS: acabaram as Gemstones");
                    Ok(format!("Elpis: acabaram as Gemstones; {} refino(s), {} Harmony(s), {} falha(s)", rep.refined, rep.ok, rep.failed))
                }
                Err(e) => {
                    tracing::warn!(char = %name, erro = %e, "ELPIS: parou");
                    Err(format!("Elpis parou: {e}"))
                }
            };
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }

    /// Treino/limpeza: K -> J no lugar onde o char está (sem ir à cidade), com prints.
    pub(super) fn start_bank(&mut self, name: &str, hwnd: isize) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma automação em andamento".into());
        }
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            let r = zen_agent::win::nav::fortune_bank(hwnd, &cb);
            tracing::warn!(char = %name, resultado = ?r, "AUTOMAÇÃO: K -> J (treino)");
            let r = match r {
                Ok(m) => Ok(format!("depósito (J): {m}")),
                Err(e) => Err(format!("depósito (J) falhou: {e}")),
            };
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }

    /// v4: "ir até" do painel: (x, y) num mapa validado, sem mexer no spot.
    pub(super) fn start_goto(&mut self, name: &str, hwnd: isize, world: u8, dest: zen_agent::win::nav::Dest, target: (i32, i32)) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma automação em andamento".into());
        }
        let terrain = zen_agent::win::nav::world_terrain_of(hwnd, world)?;
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        tracing::warn!(char = %name, world, destino = ?target, "AUTOMAÇÃO: ir até (painel)");
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            let r = input::explicit_move(|| zen_agent::win::nav::recover_in(hwnd, &terrain, target, dest, &cb));
            tracing::warn!(char = %name, resultado = ?r, "AUTOMAÇÃO: ir até concluído");
            let r = match r {
                Ok(m) => Ok(format!("ir até ({}, {}): chegou — {m}. Fica lá até \"Voltar ao spot\"", target.0, target.1)),
                Err(e) => Err(format!("ir até ({}, {}) falhou: {e}", target.0, target.1)),
            };
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }

    /// Terreno do mapa do spot (Kanturu ou Arena), carregado uma vez.
    pub(super) fn spot_terrain(&mut self, pid: u32, hwnd: isize, dest: zen_agent::win::nav::Dest) -> Result<std::sync::Arc<zen_core::terrain::Terrain>, String> {
        if dest != zen_agent::win::nav::Dest::Kanturu {
            let w = dest.world();
            if !self.other_terrain.contains_key(&w) {
                let t = zen_agent::win::nav::world_terrain_of(hwnd, w)?;
                self.other_terrain.insert(w, std::sync::Arc::new(t));
            }
            return Ok(self.other_terrain[&w].clone());
        }
        if self.terrain.is_none() {
            self.terrain = Some(std::sync::Arc::new(zen_agent::win::nav::kanturu_terrain(pid)?));
        }
        Ok(self.terrain.clone().unwrap())
    }

    pub(super) fn start(&mut self, name: &str, pid: u32, hwnd: isize, spot: (i32, i32), dest: zen_agent::win::nav::Dest, why: &str) -> Result<(), String> {
        if self.is_busy() {
            return Err("já há uma recuperação em andamento".into());
        }
        let _terrain = self.spot_terrain(pid, hwnd, dest)?;
        self.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let (busy, status, tx, name) = (self.busy.clone(), self.status.clone(), self.done_tx.clone(), name.to_string());
        tracing::warn!(char = %name, motivo = why, spot = ?spot, "AUTOMAÇÃO: recuperação iniciada");
        if let Ok(mut m) = status.lock() {
            m.insert(name.clone(), format!("recuperando ({why})"));
        }
        let retain_owned=self.return_owned.contains(&name);
        std::thread::spawn(move || {
            let _guarda = GuardaDaRotina { busy: busy.clone(), tx: tx.clone(), name: name.clone() };
            let st = status.clone();
            let n2 = name.clone();
            let cb = move |s: String| {
                if let Ok(mut m) = st.lock() {
                    m.insert(n2.clone(), s);
                }
            };
            let report = |s: String| cb(if retain_owned { format!("retorno após morte/PK: {s}") } else { s });
            let r = zen_agent::win::spots::resume(hwnd, &name, zen_core::spots::Spot{world:dest.world(),x:spot.0,y:spot.1}, retain_owned, &report);
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.send((name, r));
        });
        Ok(())
    }
}

pub(super) fn raven_cooldown(result: &Result<String,String>) -> Option<u64> {
    match result {
        Ok(m) | Err(m) if m.starts_with("Raven restaurado:") => Some(1800),
        Err(m) if m.starts_with("Raven N?O restaurado:") => Some(60),
        _ => None,
    }
}

pub(super) fn rec_cfg_of(cfg: &AgentConfig) -> zen_core::recovery::RecoveryConfig {
    zen_core::recovery::RecoveryConfig {
        max_per_hour: cfg.return_max.clamp(1, 20) as usize,
        window_s: cfg.return_window_min.clamp(1, 240) * 60,
        far_tiles: cfg.spot_drift_tiles.clamp(1, 10),
        ..Default::default()
    }
}

/// Sorteio simples (sem crate de random): RandomState traz uma semente aleatória do SO.
pub(super) fn random_between(lo: u64, hi: u64) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(now());
    lo + h.finish() % (hi - lo + 1)
}

pub(super) fn fmt_wait(s: u64) -> String {
    if s < 60 {
        format!("{s} s")
    } else {
        format!("{} min {:02} s", s / 60, s % 60)
    }
}
