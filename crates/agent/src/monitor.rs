//! Um ciclo de leitura. Não sabe nada de Windows: usa `GameAccess`.
//! Por isso dá para testar o ciclo inteiro com um jogo falso em memória.

use crate::GameAccess;
use std::collections::HashMap;
use zen_core::protocol::{CharReport, ZenSource};
use zen_core::actions::{AutoInvConfig, AutoInventory, InvDecision};
use zen_core::hplock::{DeathDetector, HpAction, HpLock};
use zen_core::ramlock::{LockAction, LockMachine, LockState};
use zen_core::title;

/// A cada quantos ciclos travados conferimos a RAM com o OCR (12 x 5 s = 1 min).
const CROSSCHECK_EVERY: u64 = 12;
/// Intervalo da leitura rápida da vida (morte dura poucos segundos na tela).
pub const HP_TICK_MS: u64 = 500;
/// Conferência da vida travada com a tela: a cada 60 leituras rápidas (30 s).
const HP_CROSSCHECK_TICKS: u64 = 60;
/// Ciclos seguidos sem conexão TCP do jogo até marcar DC.
pub const DC_CYCLES: u32 = 6;

/// Trava lembrada entre reinícios do agente (arquivo `travas.json`). Só volta para o MESMO
/// processo: mesmo pid, mesma hora de início e mesmo char. Qualquer diferença = trava nova.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedLock {
    pub pid: u32,
    pub started: u64,
    pub name: String,
    pub zen_addr: Option<u64>,
    pub hp_addr: Option<u64>,
}

/// Vida chegou a 0 na memória. O `main` tira o print e confere na tela antes de avisar.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathEvent {
    pub pid: u32,
    pub hwnd: isize,
    pub char_name: String,
    pub hp_before: u32,
}

/// Pedido de tecla V da abertura automática do inventário.
#[derive(Debug, Clone, PartialEq)]
pub struct InvRequest {
    pub pid: u32,
    pub hwnd: isize,
    pub char_name: String,
    pub open: bool,
}

#[derive(Default)]
struct CharState {
    auto_inv: AutoInventory,
    started: Option<u64>,
    hwnd: isize,
    name: Option<String>,
    hp: HpLock,
    death: DeathDetector,
    last_hp: Option<u32>,
    hp_ticks: u64,
    last_hp_status: String,
    lock: LockMachine,
    no_tcp_cycles: u32,
    cycles: u64,
    /// Último ciclo em que o Zen ou a vida mudaram (jogo vivo).
    alive_cycle: u64,
    prev_zen: Option<u64>,
    prev_hp: Option<u64>,
    /// Último estado registrado no log (só registra quando muda).
    last_status: String,
    last_disconnected: bool,
}

#[derive(Default)]
pub struct Monitor {
    chars: HashMap<u32, CharState>,
    pub ram_paused: bool,
    /// Janelas lidas no ciclo anterior (pid, nome), para registrar no log só quando muda.
    last_windows: Vec<(u32, Option<String>)>,
    logged_once: bool,
    pending_restore: Vec<SavedLock>,
}

impl Monitor {
    pub fn new(ram_paused: bool) -> Self {
        Self {
            chars: HashMap::new(),
            last_windows: Vec::new(),
            logged_once: false,
            pending_restore: Vec::new(),
            ram_paused,
        }
    }

    /// Travas salvas pelo agente anterior (aplicadas quando a janela aparecer no ciclo).
    pub fn restore_from(&mut self, saved: Vec<SavedLock>) {
        self.pending_restore = saved;
    }

    /// Travas atuais, para gravar em disco.
    pub fn saved_locks(&self) -> Vec<SavedLock> {
        let mut v: Vec<SavedLock> = self
            .chars
            .iter()
            .filter_map(|(&pid, st)| {
                let zen_addr = st.lock.is_locked();
                let hp_addr = st.hp.is_locked();
                if zen_addr.is_none() && hp_addr.is_none() {
                    return None;
                }
                Some(SavedLock { pid, started: st.started?, name: st.name.clone()?, zen_addr, hp_addr })
            })
            .collect();
        v.sort_by_key(|s| s.pid);
        v
    }

