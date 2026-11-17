//! Leitura do campo do Zen por MODELO de pixels, sem OCR.
//!
//! A fonte do inventário do The Classic é de largura fixa e idêntica pixel a pixel em
//! todas as aparições (verificado em 3 prints reais, 32 glifos: 1 variante por dígito).
//! Por isso a leitura é exata: ou cada célula bate com um modelo, ou a leitura é recusada.
//! OCR genérico (Windows.Media.Ocr) leu "313,637 *884" no lugar de "913,697,884".
//!
//! Dígito: célula de 5 px de largura. Vírgula: 3 px. Altura: 7 linhas + 1 da cauda da vírgula.

pub const DIGIT_W: usize = 5;
pub const COMMA_W: usize = 3;
pub const CELL_H: usize = 8;

/// Modelos extraídos dos prints reais de 07/11/2026 (`tests/fixtures/zen_field/`).
const DIGITS: [(char, [&str; 7]); 10] = [
    ('0', [".##..", "#..#.", "#..#.", "#..#.", "#..#.", "#..#.", ".##.."]),
    ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."]),
    ('2', ["###..", "...#.", "...#.", "..#..", ".#...", "#....", "####."]),
    ('3', ["###..", "...#.", "...#.", ".##..", "...#.", "...#.", "###.."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."]),
    ('5', ["####.", "#....", "#....", "###..", "...#.", "...#.", "###.."]),
    ('6', [".##..", "#....", "#....", "###..", "#..#.", "#..#.", ".##.."]),
    ('7', ["####.", "...#.", "..#..", "..#..", ".#...", ".#...", "#...."]),
    ('8', [".##..", "#..#.", "#..#.", ".##..", "#..#.", "#..#.", ".##.."]),
    ('9', [".##..", "#..#.", "#..#.", ".###.", "...#.", "...#.", ".##.."]),
];
const COMMA: [&str; CELL_H] = ["...", "...", "...", "...", "...", ".#.", ".#.", "#.."];

/// Pixels já binarizados (true = tinta).
#[derive(Debug, Clone, PartialEq)]
pub struct Bits {
    pub w: usize,
    pub h: usize,
    pub px: Vec<bool>,
}

impl Bits {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![false; w * h],
        }
    }

    pub fn get(&self, x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.px[y as usize * self.w + x as usize]
    }

    pub fn set(&mut self, x: usize, y: usize) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = true;
        }
    }

    /// '#' = tinta, qualquer outro caractere = fundo. Uma linha de texto por linha de pixels.
    /// Inverso de `from_text`: '#' tinta, '.' vazio, uma linha por linha.
    pub fn to_text(&self) -> String {
        let mut s = String::with_capacity((self.w + 1) * self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                s.push(if self.px[y * self.w + x] { '#' } else { '.' });
            }
            s.push('\n');
        }
        s
    }

    pub fn from_text(s: &str) -> Self {
        let rows: Vec<&str> = s.lines().filter(|l| !l.is_empty()).collect();
        let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        let mut b = Self::new(w, rows.len());
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.chars().enumerate() {
                if c == '#' {
                    b.set(x, y);
                }
            }
        }
        b
    }
}

/// Mesmo critério do tratamento de imagem: maior canal RGB (o Zen é vermelho), contraste
/// esticado no recorte, limiar 110/255.
pub fn binarize_bgra(bgra: &[u8], w: usize, h: usize) -> Bits {
    let lum: Vec<u8> = bgra
        .chunks_exact(4)
        .take(w * h)
        .map(|p| p[0].max(p[1]).max(p[2]))
        .collect();
    let (lo, hi) = lum.iter().fold((255u8, 0u8), |(a, b), &v| (a.min(v), b.max(v)));
    let span = (hi.saturating_sub(lo)).max(1) as u32;
    let mut b = Bits::new(w, h);
    for (i, &v) in lum.iter().enumerate() {
        if (v.saturating_sub(lo)) as u32 * 255 / span > 110 {
            b.px[i] = true;
        }
    }
    b
}

