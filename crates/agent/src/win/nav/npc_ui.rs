use super::*;

/// O ponto `q` da tela cai no Safety Guardian (corpo + baú)? 60 px de raio não bastou: o
/// ReDMooN (21:19) abriu o baú 12 vezes em 25 s parado em (33,217). A área clicável é alta:
/// elipse de 80 x 115 px centrada 45 px acima do pé do NPC.
pub(super) fn on_npc(pos: (i32, i32), q: (i32, i32)) -> bool {
    let inside = |n: (i32, i32)| {
        let (dx, dy) = ((q.0 - n.0) as f64 / 80.0, (q.1 - (n.1 - 45)) as f64 / 115.0);
        dx * dx + dy * dy < 1.0
    };
    let world = CUR_WORLD.load(std::sync::atomic::Ordering::SeqCst);
    if world == KANTURU_WORLD && cheb(pos, VAULT_TILE) <= 10 && inside(npc_on_screen(pos)) {
        return true;
    }
    if let Ok(g) = LIVE_NPCS.lock() {
        if g.iter().any(|t| cheb(pos, *t) <= 10 && inside(npc_px(pos, (t.0 as f64 + 0.5, t.1 as f64 + 0.5)))) {
            return true;
        }
    }
    learned_npcs(world).into_iter().any(|t| cheb(pos, t) <= 10 && inside(npc_px(pos, (t.0 as f64, t.1 as f64))))
}

// ---------------------------------------------------------------- janela de NPC aberta (11/11)
// Toda janela de NPC (Trainer, Chaos Goblin, ...) tem a mesma moldura de cima em x 333..690,
// y 122..133: igual pixel a pixel entre as janelas e bem diferente com tudo fechado (medido nos
// prints de 11/11). O YouSee abriu o Chaos Goblin andando até o Trainer e ficou preso.
pub(super) const NPC_TOP: &[u8; 358 * 12] = include_bytes!("../npc_dialog_top.gray");

/// "OK" das caixas de aviso de NPC (ex.: Shadow Phantom Soldier em Devias, 11/11 22:37: "You can
/// now stand alone without my support." travou o x9901: nem andava nem abria o M). Texto branco
/// (menor canal >= 190) achado em (499,307) no print; a caixa pode mudar de altura.
pub(super) const MSG_OK: &str = "\
................................
................................
.......###......................
.........##.......###.....##....
...##.....####....###.....#.....
..##.......###....###.....#.....
..##........##....###...........
..##........##....###...........
..##........##....###..##.......
..##........##....###.###.......
..##........##....########......
..##........##....####..###.....
..##........##....###...###.....
..##........##....###....###....
..##.......###....###.....###...
..###......##.....###.....###...
...###............###......###..
.....#####........##........##..
................................
................................";

/// Caixa de aviso com botão OK aberta? Devolve o centro do OK.
pub(super) fn msgbox_ok(hwnd: isize) -> Option<(i32, i32)> {
    let t = Bits::from_text(MSG_OK);
    let (x0, y0, w, h) = (485i32, 230i32, 62i32, 210i32);
    let img = capture::capture(hwnd, Some([x0, y0, w, h]))?;
    let b = binarize_white(&img.bgra, img.w as usize, img.h as usize, 190);
    for oy in 0..=(b.h.saturating_sub(t.h)) {
        for ox in 0..=(b.w.saturating_sub(t.w)) {
            let mut d = 0;
            'm: for yy in 0..t.h {
                for xx in 0..t.w {
                    if b.px[(oy + yy) * b.w + ox + xx] != t.px[yy * t.w + xx] {
                        d += 1;
                        if d > 14 {
                            break 'm;
                        }
                    }
                }
            }
            if d <= 14 {
                return Some((x0 + ox as i32 + t.w as i32 / 2, y0 + oy as i32 + t.h as i32 / 2));
            }
        }
    }
    None
}