    /// Pede nova trava para um char (comando "Refazer trava de RAM").
    pub fn relock(&mut self, access: &dyn GameAccess, char_name: &str) -> bool {
        for w in access.list_windows() {
            if title::parse(&w.title).name.as_deref() == Some(char_name) {
                if let Some(s) = self.chars.get_mut(&w.pid) {
                    s.lock.reset();
                    return true;
                }
            }
        }
        false
    }

    pub fn hwnd_of(&self, access: &dyn GameAccess, char_name: &str) -> Option<isize> {
        access
            .list_windows()
            .into_iter()
            .find(|w| title::parse(&w.title).name.as_deref() == Some(char_name))
            .map(|w| w.hwnd)
    }

    pub fn cycle(&mut self, access: &dyn GameAccess) -> Vec<CharReport> {
        let windows = access.list_windows();
        let seen: Vec<(u32, Option<String>)> = windows.iter().map(|w| (w.pid, title::parse(&w.title).name)).collect();
        if seen != self.last_windows || !self.logged_once {
            if seen.is_empty() {
                tracing::info!(
                    "nenhuma janela do jogo para ler (o jogo ou o fake-client está aberto? \
                     se allowed_chars estiver preenchido, só esses nomes contam)"
                );
            } else {
                tracing::info!(janelas = ?seen, "janelas do jogo lidas neste ciclo");
            }
            self.last_windows = seen;
            self.logged_once = true;
        }
        self.chars.retain(|pid, _| windows.iter().any(|w| w.pid == *pid));
        let mut out = Vec::with_capacity(windows.len());

        for w in windows {
            let info = title::parse(&w.title);
            let is_new = !self.chars.contains_key(&w.pid);
            let st = self.chars.entry(w.pid).or_default();
            if is_new {
                st.started = access.process_start(w.pid);
                let hit = self.pending_restore.iter().position(|s| {
                    s.pid == w.pid && Some(s.started) == st.started && Some(&s.name) == info.name.as_ref()
                });
                if let Some(i) = hit {
                    let s = self.pending_restore.remove(i);
                    if let Some(a) = s.zen_addr {
                        st.lock.state = LockState::Locked { addr: a, bad_reads: 0, ocr_mismatch: 0 };
                    }
                    if let Some(a) = s.hp_addr {
                        st.hp.restore(a);
                    }
                    tracing::info!(pid = w.pid, char = %s.name, zen = ?s.zen_addr.map(|a| format!("{a:#x}")),
                        vida = ?s.hp_addr.map(|a| format!("{a:#x}")), "travas restauradas (mesmo processo do jogo)");
                }
            }
            st.cycles += 1;
            st.hwnd = w.hwnd;
            st.name = info.name.clone();

            let tcp = access.has_external_tcp(w.pid);
            st.no_tcp_cycles = if tcp { 0 } else { st.no_tcp_cycles.saturating_add(1) };

            let ocr_ready = access.zen_field_ready();
            let (zen, source) = if self.ram_paused {
                (None, ZenSource::Unknown)
            } else if !ocr_ready && st.lock.is_locked().is_none() {
                // Sem OCR configurado não existe semente confiável: não varre a memória.
                (None, ZenSource::Unknown)
            } else {
                read_zen(access, w.pid, w.hwnd, st)
            };

            // O jogo continua vivo se o Zen ou a vida mudaram há pouco (YouSee, ELISMAR-NOTE 11/11:
            // farmando com Zen subindo e o agente dava DC de até 3 min porque a tabela TCP do
            // Windows não mostrava a conexão). DC só com TCP sumido E números parados.
            let changed = |a: Option<u64>, b: Option<u64>| matches!((a, b), (Some(x), Some(y)) if x != y);
            if changed(st.prev_zen, zen) || changed(st.prev_hp, st.last_hp.map(u64::from)) {
                st.alive_cycle = st.cycles;
            }
            if zen.is_some() {
                st.prev_zen = zen;
            }
            if st.last_hp.is_some() {
                st.prev_hp = st.last_hp.map(u64::from);
            }
            let frozen = st.cycles.saturating_sub(st.alive_cycle) >= u64::from(DC_CYCLES);
            let disconnected = st.no_tcp_cycles >= DC_CYCLES && frozen;
            if disconnected != st.last_disconnected {
                if disconnected {
                    tracing::info!(pid = w.pid, char = ?info.name, "conexão do jogo caiu (DC)");
                } else {
                    tracing::info!(pid = w.pid, char = ?info.name, "conexão do jogo voltou");
                }
                st.last_disconnected = disconnected;
            }
            if st.no_tcp_cycles == DC_CYCLES && !frozen {
                tracing::info!(pid = w.pid, char = ?info.name, "sem conexão TCP visível, mas Zen/vida mudando: não é DC");
            }

            let status = if !ocr_ready && !self.ram_paused && st.lock.is_locked().is_none() {
                "zen_ocr_region vazia no agent.toml".to_string()
            } else {
                lock_status(&st.lock, self.ram_paused)
            };
            if status != st.last_status {
                tracing::info!(pid = w.pid, char = ?info.name, estado = %status, zen = ?zen, "trava");
                st.last_status = status.clone();
            }
            out.push(CharReport {
                primary_retry_at: None,
                home_spot: None,
                pid: w.pid,
                title: info,
                zen,
                zen_source: source,
                lock_status: status,
                tcp_connected: tcp,
                disconnected,
                hp: if self.ram_paused { None } else { st.last_hp },
                hp_status: if self.ram_paused {
                    "RAM pausada".into()
                } else if !access.hp_field_ready() {
                    "vida desligada (hp_region vazia)".into()
                } else {
                    st.hp.status()
                },
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
            });
        }
        out
    }