fn cell_distance(b: &Bits, x0: isize, y0: isize, model: &[&str]) -> usize {
    let mut d = 0;
    for (dy, row) in model.iter().enumerate() {
        for (dx, c) in row.chars().enumerate() {
            if b.get(x0 + dx as isize, y0 + dy as isize) != (c == '#') {
                d += 1;
            }
        }
    }
    d
}

fn digit_model<'a>(m: &[&'a str; 7]) -> [&'a str; CELL_H] {
    [m[0], m[1], m[2], m[3], m[4], m[5], m[6], "....."]
}

fn column_has_ink(b: &Bits, x: isize, y0: isize) -> bool {
    (0..CELL_H as isize).any(|dy| b.get(x, y0 + dy))
}

/// Tolerância: até 1 pixel diferente por célula. Os modelos diferem entre si em 3+ pixels
/// (teste `modelos_bem_separados`), então 1 de tolerância nunca confunde dois dígitos.
const TOLERANCE: usize = 1;

fn decode_from(b: &Bits, start: isize, top: isize) -> Option<String> {
    let mut out = String::new();
    let mut x = start;
    loop {
        if !(x..b.w as isize).any(|cx| column_has_ink(b, cx, top)) {
            break;
        }
        let mut scores: Vec<(usize, char)> = DIGITS
            .iter()
            .map(|(c, m)| (cell_distance(b, x, top, &digit_model(m)), *c))
            .collect();
        scores.sort();
        if scores[0].0 <= TOLERANCE && scores[1].0 > scores[0].0 {
            out.push(scores[0].1);
            x += DIGIT_W as isize;
            continue;
        }
        if cell_distance(b, x, top, &COMMA) <= TOLERANCE {
            out.push(',');
            x += COMMA_W as isize;
            continue;
        }
        return None;
    }
    Some(out)
}

/// "913,697,884" -> 913697884, só se o formato de milhar estiver certo.
fn parse_grouped(s: &str) -> Option<u64> {
    let groups: Vec<&str> = s.split(',').collect();
    let first = groups.first()?;
    if first.is_empty() || first.len() > 3 || groups[1..].iter().any(|g| g.len() != 3) {
        return None;
    }
    if groups.len() > 1 && first.starts_with('0') {
        return None;
    }
    let digits: String = groups.concat();
    if digits.len() > 10 {
        return None;
    }
    digits.parse().ok()
}

/// Lê o número do campo. `None` se qualquer pixel não se encaixar num modelo.
pub fn read_number(b: &Bits) -> Option<u64> {
    let top = (0..b.h).find(|&y| (0..b.w).any(|x| b.px[y * b.w + x]))? as isize;
    // Tinta fora da faixa de 8 linhas = não é o campo (ou região errada).
    if (top as usize + CELL_H..b.h).any(|y| (0..b.w).any(|x| b.px[y * b.w + x])) {
        return None;
    }
    let first = (0..b.w).find(|&x| column_has_ink(b, x as isize, top))? as isize;
    // A primeira tinta pode estar 0, 1 ou 2 px dentro da célula (o "1" começa na coluna 1).
    for off in 0..=2 {
        let start = first - off;
        if let Some(s) = decode_from(b, start, top) {
            if let Some(n) = parse_grouped(&s) {
                return Some(n);
            }
        }
    }
    None
}

/// Binariza texto BRANCO (coordenadas do canto da tela): menor canal RGB >= `thr`.
pub fn binarize_white(bgra: &[u8], w: usize, h: usize, thr: u8) -> Bits {
    let mut b = Bits::new(w, h);
    for (i, p) in bgra.chunks_exact(4).take(w * h).enumerate() {
        if p[0].min(p[1]).min(p[2]) >= thr {
            b.set(i % w, i / w);
        }
    }
    b
}

/// Limiar do texto das coordenadas (medido em 44 recortes reais de 08/11/2026).
pub const COORDS_WHITE: u8 = 150;

