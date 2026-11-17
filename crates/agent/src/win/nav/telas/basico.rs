use crate::win::nav::*;


pub const CORNER: [i32; 4] = [0, 30, 300, 24];
pub(in crate::win::nav) const VK_HOME: u16 = 0x24;
pub(in crate::win::nav) const VK_M: u16 = 0x4D;
/// Botão "Kanturu" na janela do M (print de 08/11, cliente 1030x797).
pub(in crate::win::nav) const M_KANTURU: (i32, i32) = (612, 323);
/// Centro dos cliques de movimento (o char fica no meio da tela).
pub(in crate::win::nav) const CENTER: (i32, i32) = (515, 400);
/// Pasta World do Kanturu 1.
pub const KANTURU_WORLD: u8 = 38;

pub(in crate::win::nav) fn sleep(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub(in crate::win::nav) fn cheb(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

pub(in crate::win::nav) fn corner(hwnd: isize) -> Option<Bits> {
    let img = capture::capture(hwnd, Some(CORNER))?;
    Some(binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE))
}

/// (está no Kanturu?, posição). Mapa só é afirmado quando as coordenadas são legíveis.
pub fn observe_map(hwnd: isize) -> (Option<bool>, Option<(i32, i32)>) {
    let Some(b) = corner(hwnd) else { return (None, None) };
    let pos = read_coords(&b).map(|(x, y)| (x as i32, y as i32));
    (pos.map(|_| map_is_kanturu(&b)), pos)
}

pub fn position(hwnd: isize) -> Option<(i32, i32)> {
    corner(hwnd).and_then(|b| read_coords(&b)).map(|(x, y)| (x as i32, y as i32))
}

/// `Some(true)` pause = ligado, `Some(false)` play = desligado, `None` não reconhecido.
pub fn helper_state(hwnd: isize) -> Option<bool> {
    // O ícone nem sempre está no mesmo x: no DESKTOP-KIS9DG7 (09/11) a barra de cima vem 25 px
    // à direita. Procura de -12 a +40 px, o deslocamento mais perto de 0 primeiro.
    let [x, y, w, h] = HELPER_ICON;
    let img = capture::capture(hwnd, Some([x - 12, y, w + 52, h]))?;
    let big = binarize_white(&img.bgra, img.w as usize, img.h as usize, HELPER_WHITE);
    let mut offs: Vec<i32> = (-12..=40).collect();
    offs.sort_by_key(|d| d.abs());
    for d in offs {
        let ox = (d + 12) as usize;
        let mut b = Bits::new(w as usize, h as usize);
        for yy in 0..h as usize {
            for xx in 0..w as usize {
                b.px[yy * w as usize + xx] = big.px[yy * big.w + ox + xx];
            }
        }
        if let Some(on) = helper_on(&b) {
            return Some(on);
        }
    }
    None
}

/// Inventário aberto? (o campo do Zen só aparece com o inventário aberto)
pub fn inventory_open(hwnd: isize) -> bool {
    capture::capture(hwnd, Some([786, 609, 78, 14])).is_some_and(|img| {
        let b = zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
        zen_core::glyphs::read_number(&b).is_some()
    })
}

/// OK dos avisos do jogo no meio da tela ("Helper only runs in field", "close Inventory").
pub(in crate::win::nav) const DIALOG_OK: (i32, i32) = (512, 316);

/// Liga/desliga o helper só se precisar, e confere pelo ícone. Nunca aperta Home às cegas.
/// Para LIGAR: o jogo recusa com o inventário aberto (visto em 08/11: "In order to run The
/// Classic Helper, please close Inventory window"), então fecha o inventário antes; se ainda
/// assim não ligar, fecha o aviso (OK) e tenta mais uma vez.
pub fn set_helper(hwnd: isize, on: bool) -> Result<(), String> {
    let _lease = input::reserve(hwnd)?;
    let initial = helper_state(hwnd);
    tracing::info!(hwnd, solicitado = on, observado = ?initial, "HELPER: solicitação de estado");
    match initial {
        None => Err("não reconheci o ícone do helper".into()),
        Some(s) if s == on => Ok(()),
        Some(_) => {
            for attempt in 0..2 {
                // Confere antes de cada Home: Home alterna, nunca apertar se já está como pedido.
                match helper_state(hwnd) {
                    Some(s) if s == on => return Ok(()),
                    None => return Err("não reconheci o ícone do helper".into()),
                    _ => {}
                }
                if on && inventory_open(hwnd) {
                    input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
                    sleep(700);
                }
                // Janelas da Fortuna (K/J) abertas também podem segurar o helper.
                if on && bank_open(hwnd) {
                    input::run(hwnd, &[Step::Key(VK_J)])?;
                    sleep(600);
                }
                if on && gremory_open(hwnd) {
                    input::run(hwnd, &[Step::Click { x: GREMORY_X.0, y: GREMORY_X.1 }])?;
                    sleep(600);
                }
                // Limpar janelas pode levar tempo. Releia imediatamente antes de alternar:
                // o Helper pode ter ligado/desligado nesse intervalo.
                match helper_state(hwnd) {
                    Some(s) if s == on => return Ok(()),
                    None => return Err("não reconheci o ícone do helper antes do Home".into()),
                    _ => {}
                }
                tracing::info!(hwnd, solicitado = on, tentativa = attempt + 1, "HELPER: alternando com Home");
                input::run(hwnd, &[Step::Key(VK_HOME)])?;
                sleep(1300);
                let observed = helper_state(hwnd);
                tracing::info!(hwnd, solicitado = on, observado = ?observed, "HELPER: resultado após Home");
                match observed {
                    Some(s) if s == on => return Ok(()),
                    _ if attempt == 0 && on => {
                        // provável aviso na tela: OK e tenta de novo
                        input::run(hwnd, &[Step::Click { x: DIALOG_OK.0, y: DIALOG_OK.1 }])?;
                        sleep(800);
                    }
                    other => {
                        let e = format!("o helper não ficou {} (ícone {other:?})", if on { "ligado" } else { "desligado" });
                        save_error_shot(hwnd, &e);
                        return Err(e);
                    }
                }
            }
            Err("o helper não ligou depois de fechar o inventário e o aviso".into())
        }
    }
}

pub(in crate::win::nav) fn settle(hwnd: isize) -> Option<(i32, i32)> {
    let mut last = None;
    for _ in 0..25 {
        let s = position(hwnd);
        if s.is_some() && s == last {
            return s;
        }
        last = s;
        sleep(250);
    }
    last
}

pub(in crate::win::nav) fn near(hwnd: isize, prev: (i32, i32)) -> Option<(i32, i32)> {
    for _ in 0..4 {
        let s = settle(hwnd)?;
        if cheb(s, prev) <= 10 {
            return Some(s);
        }
        sleep(300);
    }
    None
}

/// M -> botão Kanturu. Espera o nome virar Kanturu e a posição saltar.
/// Texto "Kanturu" no mapa (M) aberto: igual no cliente em inglês e em português (prints de
/// 08/11 e 09/11). Fechado, a mesma região difere em 77+ pixels.
pub(in crate::win::nav) const M_LABEL: [i32; 4] = [592, 318, 40, 10];
pub(in crate::win::nav) const M_LABEL_TEXT: &str = "\
........................................
........................................
..#...#..............#..................
..#..#...............#..................
..#.#.....###..###..###.#..#..##.#..#...
..#.#.......#..#..#..#..#..#..#..#..#...
..##.#....###..#..#..#..#..#..#..#..#...
..#...#..#..#..#..#..#..#..#..#..#..#...
..#....#.####..#..#..##..###..#...###...
........................................";

/// O mapa (M) está aberto na tela?
pub fn map_open(hwnd: isize) -> bool {
    map_shift(hwnd).is_some()
}

/// Deslocamento do mapa (M) em relação ao medido no PC1: procura o texto "Kanturu" numa área
/// maior (outro layout de cliente, ex. DESKTOP-KIS9DG7). `None` = mapa fechado.
pub(in crate::win::nav) fn map_shift(hwnd: isize) -> Option<(i32, i32)> {
    let [x, y, w, h] = M_LABEL;
    let (mx, my) = (60, 40);
    let img = capture::capture(hwnd, Some([x - mx, y - my, w + 2 * mx, h + 2 * my]))?;
    let b = binarize_white(&img.bgra, img.w as usize, img.h as usize, COORDS_WHITE);
    let t = Bits::from_text(M_LABEL_TEXT);
    let mut best: Option<(usize, i32, i32)> = None;
    for oy in 0..=(b.h - t.h) {
        for ox in 0..=(b.w - t.w) {
            let mut d = 0;
            for yy in 0..t.h {
                for xx in 0..t.w {
                    if b.px[(oy + yy) * b.w + ox + xx] != t.px[yy * t.w + xx] {
                        d += 1;
                    }
                }
            }
            let (dx, dy) = (ox as i32 - mx, oy as i32 - my);
            if d <= 10 && best.map_or(true, |bb| (d, dx.abs() + dy.abs()) < (bb.0, bb.1.abs() + bb.2.abs())) {
                best = Some((d, dx, dy));
            }
        }
    }
    best.map(|(_, dx, dy)| (dx, dy))
}