    /// Leitura rápida da vida (a cada `HP_TICK_MS`). Travada: lê 2 bytes por char.
    /// Sem trava: usa o número da tela como semente e filtra pelas mudanças.
    pub fn fast_tick(&mut self, access: &dyn GameAccess) -> Vec<DeathEvent> {
        let mut out = Vec::new();
        if self.ram_paused || !access.hp_field_ready() {
            return out;
        }
        for (&pid, st) in self.chars.iter_mut() {
            st.hp_ticks += 1;
            if let Some(addr) = st.hp.is_locked() {
                let r = access.read_u16_many(pid, &[addr])[0];
                if let Some(v) = st.hp.on_locked_read(r) {
                    st.last_hp = Some(v);
                    // Vida 0 na memória também confere (a cada 5 s): trava errada que ficou em 0 (Pickle,
                    // 11/11: lida de "8" de "908" e restaurada do travas.json) se desfaz sozinha.
                    if (v > 0 && st.hp_ticks % HP_CROSSCHECK_TICKS == 0) || (v == 0 && st.hp_ticks % 10 == 0) {
                        if let Some(screen) = access.read_hp_field(st.hwnd) {
                            let after = access.read_u16_many(pid, &[addr])[0];
                            st.hp.on_crosscheck(screen, r, after);
                        }
                    }
                    if st.hp.is_locked().is_some() {
                        if let Some(before) = st.death.on_hp(v) {
                            tracing::warn!(pid, char = ?st.name, hp_antes = before, "VIDA ZEROU na memória");
                            out.push(DeathEvent {
                                pid,
                                hwnd: st.hwnd,
                                char_name: st.name.clone().unwrap_or_else(|| format!("pid {pid}")),
                                hp_before: before,
                            });
                        }
                    }
                }
                if st.hp.is_locked().is_none() {
                    st.death.forget();
                    st.last_hp = None;
                }
            } else {
                let cands = st.hp.candidates();
                let before = if cands.is_empty() { vec![] } else { access.read_u16_many(pid, &cands) };
                let screen = access.read_hp_field(st.hwnd);
                match st.hp.on_screen(screen) {
                    HpAction::Scan(v) => {
                        let found = access.scan_u16(pid, v);
                        tracing::info!(pid, char = ?st.name, valor = v, candidatos = found.len(), "varredura da vida");
                        st.hp.on_scan(found);
                    }
                    HpAction::ReadCandidates(hp) => {
                        let after = access.read_u16_many(pid, &cands);
                        let reads: Vec<(u64, Option<u16>, Option<u16>)> = cands
                            .iter()
                            .enumerate()
                            .map(|(i, a)| (*a, before.get(i).copied().flatten(), after.get(i).copied().flatten()))
                            .collect();
                        st.hp.on_candidate_reads(hp, &reads);
                        if let Some(a) = st.hp.is_locked() {
                            tracing::info!(pid, char = ?st.name, endereco = format!("{a:#x}"), hp, "vida travada");
                            st.death.forget();
                            st.last_hp = Some(hp);
                            st.death.on_hp(hp);
                        }
                    }
                    HpAction::Nothing => {}
                }
            }
            let status = st.hp.status();
            if status != st.last_hp_status {
                tracing::info!(pid, char = ?st.name, estado = %status, "trava da vida");
                st.last_hp_status = status;
            }
        }
        out
    }