/// Lê "Mapa (X , Y)" do canto superior esquerdo: devolve (X, Y).
/// A fonte é a MESMA do campo do Zen. Procura dígitos em qualquer altura, junta dígitos
/// colados (passo de 5 px) em números e fica com os dois últimos (X e Y, 0..=255).
/// Nome do mapa não é lido aqui.
pub fn read_coords(b: &Bits) -> Option<(u32, u32)> {
    if b.h < CELL_H {
        return None;
    }
    for top in 0..=(b.h - CELL_H) as isize {
        let mut hits: Vec<(isize, char)> = Vec::new();
        let mut x = 0isize;
        while x + DIGIT_W as isize <= b.w as isize {
            let mut sc: Vec<(usize, char)> = DIGITS
                .iter()
                .map(|(c, m)| (cell_distance(b, x, top, &digit_model(m)), *c))
                .collect();
            sc.sort();
            // Coluna vazia à esquerda só no 1º dígito do número: o "4" usa as 5 colunas da
            // célula e encosta no dígito seguinte ("49" já foi lido como "4" por causa disso).
            let continues = hits.last().is_some_and(|(lx, _)| x - lx == DIGIT_W as isize);
            let left_clear = continues || !(0..7).any(|dy| b.get(x - 1, top + dy));
            if sc[0].0 <= TOLERANCE && sc[1].0 > sc[0].0 && left_clear {
                hits.push((x, sc[0].1));
                x += DIGIT_W as isize;
            } else {
                x += 1;
            }
        }
        let mut nums: Vec<(String, isize)> = Vec::new();
        for (x, c) in hits {
            match nums.last_mut() {
                Some((n, lx)) if x - *lx == DIGIT_W as isize => {
                    n.push(c);
                    *lx = x;
                }
                _ => nums.push((c.to_string(), x)),
            }
        }
        if nums.len() >= 2 {
            let k = nums.len();
            // Dígito que não bateu com modelo, colado no número (ex.: "49" lido como "4"):
            // a coluna logo depois do último dígito reconhecido tem de estar vazia.
            let clean = |lx: isize| !(0..7).any(|dy| b.get(lx + DIGIT_W as isize, top + dy));
            if !clean(nums[k - 2].1) || !clean(nums[k - 1].1) {
                return None;
            }
            let (a, b2) = (&nums[k - 2].0, &nums[k - 1].0);
            if a.len() <= 3 && b2.len() <= 3 {
                if let (Ok(x), Ok(y)) = (a.parse::<u32>(), b2.parse::<u32>()) {
                    if x <= 255 && y <= 255 {
                        return Some((x, y));
                    }
                }
            }
        }
    }
    None
}

/// "Kanturu" no canto superior esquerdo (mesmo recorte das coordenadas), tirado de recorte real.
const KANTURU: [&str; 7] = [
    "#...#............#..................",
    "#..#.............#..................",
    "#.#....##..####..##.#...#.#.#.#...#.",
    "##.......#.#...#.#..#...#.##..#...#.",
    "#.#....###.#...#.#..#...#.#...#...#.",
    "#..#..#..#.#...#.#..#...#.#...#...#.",
    "#...#..###.#...#..#..####.#....####.",
];

/// O char está no Kanturu (1)? Procura a palavra em qualquer posição (o texto é centralizado
/// e anda conforme o tamanho das coordenadas) e exige que o próximo caractere seja o "(" das
/// coordenadas — "Kanturu2"/"Kanturu Relics" não passam.
pub fn map_is_kanturu(b: &Bits) -> bool {
    map_word(b, &KANTURU)
}

/// "Lorencia" no canto (prints do ReDMooN em 09/11, cliente em inglês).
const LORENCIA: [&str; 7] = [
    "#..............................#......",
    "#.....................................",
    "#.....###..#.#..##..####...###.#..##..",
    "#....#...#.##..#..#.#...#.#....#....#.",
    "#....#...#.#...####.#...#.#....#..###.",
    "#....#...#.#...#....#...#.#....#.#..#.",
    "####..###..#....###.#...#..###.#..###.",
];

