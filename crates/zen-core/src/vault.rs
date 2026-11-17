//! Depósito de Zen no baú do Kanturu (Safety Guardian, 15 abas). Regras puras; a parte de
//! tela/clique fica em agent::win::nav. Ver docs/decisoes/0014.
//!
//! Medido em 08/11/2026 (gravação de um depósito feito à mão):
//! - move para Kanturu custou 20.000.000 (não 21 mil);
//! - cada aba guarda até 2.000.000.000 (confirmado pelo dono da conta);
//! - o depósito não cobrou taxa (char 1.980.000.000 -> 80.000.000 depositando 1.900.000.000).

/// Máximo de Zen numa aba do baú e no char.
pub const ZEN_CAP: u64 = 2_000_000_000;
/// Fica no char depois do depósito: paga 2 moves de emergência (20 mi cada).
pub const RESERVE: u64 = 40_000_000;
/// Deposita quando o char chega a isto (o Zen para de subir em 2 bi).
pub const DEPOSIT_AT: u64 = 1_990_000_000;

/// Título "Storage N/15" no topo do baú (janela inteira 1030x797) e limiar do branco.
pub const STORAGE_TITLE: [i32; 4] = [535, 40, 70, 17];
pub const TITLE_WHITE: u8 = 150;

/// Modelos do título por aba (recortes reais). Abas sem modelo ainda: não lidas (None).
const TAB_MODELS: &str = include_str!("vault_tabs.txt");

fn models() -> &'static Vec<(u8, crate::glyphs::Bits)> {
    static M: std::sync::OnceLock<Vec<(u8, crate::glyphs::Bits)>> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        let mut out = Vec::new();
        let mut cur: Option<(u8, String)> = None;
        for line in TAB_MODELS.lines().chain(std::iter::once("== 0")) {
            if line.starts_with('#') {
                continue;
            }
            if let Some(n) = line.strip_prefix("== ") {
                if let Some((k, t)) = cur.take() {
                    out.push((k, crate::glyphs::Bits::from_text(&t)));
                }
                cur = n.trim().parse().ok().map(|k| (k, String::new()));
            } else if let Some((_, t)) = cur.as_mut() {
                t.push_str(line);
                t.push('\n');
            }
        }
        out
    })
}

/// Número da aba (1..15) a partir do título binarizado. Exige um modelo claramente mais
/// parecido que os outros; na dúvida, `None`.
pub fn read_tab(b: &crate::glyphs::Bits) -> Option<u8> {
    let mut d: Vec<(usize, u8)> = models()
        .iter()
        .filter(|(_, m)| m.w == b.w && m.h == b.h)
        .map(|(n, m)| (m.px.iter().zip(&b.px).filter(|(a, c)| a != c).count(), *n))
        .collect();
    d.sort();
    match d.as_slice() {
        // "6" e "8" diferem em só 3 pixels: exige (quase) igual e o segundo pior.
        [(best, n), rest @ ..] if *best <= 1 && rest.first().map_or(true, |(s, _)| *s >= best + 3) => Some(*n),
        _ => None,
    }
}

/// Dois títulos iguais (a seta não mudou de aba)? Tolera 2 pixels.
pub fn same_title(a: &crate::glyphs::Bits, b: &crate::glyphs::Bits) -> bool {
    a.w == b.w && a.h == b.h && a.px.iter().zip(&b.px).filter(|(x, y)| x != y).count() <= 2
}

/// Quanto depositar com `zen` no char.
pub fn amount(zen: u64, reserve: u64) -> u64 {
    zen.saturating_sub(reserve)
}

/// A aba cabe o valor inteiro?
pub fn fits(stored: u64, amount: u64) -> bool {
    amount > 0 && stored.saturating_add(amount) <= ZEN_CAP
}

/// Depois do OK: o baú subiu exatamente o valor e o char caiu o valor (tolerância de taxa
/// de até 10.000, que não vimos cobrar).
pub fn deposit_ok(stored_before: u64, stored_after: u64, zen_before: u64, zen_after: u64, amount: u64) -> bool {
    stored_after == stored_before + amount
        && zen_after <= zen_before.saturating_sub(amount)
        && zen_after + 10_000 >= zen_before.saturating_sub(amount)
}

/// Teclas (VK) para digitar o valor: '0'..'9' = 0x30..0x39. Só dígitos, nada mais.
pub fn digit_keys(n: u64) -> Vec<u16> {
    n.to_string().bytes().map(|b| 0x30 + (b - b'0') as u16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valor_e_aba() {
        assert_eq!(amount(1_980_000_000, RESERVE), 1_940_000_000);
        assert_eq!(amount(30_000_000, RESERVE), 0);
        assert!(!fits(150_618_098, 1_940_000_000));
        assert!(fits(0, 1_940_000_000));
        assert!(fits(60_000_000, 1_940_000_000));
        assert!(!fits(60_000_001, 1_940_000_000));
        assert!(!fits(0, 0));
    }

    #[test]
    fn conferencia_do_deposito() {
        // o depósito real de 08/11
        assert!(deposit_ok(0, 1_900_000_000, 1_980_000_000, 80_000_000, 1_900_000_000));
        assert!(!deposit_ok(0, 190_000_000, 1_980_000_000, 1_790_000_000, 1_900_000_000));
        assert!(!deposit_ok(0, 1_900_000_000, 1_980_000_000, 1_980_000_000, 1_900_000_000));
    }

    #[test]
    fn titulo_da_aba() {
        let m = models();
        assert!(m.len() >= 2);
        for (n, b) in m {
            assert_eq!(read_tab(b), Some(*n));
            assert_eq!((b.w, b.h), (STORAGE_TITLE[2] as usize, STORAGE_TITLE[3] as usize));
        }
        let vazio = crate::glyphs::Bits::new(70, 17);
        assert_eq!(read_tab(&vazio), None);
    }

    #[test]
    fn teclas() {
        assert_eq!(digit_keys(1_900), vec![0x31, 0x39, 0x30, 0x30]);
    }
}