    /// Abertura automática do inventário: decide por char. Só olha a tela (inventário visível?)
    /// quando o Zen está sem trava ou quando fomos nós que abrimos.
    pub fn auto_inventory(&mut self, access: &dyn GameAccess, now: u64, idle_s: u64, cfg: &AutoInvConfig) -> Vec<InvRequest> {
        let mut out = Vec::new();
        if self.ram_paused || !access.zen_field_ready() {
            return out;
        }
        for (&pid, st) in self.chars.iter_mut() {
            let locked = st.lock.is_locked().is_some();
            let visible = if !locked || st.auto_inv.opened_by_us() {
                access.read_zen_field(st.hwnd).is_some()
            } else {
                false
            };
            let d = st.auto_inv.decide(now, locked, visible, idle_s, cfg);
            if d != InvDecision::Nothing {
                out.push(InvRequest {
                    pid,
                    hwnd: st.hwnd,
                    char_name: st.name.clone().unwrap_or_else(|| format!("pid {pid}")),
                    open: d == InvDecision::Open,
                });
            }
        }
        out
    }

    /// A tecla não chegou: espera antes de tentar de novo.
    pub fn auto_inventory_failed(&mut self, pid: u32, now: u64) {
        if let Some(st) = self.chars.get_mut(&pid) {
            st.auto_inv.failed(now);
        }
    }

    /// A tela mostrou vida depois do "zero" na memória: o endereço não vale mais. Refaz.
    pub fn false_death(&mut self, pid: u32) {
        if let Some(st) = self.chars.get_mut(&pid) {
            st.death.cancel();
            st.death.forget();
            st.hp.reset();
            st.last_hp = None;
        }
    }
}

fn read_zen(access: &dyn GameAccess, pid: u32, hwnd: isize, st: &mut CharState) -> (Option<u64>, ZenSource) {
    if let Some(addr) = st.lock.is_locked() {
        let v = st.lock.on_locked_read(access.read_u32(pid, addr));
        if let Some(ram) = v {
            if st.cycles % CROSSCHECK_EVERY == 0 {
                if let Some(ocr) = access.read_zen_field(hwnd) {
                    st.lock.on_crosscheck(ocr, ram);
                }
            }
            return (Some(ram), ZenSource::Ram);
        }
        return (None, ZenSource::Unknown);
    }

    let ocr = access.read_zen_field(hwnd);
    match st.lock.on_ocr(ocr) {
        LockAction::Scan(v) => {
            let found = access.scan_u32(pid, v);
            tracing::info!(pid, valor = v, candidatos = found.len(), "varredura de trava");
            st.lock.on_scan(found);
        }
        LockAction::ReadCandidates(c) => {
            let v = ocr.unwrap_or_default();
            let reads: Vec<(u64, Option<u32>)> = c.iter().map(|a| (*a, access.read_u32(pid, *a))).collect();
            st.lock.on_filter_reads(v, &reads);
            tracing::info!(pid, valor_novo = v, estado = ?lock_status(&st.lock, false), "filtro de candidatos");
            if let Some(a) = st.lock.is_locked() {
                tracing::info!(pid, endereco = format!("{a:#x}"), "Zen travado");
            }
        }
        LockAction::Nothing => {}
    }
    match ocr {
        Some(v) => (Some(u64::from(v)), ZenSource::Ocr),
        None => (None, ZenSource::Unknown),
    }
}