/// O char está em Lorencia? Mesmo critério do Kanturu (palavra + "(" logo depois).
pub fn map_is_lorencia(b: &Bits) -> bool {
    map_word(b, &LORENCIA)
}

/// "Arena" no canto (prints do 0MAGIC/xWizard em 08/11; igual em inglês e português).
const ARENA: [&str; 7] = [
    "..##........................",
    "..##........................",
    ".#..#..#.#..##..####...##...",
    ".#..#..##..#..#.#...#....#..",
    ".####..#...####.#...#..###..",
    "#....#.#...#....#...#.#..#..",
    "#....#.#....###.#...#..###..",
];

/// O char está na Arena?
pub fn map_is_arena(b: &Bits) -> bool {
    map_word(b, &ARENA)
}

/// "Lost Tower" no canto (print do Pickle em 11/11, cliente em inglês).
const LOST_TOWER: [&str; 7] = [
    "#..............#.....#####........................",
    "#..............#.......#..........................",
    "#.....###..###.##......#....###..#..#..#..##..#.#.",
    "#....#...#.#...#.......#...#...#.#..#..#.#..#.##..",
    "#....#...#..#..#.......#...#...#.#.#.#.#.####.#...",
    "#....#...#...#.#.......#...#...#..##.##..#....#...",
    "####..###..###..#......#....###...#...#...###.#...",
];

/// O char está na Lost Tower (qualquer andar: o nome no canto é o mesmo)?
pub fn map_is_lost_tower(b: &Bits) -> bool {
    map_word(b, &LOST_TOWER)
}

/// "Dungeon" no canto (print do YouSee em 11/11, cliente em inglês).
const DUNGEON: [&str; 7] = [
    "####.......................................",
    "#...#......................................",
    "#....#.#...#.####...####..##...###..####...",
    "#....#.#...#.#...#.#...#.#..#.#...#.#...#..",
    "#....#.#...#.#...#.#...#.####.#...#.#...#..",
    "#...#..#...#.#...#.#...#.#....#...#.#...#..",
    "####....####.#...#..####..###..###..#...#..",
];

/// O char está no Dungeon (qualquer andar)?
pub fn map_is_dungeon(b: &Bits) -> bool {
    map_word(b, &DUNGEON)
}

/// Nomes no canto tirados das chegadas do x9901 pelo M (11/11, cliente em inglês).
const DEVIAS: [&str; 7] = [
    "####..............#............",
    "#...#..........................",
    "#....#..##..#...#.#..##..###...",
    "#....#.#..#..#.#..#....#.#.....",
    "#....#.####..#.#..#..###..#....",
    "#...#..#.....#.#..#.#..#...#...",
    "####....###...#...#..###.###...",
];
const NORIA: [&str; 7] = [
    "#....#...........#........",
    "##...#....................",
    "#.#..#..###..#.#.#..##....",
    "#..#.#.#...#.##..#....#...",
    "#...##.#...#.#...#..###...",
    "#....#.#...#.#...#.#..#...",
    "#....#..###..#...#..###...",
];
const TARKAN: [&str; 7] = [
    "#####..........#..................",
    "..#............#..................",
    "..#....##..#.#.#..#...##..####....",
    "..#......#.##..#.#......#.#...#...",
    "..#....###.#...###....###.#...#...",
    "..#...#..#.#...#..#..#..#.#...#...",
    "..#....###.#...#...#..###.#...#...",
];
const ATLANS: [&str; 7] = [
    "..##...#..#..................",
    "..##...#..#..................",
    ".#..#..##.#..##..####..###...",
    ".#..#..#..#....#.#...#.#.....",
    ".####..#..#..###.#...#..#....",
    "#....#.#..#.#..#.#...#...#...",
    "#....#..#.#..###.#...#.###...",
];
const ICARUS: [&str; 7] = [
    "###...........................",
    ".#............................",
    ".#...###..##..#.#.#...#.###...",
    ".#..#.......#.##..#...#.#.....",
    ".#..#.....###.#...#...#..#....",
    ".#..#....#..#.#...#...#...#...",
    "###..###..###.#....####.###...",
];
const VALLEY_OF_LOREN: [&str; 7] = [
    "#...#......#.#.....................#.....#...........................",
    "#...#......#.#.....................#.....#...........................",
    "#...#..##..#.#..##..#...#.....###..###...#.....###..#.#..##..####....",
    ".#.#.....#.#.#.#..#..#.#.....#...#.#.....#....#...#.##..#..#.#...#...",
    ".#.#...###.#.#.####..#.#.....#...#.#.....#....#...#.#...####.#...#...",
    ".#.#..#..#.#.#.#.....#.#.....#...#.#.....#....#...#.#...#....#...#...",
    "..#....###.#.#..###...#.......###..#.....####..###..#....###.#...#...",
];
const CRYWOLF_FORTRESS: [&str; 7] = [
    "..####.........................#.#.....#####...........#.....................",
    ".#.............................#.#.....#...............#.....................",
    "#......#.#.#...#.#..#..#..###..#.###...#......###..#.#.##.#.#..##..###.###...",
    "#......##...#.#..#..#..#.#...#.#.#.....#####.#...#.##..#..##..#..#.#...#.....",
    "#......#....#.#..#.#.#.#.#...#.#.#.....#.....#...#.#...#..#...####..#...#....",
    ".#.....#....#.#...##.##..#...#.#.#.....#.....#...#.#...#..#...#......#...#...",
    "..####.#.....#....#...#...###..#.#.....#......###..#....#.#....###.###.###...",
];
const AIDA: [&str; 7] = [
    "..##...#.....#........",
    "..##.........#........",
    ".#..#..#..####..##....",
    ".#..#..#.#...#....#...",
    ".####..#.#...#..###...",
    "#....#.#.#...#.#..#...",
    "#....#.#..####..###...",
];

