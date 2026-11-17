//! Leitura do número de HP (o "6429" em cima da esfera vermelha) por modelo de pixels.
//!
//! Diferente do Zen, a fonte da vida é larga, com contorno, e os dígitos podem encostar
//! um no outro. Por isso a leitura é por janela deslizante com tolerância pequena
//! (`TOL` pixels diferentes por dígito). A menor distância entre dois modelos é 13 px
//! (6 contra 8), então tolerância 2 não confunde dígitos.
//!
//! Modelos extraídos de 24 recortes reais rotulados à mão (ReDMooN e x9901, 07/11/2026);
//! validados em mais 48 recortes não vistos (`tests/fixtures/hp_field/`).
//! Isto é só a SEMENTE para achar a vida na memória — igual ao Zen.

use crate::glyphs::Bits;

pub const BAND_H: usize = 12;
/// Linha onde a faixa do número começa no recorte padrão do agente.
pub const BAND_Y: usize = 12;
/// Branco do texto: menor canal RGB >= 180 (o fundo é vermelho/rosa, então o menor canal é baixo).
pub const WHITE_MIN: u8 = 180;
const TOL: usize = 2;

const DIGITS: [(char, [&str; BAND_H]); 10] = [
    ('0', [".......", ".#####.", "###.##.", "##...##", "##...##", "##...##", "##...##", "##...##", "##...##", "##...##", "######.", ".#####."]),
    ('1', [".....", "...##", "..###", ".####", "#####", "...##", "...##", "...##", "...##", "...##", "...##", "...##"]),
    ('2', ["...#....", ".######.", ".##..##.", ".#...###", ".....##.", ".....##.", "....##..", "...##...", "..##....", ".###....", "########", "########"]),
    ('3', ["..##...", ".#####.", "##..##.", "....##.", "....##.", "...###.", "....##.", ".....##", ".....##", "##...##", "#######", ".#####."]),
    ('4', [".....#..", "....###.", "...####.", "..#####.", "..##.##.", ".##..##.", "##...##.", "##..###.", "########", "########", ".....##.", ".....##."]),
    ('5', [".......", "######.", "######.", "##.....", "##.#...", "######.", "##..##.", ".....##", ".....##", "##..###", "######.", ".####.."]),
    ('6', ["...#...", ".#####.", ".##.###", "##.....", "##.....", "######.", "###.###", "##...##", "##...##", "##...##", ".######", "..####."]),
    ('7', ["........", "########", "#######.", "....###.", "....##..", "...##...", "...##...", "...##...", "..##....", "..##....", "..##....", "..##...."]),
    ('8', ["..##...", "######.", "###.###", "##...##", "###.###", ".#####.", "###.##.", "##...##", "##...##", "##...##", "#######", ".#####."]),
    ('9', [".......", ".#####.", "###.###", "##...##", "##...##", "##..###", ".######", "..##.##", ".....##", ".....##", ".#####.", ".#####."]),
];

/// Variantes de dígitos vistas em outro PC (o mesmo dígito com outro desenho). ELISMAR-NOTE,
/// 11/11: o "9" do "908" do Pickle ficava a 5 px do modelo e a vida não era lida (distância
/// do modelo mais próximo de outro dígito: 18).
const EXTRA: [(char, [&str; BAND_H]); 1] = [
    ('9', ["...#...", ".#####.", "###..##", "##...##", "##...##", "###..##", ".######", "..##.##", ".....##", ".....##", ".######", ".#####."]),
];

struct Glyph {
    ch: char,
    w: usize,
    cols: Vec<[bool; BAND_H]>,
}

fn glyphs() -> Vec<Glyph> {
    DIGITS
        .iter()
        .chain(EXTRA.iter())
        .map(|(ch, rows)| {
            let w = rows[0].len();
            let cols = (0..w)
                .map(|x| {
                    let mut c = [false; BAND_H];
                    for (y, r) in rows.iter().enumerate() {
                        c[y] = r.as_bytes()[x] == b'#';
                    }
                    c
                })
                .collect();
            Glyph { ch: *ch, w, cols }
        })
        .collect()
}

/// Binariza um recorte BGRA (como vem do PrintWindow) pelo critério do texto branco.
pub fn binarize_bgra(bgra: &[u8], w: usize, h: usize) -> Bits {
    let mut b = Bits::new(w, h);
    for (i, p) in bgra.chunks_exact(4).take(w * h).enumerate() {
        if p[0].min(p[1]).min(p[2]) >= WHITE_MIN {
            b.set(i % w, i / w);
        }
    }
    b
}

fn dist(b: &Bits, x: isize, y0: usize, g: &Glyph) -> usize {
    let mut d = 0;
    for (i, col) in g.cols.iter().enumerate() {
        for (y, &want) in col.iter().enumerate() {
            if b.get(x + i as isize, (y0 + y) as isize) != want {
                d += 1;
            }
        }
    }
    d
}

fn best_at(b: &Bits, x: isize, y0: usize, gs: &[Glyph]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (i, g) in gs.iter().enumerate() {
        let d = dist(b, x, y0, g);
        if d > TOL {
            continue;
        }
        best = match best {
            Some((bd, bi)) if (bd, usize::MAX - gs[bi].w) <= (d, usize::MAX - g.w) => Some((bd, bi)),
            _ => Some((d, i)),
        };
    }
    best
}

