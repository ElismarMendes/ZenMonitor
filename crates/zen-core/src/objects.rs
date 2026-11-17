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
    let end = e[..16].iter().position(|&c| c == 0)?;
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