fn lock_status(l: &LockMachine, paused: bool) -> String {
    if paused {
        return "RAM pausada".into();
    }
    match &l.state {
        LockState::NeedSeed => "abra o inventário (V) para travar".into(),
        LockState::NeedScan { .. } => "varrendo".into(),
        LockState::Filtering { candidates, .. } => format!("filtrando {} candidatos", candidates.len()),
        LockState::Locked { addr, .. } => format!("travado {addr:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GameWindow;
    use std::sync::Mutex;

    /// Jogo falso em memória: Zen em 3 endereços (1 real + 2 lixo que não acompanham).
    struct Fake {
        hp: Mutex<u16>,
        zen: Mutex<u32>,
        inventory_open: Mutex<bool>,
        tcp: Mutex<bool>,
        ocr_ok: Mutex<bool>,
    }

    fn fake(zen: u32, inv: bool, tcp: bool) -> Fake {
        Fake {
            hp: Mutex::new(6686),
            zen: Mutex::new(zen),
            inventory_open: Mutex::new(inv),
            tcp: Mutex::new(tcp),
            ocr_ok: Mutex::new(true),
        }
    }

    impl GameAccess for Fake {
        fn list_windows(&self) -> Vec<GameWindow> {
            vec![GameWindow {
                pid: 7,
                hwnd: 1,
                title: "TheClassic MU || Name: Teste || Level: 400 || Resets: 1".into(),
            }]
        }
        fn has_external_tcp(&self, _: u32) -> bool {
            *self.tcp.lock().unwrap()
        }
        fn read_u32(&self, _: u32, addr: u64) -> Option<u32> {
            match addr {
                0x100 => Some(*self.zen.lock().unwrap()),
                0x200 => Some(1000),
                _ => None,
            }
        }
        fn scan_u32(&self, _: u32, v: u32) -> Vec<u64> {
            let mut r = vec![];
            if v == *self.zen.lock().unwrap() {
                r.push(0x100);
            }
            if v == 1000 {
                r.push(0x200);
            }
            r
        }
        fn zen_field_ready(&self) -> bool {
            *self.ocr_ok.lock().unwrap()
        }
        fn read_zen_field(&self, _: isize) -> Option<u32> {
            // Passa pelo leitor de verdade: desenha na fonte do jogo e lê de volta.
            if !*self.inventory_open.lock().unwrap() {
                return None;
            }
            let mut b = zen_core::glyphs::Bits::new(78, 14);
            for (x, y) in zen_core::glyphs::render(u64::from(*self.zen.lock().unwrap())) {
                b.set(13 + x, 5 + y);
            }
            zen_core::glyphs::read_number(&b).map(|n| n as u32)
        }
        fn screenshot_png(&self, _: isize) -> Option<Vec<u8>> {
            None
        }
        fn hp_field_ready(&self) -> bool {
            true
        }
        fn read_hp_field(&self, _: isize) -> Option<u32> {
            // Passa pelo leitor de verdade da vida.
            let mut b = zen_core::glyphs::Bits::new(100, 36);
            for (x, y) in zen_core::hp::render(u32::from(*self.hp.lock().unwrap())) {
                b.set(20 + x, zen_core::hp::BAND_Y + y);
            }
            zen_core::hp::read_hp(&b)
        }
        fn read_u16_many(&self, _: u32, addrs: &[u64]) -> Vec<Option<u16>> {
            // 0x500 = vida de verdade; 0x502 = vida máxima (fixa); 0x504 = lixo.
            addrs
                .iter()
                .map(|a| match a {
                    0x500 => Some(*self.hp.lock().unwrap()),
                    0x502 => Some(6686),
                    0x504 => Some(77),
                    _ => None,
                })
                .collect()
        }
        fn scan_u16(&self, _: u32, v: u16) -> Vec<u64> {
            [0x500u64, 0x502, 0x504]
                .into_iter()
                .zip(self.read_u16_many(0, &[0x500, 0x502, 0x504]))
                .filter(|(_, r)| *r == Some(v))
                .map(|(a, _)| a)
                .collect()
        }
        fn death_shot(&self, _: isize) -> Option<(Vec<u8>, Option<u32>)> {
            None
        }
        fn process_start(&self, _: u32) -> Option<u64> {
            Some(42)
        }
    }

    #[test]
    fn trava_a_vida_e_detecta_a_morte_uma_vez() {
        let g = fake(1000, true, true);
        let mut m = Monitor::new(false);
        m.cycle(&g); // registra a janela
        // Vida cheia (igual à máxima) e depois oscilando: a máxima sai, a real fica.
        let seq: Vec<u16> = (0..12).flat_map(|i| [6686u16, 6600 - i * 10]).collect();
        for hp in seq {
            *g.hp.lock().unwrap() = hp;
            assert!(m.fast_tick(&g).is_empty());
            assert!(m.fast_tick(&g).is_empty()); // 2 leituras iguais = tela estável
        }
        assert_eq!(m.chars[&7].hp.is_locked(), Some(0x500), "{:?}", m.chars[&7].hp.state);
        *g.hp.lock().unwrap() = 4321;
        m.fast_tick(&g);
        assert_eq!(m.cycle(&g)[0].hp, Some(4321));
        *g.hp.lock().unwrap() = 0;
        let d = m.fast_tick(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].hp_before, 4321);
        assert_eq!(d[0].char_name, "Teste");
        assert!(m.fast_tick(&g).is_empty(), "morte repetida");
        *g.hp.lock().unwrap() = 6686; // renasceu
        m.fast_tick(&g);
        *g.hp.lock().unwrap() = 0;
        assert_eq!(m.fast_tick(&g).len(), 1, "segunda morte");
    }

    #[test]
    fn travas_voltam_so_para_o_mesmo_processo() {
        let g = fake(1000, true, true);
        let mut a = Monitor::new(false);
        a.cycle(&g);
        *g.zen.lock().unwrap() = 1500;
        a.cycle(&g);
        assert!(a.chars[&7].lock.is_locked().is_some());
        let saved = a.saved_locks();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].started, 42);

        // Agente reiniciado, mesmo processo: Zen sai da RAM já no 1º ciclo, sem inventário.
        *g.inventory_open.lock().unwrap() = false;
        let mut b = Monitor::new(false);
        b.restore_from(saved.clone());
        let r = b.cycle(&g);
        assert_eq!(r[0].zen_source, ZenSource::Ram);
        assert_eq!(r[0].zen, Some(1500));

        // Outro processo (hora de início diferente): não restaura.
        let mut c = Monitor::new(false);
        let mut other = saved.clone();
        other[0].started = 43;
        c.restore_from(other);
        assert_eq!(c.cycle(&g)[0].zen_source, ZenSource::Unknown);
        // Outro char no mesmo pid: não restaura.
        let mut d = Monitor::new(false);
        let mut other = saved;
        other[0].name = "Outro".into();
        d.restore_from(other);
        assert_eq!(d.cycle(&g)[0].zen_source, ZenSource::Unknown);
    }

    #[test]
    fn abre_o_inventario_sozinho_e_fecha_depois_de_travar() {
        let g = fake(1000, false, true); // inventário fechado: sem semente
        let mut m = Monitor::new(false);
        let c = AutoInvConfig { wait_s: 90, idle_min_s: 60, max_open_s: 900, cooldown_s: 600 };
        m.cycle(&g);
        assert!(m.auto_inventory(&g, 0, 999, &c).is_empty());
        let r = m.auto_inventory(&g, 90, 999, &c);
        assert_eq!(r.len(), 1);
        assert!(r[0].open);
        *g.inventory_open.lock().unwrap() = true; // a tecla V "chegou"
        m.cycle(&g); // semente + varredura
        *g.zen.lock().unwrap() = 1500;
        m.cycle(&g); // filtra e trava
        assert!(m.chars[&7].lock.is_locked().is_some());
        let r = m.auto_inventory(&g, 200, 999, &c);
        assert_eq!(r.len(), 1);
        assert!(!r[0].open, "travou: fecha o que abrimos");
    }

    #[test]
    fn morte_falsa_refaz_a_trava() {
        let g = fake(1000, true, true);
        let mut m = Monitor::new(false);
        m.cycle(&g);
        m.false_death(7);
        assert_eq!(m.chars[&7].hp.is_locked(), None);
    }

    #[test]
    fn trava_e_segue_o_zen() {
        let g = fake(1000, true, true);
        let mut m = Monitor::new(false);
        let r = m.cycle(&g); // semente + varredura: acha 0x100 e 0x200
        assert_eq!(r[0].zen_source, ZenSource::Ocr);
        *g.zen.lock().unwrap() = 1500;
        m.cycle(&g); // filtra: só 0x100 acompanhou
        *g.inventory_open.lock().unwrap() = false; // fechou o inventário
        *g.zen.lock().unwrap() = 1800;
        let r = m.cycle(&g);
        assert_eq!(r[0].zen, Some(1800));
        assert_eq!(r[0].zen_source, ZenSource::Ram);
        assert!(r[0].lock_status.starts_with("travado"));
    }

    #[test]
    fn dc_so_depois_de_6_ciclos() {
        let g = fake(5, false, false);
        let mut m = Monitor::new(false);
        for _ in 1..DC_CYCLES {
            assert!(!m.cycle(&g)[0].disconnected);
        }
        assert!(m.cycle(&g)[0].disconnected);
        *g.tcp.lock().unwrap() = true;
        assert!(!m.cycle(&g)[0].disconnected);
    }

    #[test]
    fn sem_tcp_mas_zen_subindo_nao_e_dc() {
        let g = fake(1000, true, true);
        let mut m = Monitor::new(false);
        m.cycle(&g);
        *g.zen.lock().unwrap() = 1500;
        m.cycle(&g);
        assert!(m.chars[&7].lock.is_locked().is_some());
        *g.tcp.lock().unwrap() = false;
        for i in 0..12u32 {
            *g.zen.lock().unwrap() = 1600 + i * 10;
            let r = m.cycle(&g);
            assert!(!r[0].disconnected, "zen mudando no ciclo {i}");
        }
        // Zen parado e sem TCP: DC depois de DC_CYCLES.
        let mut dc = false;
        for _ in 0..=DC_CYCLES {
            dc = m.cycle(&g)[0].disconnected;
        }
        assert!(dc);
    }

    #[test]
    fn sem_regiao_de_ocr_nao_varre() {
        let g = fake(1000, true, true);
        *g.ocr_ok.lock().unwrap() = false;
        let mut m = Monitor::new(false);
        let r = m.cycle(&g);
        assert_eq!(r[0].zen, None);
        assert!(r[0].lock_status.contains("zen_ocr_region"));
        assert_eq!(m.chars[&7].lock.state, LockState::NeedSeed); // nunca pediu varredura
    }

    #[test]
    fn pausado_nao_le_ram() {
        let g = fake(5, true, true);
        let mut m = Monitor::new(true);
        let r = m.cycle(&g);
        assert_eq!(r[0].zen, None);
        assert_eq!(r[0].lock_status, "RAM pausada");
    }
}