/// Lê a faixa que começa na linha `y0`. Devolve (texto, distância total).
/// Recusa leitura PARCIAL: se sobrar texto branco na faixa fora dos dígitos lidos, devolve None.
/// (ELISMAR-NOTE, 11/11 00:30: o "902" do Pickle saía com outros pixels, só o "0" batia e a
/// leitura "0" confirmava uma morte que não aconteceu.)
fn decode_band(b: &Bits, y0: usize, gs: &[Glyph]) -> Option<(String, usize)> {
    let (s, d, x0, x1) = decode_band_span(b, y0, gs)?;
    let stray = (0..b.w as isize)
        .filter(|&x| x < x0 - 2 || x >= x1 + 2)
        .map(|x| (y0 + 2..y0 + BAND_H - 1).filter(|&y| b.get(x, y as isize)).count())
        .sum::<usize>();
    if stray > 8 {
        return None;
    }
    Some((s, d))
}

fn decode_band_span(b: &Bits, y0: usize, gs: &[Glyph]) -> Option<(String, usize, isize, isize)> {
    let w = b.w as isize;
    let mut x = 0isize;
    let (d0, i0) = loop {
        if x >= w {
            return None;
        }
        if let Some(hit) = best_at(b, x, y0, gs) {
            break hit;
        }
        x += 1;
    };
    let mut out = String::from(gs[i0].ch);
    let mut total = d0;
    let x_start = x;
    x += gs[i0].w as isize;
    loop {
        // Próximo dígito: pode encostar (até 2 px de sobreposição) ou ter até 5 px de vão.
        let mut cand: Option<(usize, isize, usize)> = None; // (dist, |nx-x|, glyph) + nx
        let mut cand_nx = 0;
        for nx in (x - 2)..(x + 6) {
            if nx < 0 {
                continue;
            }
            if let Some((d, i)) = best_at(b, nx, y0, gs) {
                let key = (d, (nx - x).abs(), i);
                if cand.map_or(true, |c| (key.0, key.1) < (c.0, c.1)) {
                    cand = Some(key);
                    cand_nx = nx;
                }
            }
        }
        let Some((d, _, i)) = cand else { break };
        out.push(gs[i].ch);
        total += d;
        x = cand_nx + gs[i].w as isize;
        if out.len() > 7 {
            return None;
        }
    }
    Some((out, total, x_start, x))
}

/// Lê o HP. Aceita a faixa recortada (12 linhas) ou o recorte inteiro: procura a faixa em
/// todas as alturas e fica com a leitura de menor distância. Recusa se nada bater.
pub fn read_hp(b: &Bits) -> Option<u32> {
    if b.h < BAND_H {
        return None;
    }
    let gs = glyphs();
    let mut best: Option<(usize, String)> = None;
    for y0 in 0..=(b.h - BAND_H) {
        if let Some((s, d)) = decode_band(b, y0, &gs) {
            // Mais dígitos ganha (um pedaço de dígito sozinho não deve vencer o número inteiro).
            let better = match &best {
                None => true,
                Some((bd, bs)) => s.len() > bs.len() || (s.len() == bs.len() && d < *bd),
            };
            if better {
                best = Some((d, s));
            }
        }
    }
    best.and_then(|(_, s)| s.parse().ok())
}

/// Pixels de `n` na fonte da vida, 1 px de vão entre dígitos, relativo ao canto da faixa.
/// Usado pelo fake-client para desenhar a vida exatamente como o agente lê.
pub fn render(n: u32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut x0 = 0;
    for ch in n.to_string().chars() {
        let Some((_, rows)) = DIGITS.iter().find(|(c, _)| *c == ch) else { continue };
        for (y, r) in rows.iter().enumerate() {
            for (i, c) in r.chars().enumerate() {
                if c == '#' {
                    out.push((x0 + i, y));
                }
            }
        }
        x0 += rows[0].len() + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modelos_tem_altura_certa() {
        for (_, rows) in DIGITS.iter() {
            assert!(rows.iter().all(|r| r.len() == rows[0].len()));
        }
    }

    #[test]
    fn render_ida_e_volta() {
        for n in [0u32, 7, 110, 6429, 6686, 12345] {
            let mut b = Bits::new(100, 36);
            for (x, y) in render(n) {
                b.set(20 + x, BAND_Y + y);
            }
            assert_eq!(read_hp(&b), Some(n));
        }
    }

    #[test]
    fn recorte_vazio_nao_le_nada() {
        assert_eq!(read_hp(&Bits::new(100, 36)), None);
    }

    #[test]
    fn numero_desenhado_com_os_modelos() {
        // Desenha "6429" e um "0" sozinho (vida zerada) com 1 px de vão, 12 linhas abaixo do topo.
        for (txt, want) in [("6429", 6429u32), ("0", 0), ("110", 110)] {
            let mut b = Bits::new(100, 36);
            let mut x = 20;
            for ch in txt.chars() {
                let (_, rows) = DIGITS.iter().find(|(c, _)| *c == ch).unwrap();
                for (y, r) in rows.iter().enumerate() {
                    for (i, c) in r.chars().enumerate() {
                        if c == '#' {
                            b.set(x + i, BAND_Y + y);
                        }
                    }
                }
                x += rows[0].len() + 1;
            }
            assert_eq!(read_hp(&b), Some(want), "{txt}");
        }
    }

    #[test]
    fn nove_do_notebook() {
        let mut b = Bits::new(60, BAND_H);
        let mut x0 = 2;
        for rows in [EXTRA[0].1, DIGITS[0].1, DIGITS[8].1] {
            for (y, r) in rows.iter().enumerate() {
                for (i, c) in r.chars().enumerate() {
                    if c == '#' {
                        b.set(x0 + i, y);
                    }
                }
            }
            x0 += rows[0].len() + 3;
        }
        assert_eq!(read_hp(&b), Some(908));
    }
}