/// Todos os mapas conhecidos pelo nome no canto (o texto antes das coordenadas).
pub fn map_name(b: &Bits) -> Option<&'static str> {
    let all: [(&'static str, &[&str]); 13] = [
        ("Kanturu", &KANTURU),
        ("Arena", &ARENA),
        ("Lorencia", &LORENCIA),
        ("Lost Tower", &LOST_TOWER),
        ("Dungeon", &DUNGEON),
        ("Devias", &DEVIAS),
        ("Noria", &NORIA),
        ("Tarkan", &TARKAN),
        ("Atlans", &ATLANS),
        ("Icarus", &ICARUS),
        ("Valley of Loren", &VALLEY_OF_LOREN),
        ("Crywolf Fortress", &CRYWOLF_FORTRESS),
        ("Aida", &AIDA),
    ];
    all.into_iter().find(|(_, w)| map_word(b, w)).map(|(n, _)| n)
}

fn map_word(b: &Bits, word: &[&str]) -> bool {
    let (h, w) = (word.len() as isize, word[0].len() as isize);
    for top in 1..(b.h as isize - h) {
        for x in 0..(b.w as isize - w) {
            let mut d = 0;
            'm: for (j, row) in word.iter().enumerate() {
                for (i, c) in row.bytes().enumerate() {
                    if b.get(x + i as isize, top + j as isize) != (c == b'#') {
                        d += 1;
                        if d > 4 {
                            break 'm;
                        }
                    }
                }
            }
            if d > 4 {
                continue;
            }
            // Próximo caractere com tinta: tem de subir acima e descer abaixo da faixa ("(").
            let next = (x + w..(x + w + 10).min(b.w as isize)).find(|&cx| (top - 1..top + h + 1).any(|cy| b.get(cx, cy)));
            if let Some(cx) = next {
                let paren = (cx..cx + 3).any(|c| b.get(c, top - 1)) && (cx..cx + 3).any(|c| b.get(c, top + h));
                if paren {
                    return true;
                }
            }
        }
    }
    false
}