/// Alguma janela de NPC (ou caixa de aviso com OK) aberta no meio da tela?
pub fn npc_dialog_open(hwnd: isize) -> bool {
    if msgbox_ok(hwnd).is_some() {
        return true;
    }
    npc_frame_open(hwnd) || side_npc_close(hwnd).is_some() || chaos_shop_open(hwnd)
}

pub(super) const SIDE_CLOSE: &[u8;1600] = include_bytes!("../npc_side_close.gray");
pub(super) const CHAOS_TITLE: &[u8;1984] = include_bytes!("../npc_chaos_title.gray");

pub(super) fn gray_template_at(img: &capture::Image, template: &[u8], w: usize, h: usize) -> Option<(i32,i32)> {
    if (img.w as usize)<w || (img.h as usize)<h { return None; }
    let mut best=None;
    for y in 0..=img.h as usize-h { for x in 0..=img.w as usize-w {
        let mut error=0u32;
        for ty in 0..h { for tx in 0..w {
            let i=((y+ty)*img.w as usize+x+tx)*4;
            let g=(img.bgra[i+2] as u32*299+img.bgra[i+1] as u32*587+img.bgra[i] as u32*114)/1000;
            error+=g.abs_diff(template[ty*w+tx] as u32);
        }}
        if error/(w*h) as u32<=12 && best.map_or(true,|(e,_,_)|error<e) { best=Some((error,x as i32,y as i32)); }
    }}
    best.map(|(_,x,y)|(x,y))
}
pub(super) fn side_npc_caption(img: &capture::Image) -> bool {
    img.bgra.chunks_exact(4).filter(|p|p[1]>130 && (p[2] as u32)*10<(p[1] as u32)*7 && p[0]>80).count()>=80
}
pub(super) fn side_npc_close(hwnd: isize) -> Option<(i32,i32)> {
    // Título de NPC em ciano + botão X. Inventário tem o mesmo X, mas não esse título.
    let header=capture::capture(hwnd,Some([750,35,240,25]))?;
    if !side_npc_caption(&header) { return None; }
    let close=capture::capture(hwnd,Some([735,640,120,80]))?;
    let (x,y)=gray_template_at(&close,SIDE_CLOSE,40,40)?;
    Some((735+x+20,640+y+20))
}
pub(super) fn chaos_shop_open(hwnd: isize) -> bool {
    capture::capture(hwnd,Some([205,41,134,26])).is_some_and(|img|gray_template_at(&img,CHAOS_TITLE,124,16).is_some())
}

pub(super) fn npc_frame_open(hwnd: isize) -> bool {
    let (w, h) = (358usize, 12usize);
    let s = 3i32;
    let Some(img) = capture::capture(hwnd, Some([333 - s, 122 - s, w as i32 + 2 * s, h as i32 + 2 * s])) else { return false };
    let iw = img.w as usize;
    let gray = |x: usize, y: usize| {
        let i = (y * iw + x) * 4;
        (u32::from(img.bgra[i + 2]) * 299 + u32::from(img.bgra[i + 1]) * 587 + u32::from(img.bgra[i]) * 114) / 1000
    };
    for oy in 0..=(2 * s) as usize {
        for ox in 0..=(2 * s) as usize {
            let mut d = 0u32;
            for y in 0..h {
                for x in 0..w {
                    d += gray(ox + x, oy + y).abs_diff(u32::from(NPC_TOP[y * w + x]));
                }
            }
            if d / (w * h) as u32 <= 15 {
                return true;
            }
        }
    }
    false
}

/// Fecha a janela de NPC (ESC) se houver uma. Devolve se fechou.
pub(super) fn close_npc_dialog(hwnd: isize) -> bool {
    let mut handled=false;
    for _ in 0..3 {
        let step=if let Some((x,y))=msgbox_ok(hwnd).or_else(||side_npc_close(hwnd)) {
            Step::Click{x,y}
        } else if npc_frame_open(hwnd) || chaos_shop_open(hwnd) { Step::Key(0x1B) }
        else { return handled; };
        if input::run(hwnd,&[step]).is_err(){return false;}
        handled=true;
        sleep(400);
    }
    handled && !npc_dialog_open(hwnd)
}
