//! Lista de personagens do cliente (char, mobs, NPCs, outros jogadores), lida da memória do
//! jogo SÓ PARA LEITURA. Formato achado em 10/11/2026 no The Classic (x9901 e ReDMooN, dois
//! processos): entradas de 0x598 bytes; deslocamentos contados a partir do NOME da entrada.

/// Tamanho de cada entrada da lista.
pub const STRIDE: usize = 0x598;
/// Número do monstro/NPC (Safety Guardian = 240, Satyros = 353, Genocider = 357).
pub const OFF_CLASS: usize = 0x4c;
/// 1 = a entrada está em uso agora (na tela do cliente).
pub const OFF_LIVE: usize = 0x2d4;
/// 1 = jogador, 2 = monstro, 4 = NPC.
pub const OFF_KIND: usize = 0x2e8;
/// Posição no mundo: f32 x, f32 y (tile = valor / 100).
pub const OFF_POS: usize = 0x390;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Kind {
    Player,
    Monster,
    Npc,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Obj {
    pub kind: Kind,
    pub name: String,
    pub class: u16,
    pub x: i32,
    pub y: i32,
}

/// Golden Monster pelo nome (guia oficial: Golden Goblin, Budge Golden Dragon, Golden Titan,
/// Golden Soldier, Golden Lizard King, Golden Vepar, Golden Tantalos, Golden Wheel). A conferir
/// no cliente na primeira invasão: o nome na memória pode ser diferente do guia.
///
/// Visto em 18/11 04:00 em Lorencia: "Golden Budge Dragon" classe 43 (o guia escreve "Budge
/// Golden Dragon") e, aos 20 min do evento, "Great Golden Dragon" classe 501, que matou o x9901
/// (1026 resets) em ~10 s. Chefe perigoso: NÃO é alvo (ver `is_golden_boss`).
pub fn is_golden(o: &Obj) -> bool {
    o.kind == Kind::Monster && o.name.to_ascii_lowercase().contains("golden") && !is_golden_boss(o)
}

/// Chefe dourado que mata o char: fugir/não chegar perto.
pub fn is_golden_boss(o: &Obj) -> bool {
    o.kind == Kind::Monster && (o.class == 501 || o.name.to_ascii_lowercase().starts_with("great golden"))
}

/// Lê UMA entrada (começando no nome). `None` se não estiver em uso ou não parecer válida.
pub fn parse_entry(e: &[u8]) -> Option<Obj> {
    if e.len() < OFF_POS + 8 || e[OFF_LIVE] != 1 {
        return None;
    }
    let kind = match e[OFF_KIND] {
        1 => Kind::Player,
        2 => Kind::Monster,
        4 => Kind::Npc,
        _ => return None,
    };
    // Nome de monstro pode passar de 16 letras ("Budge Golden Dragon" tem 19). O campo vai até
    // a classe (0x4c); 32 basta.
    let end = e[..32].iter().position(|&c| c == 0)?;
    if end < 2 || !e[..end].iter().all(|&c| (0x20..0x7f).contains(&c)) {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([e[o], e[o + 1], e[o + 2], e[o + 3]]);
    let (x, y) = (f(OFF_POS), f(OFF_POS + 4));
    if !(x.is_finite() && y.is_finite() && (100.0..25600.0).contains(&x) && (100.0..25600.0).contains(&y)) {
        return None;
    }
    Some(Obj {
        kind,
        name: String::from_utf8_lossy(&e[..end]).into_owned(),
        class: u16::from_le_bytes([e[OFF_CLASS], e[OFF_CLASS + 1]]),
        x: (x / 100.0) as i32,
        y: (y / 100.0) as i32,
    })
}

/// Todas as entradas vivas num bloco lido da memória, alinhadas à entrada `anchor` (deslocamento
/// do nome do próprio char dentro de `buf`).
pub fn parse_block(buf: &[u8], anchor: usize) -> Vec<Obj> {
    let mut out = Vec::new();
    let first = anchor % STRIDE;
    let mut o = first;
    while o + STRIDE <= buf.len() {
        if let Some(obj) = parse_entry(&buf[o..o + STRIDE]) {
            out.push(obj);
        }
        o += STRIDE;
    }
    out
}

#[cfg(test)]
mod golden_tests {
    use super::*;
    fn m(name: &str, class: u16) -> Obj { Obj { kind: Kind::Monster, name: name.into(), class, x: 0, y: 0 } }
    #[test]
    fn golden_e_chefe() {
        assert!(is_golden(&m("Golden Budge Dragon", 43)));
        assert!(!is_golden(&m("Budge Dragon", 2)));
        assert!(!is_golden(&m("Great Golden Dragon", 501)));
        assert!(is_golden_boss(&m("Great Golden Dragon", 501)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: u8, live: u8, class: u16, x: f32, y: f32) -> Vec<u8> {
        let mut e = vec![0u8; STRIDE];
        e[..name.len()].copy_from_slice(name.as_bytes());
        e[OFF_CLASS..OFF_CLASS + 2].copy_from_slice(&class.to_le_bytes());
        e[OFF_LIVE] = live;
        e[OFF_KIND] = kind;
        e[OFF_POS..OFF_POS + 4].copy_from_slice(&x.to_le_bytes());
        e[OFF_POS + 4..OFF_POS + 8].copy_from_slice(&y.to_le_bytes());
        e
    }

    #[test]
    fn le_mobs_vivos_e_ignora_velhos() {
        let mut buf = vec![0u8; 100];
        buf.extend(entry("Satyros", 2, 1, 353, 12060.0, 2540.0));
        buf.extend(entry("Safety Guardian", 4, 0, 240, 10950.0, 13750.0)); // velho
        buf.extend(entry("x9901", 1, 1, 503, 12150.0, 2550.0));
        let v = parse_block(&buf, 100 + 2 * STRIDE);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], Obj { kind: Kind::Monster, name: "Satyros".into(), class: 353, x: 120, y: 25 });
        assert_eq!(v[1].kind, Kind::Player);
    }
}
