//! Parser do título da janela do cliente.
//!
//! Formato confirmado:
//! `TheClassic MU || Name: ReDMooN || Level: 400 || Resets: 911 || WC: 0 || WP: 21,519 || GP: 15,840.0`
//! Cliente em português: `TheClassic MU || Nome: xWizard || Level: 400 || Resets: 900 || ...`
//! Milhar com vírgula, decimal com ponto. A ordem dos campos não é assumida; cada um
//! é lido pela chave. Campo ausente ou ilegível vira `None` (o painel mostra "—").

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleInfo {
    pub name: Option<String>,
    pub level: Option<u32>,
    pub resets: Option<u32>,
    pub wc: Option<u64>,
    pub wp: Option<u64>,
    /// GP em décimos (15,840.0 -> 158400) para não usar ponto flutuante em dinheiro.
    pub gp_tenths: Option<u64>,
}

impl TitleInfo {
    /// Um título só conta como "de personagem logado" se tiver nome e level.
    pub fn is_logged_in(&self) -> bool {
        self.name.is_some() && self.level.is_some()
    }
}

pub fn parse(title: &str) -> TitleInfo {
    let mut info = TitleInfo::default();
    // Corta títulos absurdos antes de qualquer trabalho (entrada hostil).
    let title: String = title.chars().take(512).collect();
    for part in title.split("||") {
        let Some((k, v)) = part.split_once(':') else { continue };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim();
        match key.as_str() {
            // Cliente em português mostra "Nome:" (visto em 08/11 no DESKTOP-RAG9LEL).
            "name" | "nome" => {
                let n = crate::sanitize(val, 16);
                if !n.is_empty() && !n.contains(' ') {
                    info.name = Some(n);
                }
            }
            "level" | "nível" | "nivel" => info.level = parse_int(val).and_then(|n| u32::try_from(n).ok()),
            "resets" => info.resets = parse_int(val).and_then(|n| u32::try_from(n).ok()),
            "wc" => info.wc = parse_int(val),
            "wp" => info.wp = parse_int(val),
            "gp" => info.gp_tenths = parse_tenths(val),
            _ => {}
        }
    }
    info
}

/// "21,519" -> 21519. Rejeita qualquer coisa que não seja dígito ou vírgula de milhar.
pub fn parse_int(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s.len() > 24 {
        return None;
    }
    let mut n: u64 = 0;
    let mut digits = 0;
    for c in s.chars() {
        match c {
            '0'..='9' => {
                n = n.checked_mul(10)?.checked_add(c as u64 - '0' as u64)?;
                digits += 1;
            }
            ',' => {}
            _ => return None,
        }
    }
    (digits > 0).then_some(n)
}

/// "15,840.0" -> 158400 (décimos). Aceita sem decimal ("12") e com mais casas (trunca).
pub fn parse_tenths(s: &str) -> Option<u64> {
    let s = s.trim();
    let (int_part, dec_part) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    };
    let i = parse_int(int_part)?;
    let first_dec = match dec_part.chars().next() {
        None => 0,
        Some(c) if c.is_ascii_digit() => c as u64 - '0' as u64,
        Some(_) => return None,
    };
    if !dec_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    i.checked_mul(10)?.checked_add(first_dec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cliente_em_portugues() {
        let t = parse("TheClassic MU || Nome: xWizard || Level: 400 || Resets: 900 || WC: 0 || WP: 11 || GP: 2,950.0");
        assert_eq!(t.name.as_deref(), Some("xWizard"));
        assert_eq!((t.level, t.resets), (Some(400), Some(900)));
        assert!(t.is_logged_in());
        assert_eq!(parse("TheClassic MU || Nome: 0MAGIC || Level: 400").name.as_deref(), Some("0MAGIC"));
    }

    const REAL: &str =
        "TheClassic MU || Name: ReDMooN || Level: 400 || Resets: 911 || WC: 0 || WP: 21,519 || GP: 15,840.0";

    #[test]
    fn titulo_real() {
        let t = parse(REAL);
        assert_eq!(t.name.as_deref(), Some("ReDMooN"));
        assert_eq!(t.level, Some(400));
        assert_eq!(t.resets, Some(911));
        assert_eq!(t.wc, Some(0));
        assert_eq!(t.wp, Some(21_519));
        assert_eq!(t.gp_tenths, Some(158_400));
        assert!(t.is_logged_in());
    }

    #[test]
    fn titulo_tela_de_login() {
        let t = parse("TheClassic MU");
        assert!(!t.is_logged_in());
        assert_eq!(t, TitleInfo::default());
    }

    #[test]
    fn campos_quebrados_viram_none() {
        let t = parse("Name: A B || Level: 4x0 || Resets: 99999999999999 || GP: 1.x");
        assert_eq!(t.name, None);
        assert_eq!(t.level, None);
        assert_eq!(t.resets, None);
        assert_eq!(t.gp_tenths, None);
    }

    #[test]
    fn numeros() {
        assert_eq!(parse_int("1,234,567"), Some(1_234_567));
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int(","), None);
        assert_eq!(parse_tenths("12"), Some(120));
        assert_eq!(parse_tenths("12.75"), Some(127));
    }
}
