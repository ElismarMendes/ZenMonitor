//! Trava do HP na memória e detecção de morte. Pura: o agente faz as leituras e passa aqui.
//!
//! O que a medição real (zen-diag vida-mem, 08/11/2026) mostrou:
//! - O HP mora num **u16**. A semente (valor da tela) acha ~18 mil u16 iguais.
//! - Ao filtrar pelas mudanças, sobra 1 endereço que acompanhou 42 mudanças seguidas.
//! - Mas a tela e a memória às vezes estão 1 quadro defasadas: o endereço certo "erra"
//!   de vez em quando. Um filtro que descarta por falhas SOMADAS perde o endereço certo
//!   (foi o que aconteceu no ReDMooN). Aqui o descarte é por falhas SEGUIDAS ou por
//!   taxa de acerto baixa, e cada conferência lê a memória antes E depois da tela.

/// Mudanças de HP acompanhadas antes de travar.
pub const LOCK_HITS: u32 = 8;
/// Falhas seguidas que descartam um candidato.
const MAX_CONSEC_MISS: u32 = 3;
/// Leituras falhas seguidas no endereço travado antes de largar a trava.
const MAX_LOCKED_FAILS: u32 = 3;
/// Semente comum demais (ex.: HP 100) gera candidatos demais: espera outro valor.
pub const MAX_CANDIDATES: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HpCand {
    pub addr: u64,
    pub hits: u32,
    pub misses: u32,
    pub consec_miss: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HpState {
    NeedSeed,
    Hunting { cands: Vec<HpCand>, changes: u32 },
    Locked { addr: u64, fails: u32, cross_miss: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HpAction {
    Nothing,
    /// Varrer a memória atrás deste u16.
    Scan(u16),
    /// Reler os candidatos (antes e depois da tela) e chamar `on_candidate_reads` com este HP.
    ReadCandidates(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HpLock {
    pub state: HpState,
    prev_screen: Option<u32>,
    last_used: Option<u32>,
}

impl Default for HpLock {
    fn default() -> Self {
        Self { state: HpState::NeedSeed, prev_screen: None, last_used: None }
    }
}

impl HpLock {
    pub fn is_locked(&self) -> Option<u64> {
        match self.state {
            HpState::Locked { addr, .. } => Some(addr),
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Trava lembrada de antes (mesmo processo). As leituras e a conferência com a tela
    /// derrubam a trava se o endereço não valer mais.
    pub fn restore(&mut self, addr: u64) {
        self.state = HpState::Locked { addr, fails: 0, cross_miss: 0 };
    }

    /// Valor lido na tela agora. Só age quando o valor ficou igual em 2 leituras seguidas
    /// (a tela não está no meio de uma mudança) e é diferente do último usado.
    pub fn on_screen(&mut self, screen: Option<u32>) -> HpAction {
        let stable = screen.is_some() && screen == self.prev_screen;
        self.prev_screen = screen;
        let Some(hp) = screen.filter(|_| stable) else { return HpAction::Nothing };
        if self.last_used == Some(hp) || hp > u16::MAX as u32 {
            return HpAction::Nothing;
        }
        match &self.state {
            HpState::NeedSeed => {
                // HP 0 (morto) não serve de semente: zero tem em todo lugar.
                if hp == 0 {
                    return HpAction::Nothing;
                }
                self.last_used = Some(hp);
                HpAction::Scan(hp as u16)
            }
            HpState::Hunting { .. } => {
                self.last_used = Some(hp);
                HpAction::ReadCandidates(hp)
            }
            HpState::Locked { .. } => HpAction::Nothing,
        }
    }

    pub fn on_scan(&mut self, found: Vec<u64>) {
        if found.is_empty() || found.len() > MAX_CANDIDATES {
            self.state = HpState::NeedSeed;
            return;
        }
        let cands = found
            .into_iter()
            .map(|addr| HpCand { addr, hits: 1, misses: 0, consec_miss: 0 })
            .collect();
        self.state = HpState::Hunting { cands, changes: 0 };
    }

    pub fn candidates(&self) -> Vec<u64> {
        match &self.state {
            HpState::Hunting { cands, .. } => cands.iter().map(|c| c.addr).collect(),
            _ => vec![],
        }
    }

    /// `reads`: (endereço, leitura antes da tela, leitura depois da tela).
    /// Acerto se QUALQUER uma das duas bate com o HP da tela.
    pub fn on_candidate_reads(&mut self, hp: u32, reads: &[(u64, Option<u16>, Option<u16>)]) {
        let HpState::Hunting { cands, changes } = &mut self.state else { return };
        *changes += 1;
        let map: std::collections::HashMap<u64, (Option<u16>, Option<u16>)> =
            reads.iter().map(|(a, b, c)| (*a, (*b, *c))).collect();
        for c in cands.iter_mut() {
            let (b, a) = map.get(&c.addr).copied().unwrap_or((None, None));
            let hit = b.map(u32::from) == Some(hp) || a.map(u32::from) == Some(hp);
            if hit {
                c.hits += 1;
                c.consec_miss = 0;
            } else {
                c.misses += 1;
                c.consec_miss += 1;
            }
        }
        cands.retain(|c| {
            let total = c.hits + c.misses;
            c.consec_miss < MAX_CONSEC_MISS && !(total >= 6 && c.misses * 2 > total)
        });
        if cands.is_empty() {
            self.state = HpState::NeedSeed;
            self.last_used = None;
            return;
        }
        // Trava no melhor: muitos acertos e taxa >= 80%. Se houver cópias do HP (a tela e a
        // lógica guardam o mesmo valor), qualquer uma serve: fica a de mais acertos.
        let best = cands
            .iter()
            .filter(|c| c.hits >= LOCK_HITS && c.misses * 5 <= c.hits + c.misses)
            .max_by_key(|c| (c.hits, std::cmp::Reverse(c.misses), std::cmp::Reverse(c.addr)));
        if let Some(b) = best {
            self.state = HpState::Locked { addr: b.addr, fails: 0, cross_miss: 0 };
        }
    }

    /// Leitura do endereço travado. Devolve o HP; larga a trava depois de 3 falhas seguidas
    /// (o processo trocou de mapa/relogou e a memória mudou).
    pub fn on_locked_read(&mut self, read: Option<u16>) -> Option<u32> {
        let HpState::Locked { fails, .. } = &mut self.state else { return None };
        match read {
            Some(v) => {
                *fails = 0;
                Some(v as u32)
            }
            None => {
                *fails += 1;
                if *fails >= MAX_LOCKED_FAILS {
                    self.reset();
                }
                None
            }
        }
    }

    /// Conferência com a tela (de tempos em tempos). 3 discordâncias seguidas -> refaz a trava.
    pub fn on_crosscheck(&mut self, screen: u32, before: Option<u16>, after: Option<u16>) {
        let HpState::Locked { cross_miss, .. } = &mut self.state else { return };
        let ok = before.map(u32::from) == Some(screen) || after.map(u32::from) == Some(screen);
        if ok {
            *cross_miss = 0;
        } else {
            *cross_miss += 1;
            if *cross_miss >= 3 {
                self.reset();
            }
        }
    }

    pub fn status(&self) -> String {
        match &self.state {
            HpState::NeedSeed => "vida: esperando o número da tela".into(),
            HpState::Hunting { cands, changes } => format!("vida: filtrando {} ({} mudanças)", cands.len(), changes),
            HpState::Locked { addr, .. } => format!("vida travada {addr:#x}"),
        }
    }
}

/// Detector de morte: HP > 0 e depois 0. Uma morte só; a próxima só depois de o HP voltar.
#[derive(Debug, Clone, Default)]
pub struct DeathDetector {
    last_alive_hp: Option<u32>,
    dead: bool,
}

impl DeathDetector {
    /// Devolve `Some(hp_antes)` no instante da morte.
    pub fn on_hp(&mut self, hp: u32) -> Option<u32> {
        if hp == 0 {
            if !self.dead {
                if let Some(before) = self.last_alive_hp {
                    self.dead = true;
                    return Some(before);
                }
            }
            return None;
        }
        self.dead = false;
        self.last_alive_hp = Some(hp);
        None
    }

    /// Morte descartada (a tela mostrou vida): volta a vigiar sem esperar o HP subir.
    pub fn cancel(&mut self) {
        self.dead = false;
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// A trava mudou de endereço: esquece o histórico para não comparar memórias diferentes.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Memória falsa: o HP verdadeiro em 0x10, o HP máximo (fixo) em 0x20, lixo em 0x30.
    fn reads(true_hp: u16, lag: bool) -> Vec<(u64, Option<u16>, Option<u16>)> {
        vec![
            (0x10, Some(if lag { true_hp.wrapping_add(7) } else { true_hp }), Some(true_hp)),
            (0x20, Some(6686), Some(6686)),
            (0x30, Some(123), Some(123)),
        ]
    }

    fn feed(l: &mut HpLock, hp: u32, lag: bool) {
        l.on_screen(Some(hp));
        if let HpAction::ReadCandidates(v) = l.on_screen(Some(hp)) {
            l.on_candidate_reads(v, &reads(hp as u16, lag));
        }
    }

    #[test]
    fn trava_no_endereco_que_acompanha_mesmo_com_defasagem() {
        let mut l = HpLock::default();
        assert_eq!(l.on_screen(Some(6686)), HpAction::Nothing); // 1ª leitura: ainda não estável
        assert_eq!(l.on_screen(Some(6686)), HpAction::Scan(6686));
        l.on_scan(vec![0x10, 0x20]);
        // Mudanças de HP, com defasagem tela/memória em algumas (a leitura "depois" acerta).
        let seq = [6600, 6686, 6650, 6686, 6640, 6686, 6630, 6620, 6686, 6610];
        for (i, hp) in seq.iter().enumerate() {
            feed(&mut l, *hp, i % 3 == 0);
        }
        assert_eq!(l.is_locked(), Some(0x10), "{:?}", l.state);
    }

    #[test]
    fn falhas_somadas_nao_derrubam_o_certo_mas_seguidas_sim() {
        let mut l = HpLock::default();
        l.on_screen(Some(500));
        l.on_screen(Some(500));
        l.on_scan(vec![0x10]);
        // O certo erra 1 vez a cada 4 (defasagem total, as duas leituras erradas).
        for i in 0..7u32 {
            let hp = 400 + i;
            l.on_screen(Some(hp));
            if let HpAction::ReadCandidates(v) = l.on_screen(Some(hp)) {
                let r = if i % 4 == 3 { Some(1) } else { Some(v as u16) };
                l.on_candidate_reads(v, &[(0x10, r, r)]);
            }
        }
        assert!(matches!(l.state, HpState::Hunting { .. } | HpState::Locked { .. }), "{:?}", l.state);
        // Agora 3 seguidas: descarta e volta a pedir semente.
        let mut l2 = HpLock::default();
        l2.on_screen(Some(500));
        l2.on_screen(Some(500));
        l2.on_scan(vec![0x10]);
        for hp in [401u32, 402, 403] {
            l2.on_screen(Some(hp));
            if let HpAction::ReadCandidates(v) = l2.on_screen(Some(hp)) {
                l2.on_candidate_reads(v, &[(0x10, Some(9), Some(9))]);
            }
        }
        assert_eq!(l2.state, HpState::NeedSeed);
    }

    #[test]
    fn hp_zero_nao_e_semente_e_tela_instavel_nao_age() {
        let mut l = HpLock::default();
        l.on_screen(Some(0));
        assert_eq!(l.on_screen(Some(0)), HpAction::Nothing);
        l.on_screen(Some(10));
        assert_eq!(l.on_screen(Some(11)), HpAction::Nothing);
        assert_eq!(l.on_screen(None), HpAction::Nothing);
    }

    #[test]
    fn semente_comum_demais_espera_outra() {
        let mut l = HpLock::default();
        l.on_screen(Some(100));
        l.on_screen(Some(100));
        l.on_scan((0..(MAX_CANDIDATES as u64 + 1)).collect());
        assert_eq!(l.state, HpState::NeedSeed);
    }

    #[test]
    fn travado_larga_depois_de_falhas_e_de_discordar_da_tela() {
        let mut l = HpLock { state: HpState::Locked { addr: 0x10, fails: 0, cross_miss: 0 }, ..Default::default() };
        assert_eq!(l.on_locked_read(Some(5)), Some(5));
        l.on_locked_read(None);
        l.on_locked_read(None);
        assert!(l.is_locked().is_some());
        l.on_locked_read(None);
        assert_eq!(l.is_locked(), None);

        let mut l = HpLock { state: HpState::Locked { addr: 0x10, fails: 0, cross_miss: 0 }, ..Default::default() };
        l.on_crosscheck(500, Some(1), Some(500)); // a leitura "depois" bate: ok
        l.on_crosscheck(500, Some(1), Some(2));
        l.on_crosscheck(500, Some(1), Some(2));
        assert!(l.is_locked().is_some());
        l.on_crosscheck(500, Some(1), Some(2));
        assert_eq!(l.is_locked(), None);
    }

    #[test]
    fn morte_uma_vez_so() {
        let mut d = DeathDetector::default();
        assert_eq!(d.on_hp(0), None); // começou em 0: não sabemos se morreu agora
        assert_eq!(d.on_hp(6000), None);
        assert_eq!(d.on_hp(120), None);
        assert_eq!(d.on_hp(0), Some(120));
        assert_eq!(d.on_hp(0), None); // continua morto: não repete
        assert!(d.is_dead());
        assert_eq!(d.on_hp(6686), None); // renasceu
        assert_eq!(d.on_hp(0), Some(6686));
    }
}