/// Desenha um número na mesma fonte (para o fake-client e para os testes).
/// Devolve os pixels de tinta a partir de (0, 0).
pub fn render(n: u64) -> Vec<(usize, usize)> {
    let mut px = Vec::new();
    let mut x = 0;
    for ch in crate::fmt::int_br(n).replace('.', ",").chars() {
        let (model, w): ([&str; CELL_H], usize) = if ch == ',' {
            (COMMA, COMMA_W)
        } else {
            let m = DIGITS.iter().find(|(c, _)| *c == ch).map(|(_, m)| m).expect("dígito");
            (digit_model(m), DIGIT_W)
        };
        for (dy, row) in model.iter().enumerate() {
            for (dx, c) in row.chars().enumerate() {
                if c == '#' {
                    px.push((x + dx, dy));
                }
            }
        }
        x += w;
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas_with(n: u64, ox: usize, oy: usize) -> Bits {
        let mut b = Bits::new(78, 14);
        for (x, y) in render(n) {
            b.set(ox + x, oy + y);
        }
        b
    }

    #[test]
    fn modelos_bem_separados() {
        let all: Vec<[&str; CELL_H]> = DIGITS.iter().map(|(_, m)| digit_model(m)).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let mut bits = Bits::new(DIGIT_W, CELL_H);
                for (y, row) in a.iter().enumerate() {
                    for (x, c) in row.chars().enumerate() {
                        if c == '#' {
                            bits.set(x, y);
                        }
                    }
                }
                assert!(cell_distance(&bits, 0, 0, b) >= 3, "{a:?} x {b:?}");
            }
        }
    }

    #[test]
    fn ida_e_volta_varios_tamanhos_e_posicoes() {
        for n in [
            0u64,
            1,
            7,
            10,
            111,
            1_000,
            12_345,
            999_999,
            1_000_000,
            797_124_492,
            1_999_999_999,
            2_000_000_000,
        ] {
            for ox in [0usize, 3, 13, 20] {
                for oy in [0usize, 5, 6] {
                    let b = canvas_with(n, ox, oy);
                    if crate::fmt::int_br(n).len() * 5 + ox > 78 {
                        continue;
                    }
                    assert_eq!(read_number(&b), Some(n), "n={n} ox={ox} oy={oy}");
                }
            }
        }
    }

    #[test]
    fn um_pixel_de_ruido_tolera_dois_nao() {
        let mut b = canvas_with(913_697_884, 13, 5);
        b.set(13, 8); // 1 pixel a mais dentro do "9" (fundo dele)
        assert_eq!(read_number(&b), Some(913_697_884));
        b.set(17, 5); // outro na mesma célula (coluna vazia do "9")
        assert_eq!(read_number(&b), None);
    }

    #[test]
    fn lixo_e_recusado() {
        assert_eq!(read_number(&Bits::new(78, 14)), None);
        let mut b = canvas_with(123, 13, 5);
        b.set(70, 0); // tinta acima da faixa
        assert_eq!(read_number(&b), None);
        let mut b = Bits::new(78, 14);
        for x in 10..60 {
            b.set(x, 7); // uma linha reta não é número
        }
        assert_eq!(read_number(&b), None);
    }

    #[test]
    fn agrupamento_de_milhar() {
        assert_eq!(parse_grouped("1,234"), Some(1234));
        assert_eq!(parse_grouped("12,34"), None);
        assert_eq!(parse_grouped("1234"), None); // o jogo sempre mostra a vírgula
        assert_eq!(parse_grouped("123"), Some(123));
        assert_eq!(parse_grouped(",123"), None);
        assert_eq!(parse_grouped("01,234"), None);
    }

    #[test]
    fn coordenada_com_digito_estragado_e_recusada() {
        // Visto em 08/11: "(49 , 227)" lido como (4, 227) porque o "9" não bateu.
        let draw = |b: &mut Bits, ch: char, x0: usize| {
            let (_, m) = DIGITS.iter().find(|(c, _)| *c == ch).unwrap();
            for (y, r) in m.iter().enumerate() {
                for (x, c) in r.chars().enumerate() {
                    if c == '#' {
                        b.set(x0 + x, 5 + y);
                    }
                }
            }
        };
        let mut b = Bits::new(80, 20);
        for (i, c) in "49".chars().enumerate() {
            draw(&mut b, c, 10 + i * DIGIT_W);
        }
        for (i, c) in "227".chars().enumerate() {
            draw(&mut b, c, 35 + i * DIGIT_W);
        }
        assert_eq!(read_coords(&b), Some((49, 227)));
        // Estraga o "9" (3 pixels): tem de recusar, nunca devolver 4.
        b.set(16, 9);
        b.set(17, 9);
        b.set(17, 10);
        b.px[10 * b.w + 18] = false;
        b.px[6 * b.w + 16] = false;
        assert_eq!(read_coords(&b), None);
    }

    #[test]
    fn kanturu_so_com_parenteses_depois() {
        let mut b = Bits::new(120, 24);
        for (j, row) in KANTURU.iter().enumerate() {
            for (i, c) in row.chars().enumerate() {
                if c == '#' {
                    b.set(10 + i, 7 + j);
                }
            }
        }
        // "Kanturu" seguido de "2" (Kanturu 2): não é o Kanturu 1.
        let mut k2 = b.clone();
        let (_, m) = DIGITS.iter().find(|(c, _)| *c == '2').unwrap();
        for (j, r) in m.iter().enumerate() {
            for (i, c) in r.chars().enumerate() {
                if c == '#' {
                    k2.set(50 + i, 7 + j);
                }
            }
        }
        assert!(!map_is_kanturu(&k2));
        // Com "(" (sobe uma linha e desce uma linha além da faixa): é.
        for y in 6..15 {
            b.set(if (8..13).contains(&y) { 50 } else { 51 }, y);
        }
        assert!(map_is_kanturu(&b));
    }

    #[test]
    fn lost_tower_e_dungeon() {
        for (word, f) in [(&LOST_TOWER, map_is_lost_tower as fn(&Bits) -> bool), (&DUNGEON, map_is_dungeon)] {
            let mut b = Bits::new(140, 24);
            for (j, row) in word.iter().enumerate() {
                for (i, c) in row.chars().enumerate() {
                    if c == '#' {
                        b.set(10 + i, 7 + j);
                    }
                }
            }
            let px = 10 + word[0].len() + 2;
            for y in 6..15 {
                b.set(if (8..13).contains(&y) { px } else { px + 1 }, y);
            }
            assert!(f(&b));
            assert!(!map_is_kanturu(&b) && !map_is_lorencia(&b) && !map_is_arena(&b));
        }
        let mut lt = Bits::new(140, 24);
        for (j, row) in LOST_TOWER.iter().enumerate() {
            for (i, c) in row.chars().enumerate() {
                if c == '#' {
                    lt.set(10 + i, 7 + j);
                }
            }
        }
        assert!(!map_is_dungeon(&lt));
    }

    #[test]
    fn nomes_dos_mapas_nao_se_confundem() {
        let all: [(&str, &[&str]); 13] = [
            ("Kanturu", &KANTURU), ("Arena", &ARENA), ("Lorencia", &LORENCIA), ("Lost Tower", &LOST_TOWER), ("Dungeon", &DUNGEON),
            ("Devias", &DEVIAS),
            ("Noria", &NORIA),
            ("Tarkan", &TARKAN),
            ("Atlans", &ATLANS),
            ("Icarus", &ICARUS),
            ("Valley of Loren", &VALLEY_OF_LOREN),
            ("Crywolf Fortress", &CRYWOLF_FORTRESS),
            ("Aida", &AIDA),
        ];
        for (name, word) in all {
            let mut b = Bits::new(160, 24);
            for (j, row) in word.iter().enumerate() {
                for (i, c) in row.chars().enumerate() {
                    if c == '#' {
                        b.set(10 + i, 7 + j);
                    }
                }
            }
            let px = 10 + word[0].len() + 2;
            for y in 6..15 {
                b.set(if (8..13).contains(&y) { px } else { px + 1 }, y);
            }
            assert_eq!(map_name(&b), Some(name), "{name}");
        }
    }
}
