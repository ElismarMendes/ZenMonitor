//! Trava de endereço do Zen na RAM — a parte "pensante", sem I/O.
//!
//! Fluxo (o mesmo que a 2.5 provou em 8 chars):
//! 1. OCR lê o Zen na tela (inventário aberto) -> valor semente V1.
//! 2. O agente varre a memória UMA vez procurando V1 como u32 alinhado -> candidatos.
//! 3. Quando o OCR mostrar outro valor V2, o agente lê só os candidatos; fica quem tem V2.
//! 4. Sobrou 1 (ou só cópias que andam juntas por 2 mudanças) -> TRAVADO.
//! 5. Travado: lê 4 bytes por ciclo. 3 leituras ruins seguidas, ou valor > limite,
//!    ou divergir do OCR 2 vezes -> volta ao passo 1.
//!
//! A varredura completa só acontece no passo 2, uma vez por char por sessão.

use crate::ZEN_LIMIT;

/// Acha todas as posições alinhadas em 4 bytes onde `value` aparece (little-endian).
/// `base` é o endereço do primeiro byte de `haystack`.
pub fn find_u32(haystack: &[u8], base: u64, value: u32) -> Vec<u64> {
    let needle = value.to_le_bytes();
    let start = ((4 - (base % 4)) % 4) as usize;
    let mut out = Vec::new();
    let mut i = start;
    while i + 4 <= haystack.len() {
        if haystack[i..i + 4] == needle {
            out.push(base + i as u64);
        }
        i += 4;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockState {
    /// Precisa de um valor do OCR para começar.
    NeedSeed,
    /// Precisa varrer a memória procurando `seed`.
    NeedScan { seed: u32 },
    /// Esperando o Zen mudar para filtrar.
    Filtering {
        last_value: u32,
        candidates: Vec<u64>,
        changes_seen: u32,
    },
    Locked {
        addr: u64,
        bad_reads: u32,
        ocr_mismatch: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockAction {
    Nothing,
    /// O agente deve varrer a memória procurando este valor e chamar `on_scan`.
    Scan(u32),
    /// O agente deve ler estes endereços e chamar `on_filter_reads`.
    ReadCandidates(Vec<u64>),
}

#[derive(Debug, Clone)]
pub struct LockMachine {
    pub state: LockState,
    /// Muitos candidatos = valor comum demais (ex.: 0). Pede outra semente.
    pub max_candidates: usize,
}

impl Default for LockMachine {
    fn default() -> Self {
        Self {
            state: LockState::NeedSeed,
            max_candidates: 50_000,
        }
    }
}

impl LockMachine {
    pub fn is_locked(&self) -> Option<u64> {
        match self.state {
            LockState::Locked { addr, .. } => Some(addr),
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        self.state = LockState::NeedSeed;
    }

    /// Chamado a cada leitura de OCR bem-sucedida (pode ser `None` se o inventário está fechado).
    pub fn on_ocr(&mut self, ocr: Option<u32>) -> LockAction {
        let Some(v) = ocr else { return LockAction::Nothing };
        if u64::from(v) > ZEN_LIMIT {
            return LockAction::Nothing; // OCR leu lixo
        }
        match &mut self.state {
            LockState::NeedSeed => {
                // Zen 0 é comum demais na memória: não serve de semente.
                if v == 0 {
                    return LockAction::Nothing;
                }
                self.state = LockState::NeedScan { seed: v };
                LockAction::Scan(v)
            }
            LockState::NeedScan { seed } => LockAction::Scan(*seed),
            LockState::Filtering {
                last_value, candidates, ..
            } => {
                if v != *last_value {
                    LockAction::ReadCandidates(candidates.clone())
                } else {
                    LockAction::Nothing
                }
            }
            LockState::Locked { .. } => LockAction::Nothing,
        }
    }

    pub fn on_scan(&mut self, found: Vec<u64>) {
        let LockState::NeedScan { seed } = self.state else {
            return;
        };
        if found.is_empty() || found.len() > self.max_candidates {
            self.state = LockState::NeedSeed;
            return;
        }
        self.state = LockState::Filtering {
            last_value: seed,
            candidates: found,
            changes_seen: 0,
        };
    }

    /// `ocr` = valor novo na tela; `reads` = o que cada candidato tem agora.
    pub fn on_filter_reads(&mut self, ocr: u32, reads: &[(u64, Option<u32>)]) {
        let LockState::Filtering {
            candidates,
            changes_seen,
            last_value,
        } = &mut self.state
        else {
            return;
        };
        let keep: Vec<u64> = candidates
            .iter()
            .copied()
            .filter(|a| reads.iter().any(|(ra, rv)| ra == a && *rv == Some(ocr)))
            .collect();
        *changes_seen += 1;
        *last_value = ocr;
        match keep.len() {
            0 => self.state = LockState::NeedSeed,
            1 => {
                self.state = LockState::Locked {
                    addr: keep[0],
                    bad_reads: 0,
                    ocr_mismatch: 0,
                }
            }
            // Cópias que acompanharam 2 mudanças são equivalentes: pega a primeira.
            _ if *changes_seen >= 2 => {
                self.state = LockState::Locked {
                    addr: keep[0],
                    bad_reads: 0,
                    ocr_mismatch: 0,
                }
            }
            _ => *candidates = keep,
        }
    }

    /// Leitura do ciclo normal. Devolve o Zen confiável, ou `None`.
    pub fn on_locked_read(&mut self, read: Option<u32>) -> Option<u64> {
        let LockState::Locked { bad_reads, .. } = &mut self.state else {
            return None;
        };
        match read {
            Some(v) if u64::from(v) <= ZEN_LIMIT => {
                *bad_reads = 0;
                Some(u64::from(v))
            }
            _ => {
                *bad_reads += 1;
                if *bad_reads >= 3 {
                    self.state = LockState::NeedSeed;
                }
                None
            }
        }
    }

    /// Conferência periódica com o OCR (quando o inventário estiver aberto).
    pub fn on_crosscheck(&mut self, ocr: u32, ram: u64) {
        let LockState::Locked { ocr_mismatch, .. } = &mut self.state else {
            return;
        };
        if u64::from(ocr) == ram {
            *ocr_mismatch = 0;
        } else {
            *ocr_mismatch += 1;
            if *ocr_mismatch >= 2 {
                self.state = LockState::NeedSeed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busca_alinhada() {
        let mut mem = vec![0u8; 64];
        mem[8..12].copy_from_slice(&123_456_789u32.to_le_bytes());
        mem[13..17].copy_from_slice(&123_456_789u32.to_le_bytes()); // desalinhado: ignora
        mem[40..44].copy_from_slice(&123_456_789u32.to_le_bytes());
        assert_eq!(find_u32(&mem, 0x1000, 123_456_789), vec![0x1008, 0x1028]);
        // base desalinhada: alinhamento é pelo endereço, não pelo índice
        assert_eq!(find_u32(&mem[1..], 0x1001, 123_456_789), vec![0x1008, 0x1028]);
        assert!(find_u32(&mem, 0x1000, 0xDEAD_BEEF).is_empty());
    }

    #[test]
    fn fluxo_completo() {
        let mut m = LockMachine::default();
        assert_eq!(m.on_ocr(None), LockAction::Nothing);
        assert_eq!(m.on_ocr(Some(1000)), LockAction::Scan(1000));
        m.on_scan(vec![0x10, 0x20, 0x30]);
        assert_eq!(m.on_ocr(Some(1000)), LockAction::Nothing);
        assert_eq!(m.on_ocr(Some(1500)), LockAction::ReadCandidates(vec![0x10, 0x20, 0x30]));
        m.on_filter_reads(1500, &[(0x10, Some(1000)), (0x20, Some(1500)), (0x30, None)]);
        assert_eq!(m.is_locked(), Some(0x20));
        assert_eq!(m.on_locked_read(Some(1600)), Some(1600));
    }

    #[test]
    fn copias_espelho_travam_depois_de_2_mudancas() {
        let mut m = LockMachine::default();
        m.on_ocr(Some(1000));
        m.on_scan(vec![0x10, 0x20]);
        m.on_filter_reads(1500, &[(0x10, Some(1500)), (0x20, Some(1500))]);
        assert_eq!(m.is_locked(), None);
        m.on_filter_reads(1700, &[(0x10, Some(1700)), (0x20, Some(1700))]);
        assert_eq!(m.is_locked(), Some(0x10));
    }

    #[test]
    fn nenhum_candidato_recomeca() {
        let mut m = LockMachine::default();
        m.on_ocr(Some(1000));
        m.on_scan(vec![0x10]);
        m.on_filter_reads(1500, &[(0x10, Some(999))]);
        assert_eq!(m.state, LockState::NeedSeed);
    }

    #[test]
    fn semente_zero_ou_lixo_recusada() {
        let mut m = LockMachine::default();
        assert_eq!(m.on_ocr(Some(0)), LockAction::Nothing);
        assert_eq!(m.on_ocr(Some(u32::MAX)), LockAction::Nothing);
        m.on_ocr(Some(5));
        m.on_scan(vec![]);
        assert_eq!(m.state, LockState::NeedSeed);
    }

    #[test]
    fn leituras_ruins_destravam() {
        let mut m = LockMachine {
            state: LockState::Locked {
                addr: 1,
                bad_reads: 0,
                ocr_mismatch: 0,
            },
            ..Default::default()
        };
        assert_eq!(m.on_locked_read(None), None);
        assert_eq!(m.on_locked_read(Some(u32::MAX)), None);
        assert!(m.is_locked().is_some());
        assert_eq!(m.on_locked_read(None), None);
        assert_eq!(m.state, LockState::NeedSeed);
    }

    #[test]
    fn ocr_divergente_destrava() {
        let mut m = LockMachine {
            state: LockState::Locked {
                addr: 1,
                bad_reads: 0,
                ocr_mismatch: 0,
            },
            ..Default::default()
        };
        m.on_crosscheck(10, 11);
        m.on_crosscheck(10, 10);
        m.on_crosscheck(10, 11);
        assert!(m.is_locked().is_some());
        m.on_crosscheck(10, 11);
        assert_eq!(m.state, LockState::NeedSeed);
    }
}

/// Extrai o Zen do texto que o OCR devolveu. Procura a linha com "Zen" e pega o maior
/// número da mesma linha (o MU mostra "Zen 423,485,898" ou "423.485.898 Zen").
/// Devolve `None` se nada plausível aparecer. Fica aqui para ser testável sem Windows.
pub fn zen_from_ocr_lines(lines: &[String]) -> Option<u32> {
    let mut best: Option<u64> = None;
    for line in lines {
        if !line.to_ascii_lowercase().contains("zen") {
            continue;
        }
        let mut cur = String::new();
        let flush = |cur: &mut String, best: &mut Option<u64>| {
            let digits: String = cur.chars().filter(|c| c.is_ascii_digit()).collect();
            if (1..=10).contains(&digits.len()) {
                if let Ok(n) = digits.parse::<u64>() {
                    if n <= ZEN_LIMIT && best.is_none_or(|b| n > b) {
                        *best = Some(n);
                    }
                }
            }
            cur.clear();
        };
        for ch in line.chars() {
            // OCR confunde O/o com 0 dentro de números; só aceitamos dígitos e separadores.
            if ch.is_ascii_digit() || ((ch == '.' || ch == ',') && !cur.is_empty()) {
                cur.push(ch);
            } else {
                flush(&mut cur, &mut best);
            }
        }
        flush(&mut cur, &mut best);
    }
    best.map(|n| n as u32)
}

/// Zen lido de um RECORTE que só contém o valor (o campo do inventário, ex.: "797,124,492").
/// Mais rígido que `zen_from_ocr_lines`: qualquer letra no recorte invalida a leitura,
/// porque um dígito trocado aqui vira semente errada e varredura inútil.
/// Aceita espaço, vírgula e ponto como separador de milhar (o OCR alterna entre eles).
pub fn zen_from_region_lines(lines: &[String]) -> Option<u32> {
    let text: String = lines.join(" ");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut digits = String::new();
    for ch in text.chars() {
        match ch {
            '0'..='9' => digits.push(ch),
            ',' | '.' | ' ' => {}
            // O OCR às vezes lê 0 como O dentro de número.
            'O' | 'o' => digits.push('0'),
            _ => return None,
        }
    }
    if digits.is_empty() || digits.len() > 10 {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    (n <= ZEN_LIMIT).then_some(n as u32)
}

#[cfg(test)]
mod ocr_tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn formatos() {
        assert_eq!(zen_from_ocr_lines(&l(&["Zen 423,485,898"])), Some(423_485_898));
        assert_eq!(zen_from_ocr_lines(&l(&["423.485.898 Zen"])), Some(423_485_898));
        assert_eq!(zen_from_ocr_lines(&l(&["Level 400", "ZEN: 1,000"])), Some(1000));
    }

    #[test]
    fn recorte_do_inventario() {
        assert_eq!(zen_from_region_lines(&l(&["797,124,492"])), Some(797_124_492));
        assert_eq!(zen_from_region_lines(&l(&["797.124.492"])), Some(797_124_492));
        assert_eq!(zen_from_region_lines(&l(&["797 124 492"])), Some(797_124_492));
        assert_eq!(zen_from_region_lines(&l(&["797,124,49O"])), Some(797_124_490));
        assert_eq!(zen_from_region_lines(&l(&["0"])), Some(0));
    }

    #[test]
    fn recorte_com_lixo_e_recusado() {
        assert_eq!(zen_from_region_lines(&l(&[])), None);
        assert_eq!(zen_from_region_lines(&l(&["Zen 162812"])), None);
        assert_eq!(zen_from_region_lines(&l(&["797,124,49?"])), None);
        assert_eq!(zen_from_region_lines(&l(&["9,999,999,999"])), None);
        assert_eq!(zen_from_region_lines(&l(&["12345678901"])), None);
    }

    #[test]
    fn sem_zen_ou_lixo() {
        assert_eq!(zen_from_ocr_lines(&l(&["Level 400"])), None);
        assert_eq!(zen_from_ocr_lines(&l(&["Zen 9,999,999,999"])), None);
        assert_eq!(zen_from_ocr_lines(&l(&["Zen"])), None);
    }
}
