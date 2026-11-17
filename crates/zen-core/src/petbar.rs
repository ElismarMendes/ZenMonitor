//! Barra de vida do Dark Raven (pet do Dark Lord) no topo da tela, lida da TELA (sem memória).
//! Medida nos prints do YouSee em 11/11 (cliente 1030x797): moldura teal escura de 85x11 px
//! (y 68..78); a parte cheia é teal claro nas linhas 3..7 da moldura, até 78 px. A barra NÃO
//! fica sempre no mesmo x (no Dungeon em x 528, em Lorencia em x 224): procura na faixa do
//! topo e reconhece a do Raven pela palavra "Raven" 12 px acima (a do Pascualino é outra).

/// Faixa do topo onde as barras dos pets aparecem (x, y, largura, altura).
pub const RAVEN_SCAN: [i32; 4] = [100, 40, 920, 45];
/// "Raven" do rótulo (branco, menor canal >= 170), 38 px à direita e 13 px acima da moldura.
const LABEL: [&str; 8] = [
    ".............................",
    ".###.........................",
    ".#..#........................",
    ".#..#...##..#...#..##..####..",
    ".###......#..#.#..#..#.#...#.",
    ".#.#....###..#.#..####.#...#.",
    ".#..#..#..#..#.#..#....#...#.",
    ".#...#..###...#....###.#...#.",
];
/// Largura útil da parte cheia (px).
const FILL_W: usize = 78;

fn px(b: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * w + x) * 4;
    (b[i + 2], b[i + 1], b[i])
}

/// Teal escuro do fundo/moldura.
fn is_frame((r, g, b): (u8, u8, u8)) -> bool {
    r < 0x50 && (60..=110).contains(&g) && (70..=130).contains(&b)
}

/// Teal claro da vida.
fn is_fill((r, g, b): (u8, u8, u8)) -> bool {
    g > 180 && r < 120 && b > 100
}

fn frame_at(b: &[u8], w: usize, fx: usize, fy: usize) -> bool {
    // >= 82 de 85: a moldura certa até 3 px de folga (com 70, achava a mesma barra 15 vezes
    // deslocada e o rótulo caía no lugar errado: YouSee no Dungeon, 11/11 21:20).
    [0usize, 1, 9].iter().all(|&dy| (0..85).filter(|&x| is_frame(px(b, w, fx + x, fy + dy))).count() >= 82)
}

fn fill_at(b: &[u8], w: usize, fx: usize, fy: usize) -> u8 {
    // Conta os pixels cheios de cada linha 3..=7 (a moldura achada pode estar 1 px deslocada)
    // e fica com a mediana (um nome de mob por cima de
    // uma linha não estraga a leitura).
    let mut counts: Vec<usize> = (3..=7)
        .map(|dy| (1..84).filter(|&x| is_fill(px(b, w, fx + x, fy + dy))).count())
        .collect();
    counts.sort_unstable();
    ((counts[2] * 100 + FILL_W / 2) / FILL_W).min(100) as u8
}

fn label_at(b: &[u8], w: usize, h: usize, fx: usize, fy: usize) -> bool {
    for oy in -2i32..=1 {
        for ox in -4i32..=4 {
            let (x0, y0) = (fx as i32 + 38 + ox, fy as i32 - 13 + oy);
            if x0 < 0 || y0 < 0 || x0 as usize + 29 > w || y0 as usize + 8 > h {
                continue;
            }
            let mut d = 0;
            for (j, row) in LABEL.iter().enumerate() {
                for (i, c) in row.bytes().enumerate() {
                    let (r, g, bb) = px(b, w, x0 as usize + i, y0 as usize + j);
                    if (r.min(g).min(bb) >= 170) != (c == b'#') {
                        d += 1;
                    }
                }
            }
            if d <= 6 {
                return true;
            }
        }
    }
    false
}

/// Vida do Raven em % (0..=100) num recorte BGRA de 85x11 que é a própria moldura (testes).
pub fn raven_percent(bgra: &[u8], w: usize, h: usize) -> Option<u8> {
    if w < 85 || h < 11 || bgra.len() < w * h * 4 || !frame_at(bgra, w, 0, 0) {
        return None;
    }
    Some(fill_at(bgra, w, 0, 0))
}

/// Vida do Raven em % no recorte `RAVEN_SCAN`. Acha as molduras de barra de pet; fica com a
/// que tem "Raven" em cima. Sem rótulo legível (nome de mob por cima) e com duas barras, a da
/// esquerda (nos prints o Raven vem sempre antes do Pascualino). `None` = sem barra do Raven.
pub fn raven_in_scan(bgra: &[u8], w: usize, h: usize) -> Option<u8> {
    if w < 85 || h < 11 || bgra.len() < w * h * 4 {
        return None;
    }
    let mut frames = Vec::new();
    for fy in 0..=h - 11 {
        let mut fx = 0;
        while fx + 85 <= w {
            if is_frame(px(bgra, w, fx + 1, fy)) && frame_at(bgra, w, fx, fy) {
                frames.push((fx, fy));
                fx += 85;
            } else {
                fx += 1;
            }
        }
        if !frames.is_empty() {
            break; // as barras ficam na mesma linha: a primeira linha achada basta
        }
    }
    if let Some(&(fx, fy)) = frames.iter().find(|&&(fx, fy)| label_at(bgra, w, h, fx, fy)) {
        return Some(fill_at(bgra, w, fx, fy));
    }
    if frames.len() == 2 {
        let (fx, fy) = frames[0].min(frames[1]);
        return Some(fill_at(bgra, w, fx, fy));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_real_do_yousee_e_char_sem_raven() {
        let a = include_bytes!("raven_42.bgra");
        assert_eq!(raven_percent(a, 85, 11), Some(44));
        let b = include_bytes!("raven_none.bgra");
        assert_eq!(raven_percent(b, 85, 11), None);
    }

    #[test]
    fn acha_a_barra_em_qualquer_x() {
        let (w, h) = (820, 45);
        // Lorencia: barra do Raven em x 224, vazia (0%)
        assert_eq!(raven_in_scan(include_bytes!("raven_scan_lor.bgra"), w, h), Some(0));
        // Dungeon: barra em x 528, 44%
        assert_eq!(raven_in_scan(include_bytes!("raven_scan_dgn.bgra"), w, h), Some(44));
        // Dungeon, outro momento: barra do Raven em x 831 (vazia), Pascualino cortado à direita
        assert_eq!(raven_in_scan(include_bytes!("raven_scan_dgn2.bgra"), w, h), Some(0));
        // x9901 (Soul Master): só a barra do Pascualino
        assert_eq!(raven_in_scan(include_bytes!("raven_scan_none.bgra"), w, h), None);
    }

    #[test]
    fn cheia_e_vazia() {
        let mut v = include_bytes!("raven_42.bgra").to_vec();
        let set = |v: &mut Vec<u8>, x: usize, y: usize, c: (u8, u8, u8)| {
            let i = (y * 85 + x) * 4;
            v[i] = c.2;
            v[i + 1] = c.1;
            v[i + 2] = c.0;
        };
        for y in 3..=7 {
            for x in 3..3 + FILL_W {
                set(&mut v, x, y, (0x20, 0xE0, 0xA0));
            }
        }
        assert_eq!(raven_percent(&v, 85, 11), Some(100));
        for y in 3..=7 {
            for x in 3..3 + FILL_W {
                set(&mut v, x, y, (0x20, 0x50, 0x60));
            }
        }
        assert_eq!(raven_percent(&v, 85, 11), Some(0));
    }
}
