use super::*;

// ---------------------------------------------------------------- Elpis: Gemstone -> Jewel of Harmony (16/11)
// MaMaKi (PC1) na Refinery Tower, ao lado da Elpis. Visto no jogo em 16/11 20:20:
// - Clique na Elpis -> diálogo (About refinery / Jewel of Harmony / Refine Gemstone / Close).
// - "Refine Gemstone" -> painel Refine + inventário. Botão direito numa Gemstone AVULSA a manda
//   para a caixa ("Refine Gemstone" amarelo; taxa mostrada 15%). Refine -> "OK" -> chat diz
//   "Refine has failed." quando falha. Depois de um refino o painel fica vazio: precisa fechar
//   (X do inventário) e abrir de novo pela Elpis.
// - Separar 1 da pilha: R (repair) com SÓ o inventário aberto e clique na pilha; o repair fica
//   ativo e cada clique separa mais uma. Com o painel do Refine aberto o R não vale e o clique
//   PEGA a pilha inteira (16/11 20:23): por isso só separa com o painel fechado.
// - O bot não anda: o dono leva o char até a Elpis.
pub(super) const ELPIS_DLG: &[u8; 90 * 16] = include_bytes!("../elpis_dlg.gray");
pub(super) const ELPIS_DLG_AT: (i32, i32) = (468, 465);
pub(super) const ELPIS_READY: &[u8; 86 * 14] = include_bytes!("../elpis_ready.gray");
pub(super) const ELPIS_READY_AT: (i32, i32) = (138, 103);
pub(super) const ELPIS_BTN: &[u8; 54 * 44] = include_bytes!("../elpis_btn.gray");
pub(super) const ELPIS_BTN_AT: (i32, i32) = (238, 638);
pub(super) const ELPIS_CONFIRM: &[u8; 50 * 21] = include_bytes!("../elpis_confirm.gray");
pub(super) const ELPIS_CONFIRM_AT: (i32, i32) = (396, 330);
#[cfg(test)]
pub(super) const ELPIS_FAILED: &[u8; 78 * 13] = include_bytes!("../elpis_failed.gray");
/// X do meio da tela do Refine: aparece com o painel ativo ou vazio (depois de um refino).
pub(super) const ELPIS_PANEL_X: &[u8; 40 * 38] = include_bytes!("../elpis_panel_x.gray");
pub(super) const ELPIS_PANEL_X_AT: (i32, i32) = (458, 656);
pub(super) const GEM_SINGLE: &[u8; 32 * 32] = include_bytes!("../gem_single.gray");
/// Caixa do Refine vazia (8x4 casas de 32 px a partir de (140,203)).
pub(super) const ELPIS_BOX_EMPTY: &[u8; 256 * 128] = include_bytes!("../elpis_box_empty.gray");
pub(super) const ELPIS_BOX: [i32; 4] = [140, 203, 256, 128];
pub(super) const ELPIS_REFINE_OPTION: (i32, i32) = (513, 472);
pub(super) const ELPIS_DIALOG_CLOSE: (i32, i32) = (513, 542);
pub(super) const ELPIS_REFINE: (i32, i32) = (265, 659);
pub(super) const ELPIS_OK: (i32, i32) = (421, 340);
pub(super) const INV_CLOSE: (i32, i32) = (782, 675);
pub(super) const INV_GRID: [i32; 4] = [748, 347, 260, 260];
pub(super) const VK_R: u16 = 0x52;

pub(super) fn fixed_at(hwnd: isize, t: &[u8], w: usize, h: usize, at: (i32, i32)) -> bool {
    let m = 4;
    capture::capture(hwnd, Some([at.0 - m, at.1 - m, w as i32 + 2 * m, h as i32 + 2 * m]))
        .is_some_and(|img| gray_match(&img, t, w, h, 8).is_some())
}
/// Casamento só pelo desenho do texto/ícone: os pixels claros do modelo (>= 120) têm de bater
/// (erro médio <= 30) e quase nenhum pixel escuro do modelo (< 50) pode estar claro (> 110).
/// Serve para janelas semitransparentes do jogo, em que o cenário atrás muda o fundo.
#[allow(dead_code)]
pub(super) fn mask_match(img: &capture::Image, t: &[u8], w: usize, h: usize) -> bool {
    let (iw, ih) = (img.w as usize, img.h as usize);
    if iw < w || ih < h { return false; }
    let lit = t.iter().filter(|&&v| v >= 120).count();
    let dark = t.iter().filter(|&&v| v < 50).count();
    if lit < 40 || dark < 40 { return false; }
    let gray: Vec<u8> = img.bgra.chunks_exact(4)
        .map(|p| ((p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000) as u8).collect();
    for y in 0..=ih - h {
        for x in 0..=iw - w {
            let (mut err, mut bright) = (0u32, 0usize);
            for ty in 0..h {
                for tx in 0..w {
                    let (g, v) = (gray[(y + ty) * iw + x + tx], t[ty * w + tx]);
                    if v >= 120 { err += (g as u32).abs_diff(v as u32); }
                    else if v < 50 && g > 110 { bright += 1; }
                }
            }
            if err <= 30 * lit as u32 && bright * 10 <= dark { return true; }
        }
    }
    false
}
pub(super) fn wait_fixed(hwnd: isize, t: &[u8], w: usize, h: usize, at: (i32, i32), ms: u128) -> bool {
    let t0 = std::time::Instant::now();
    loop {
        if fixed_at(hwnd, t, w, h, at) { return true; }
        if t0.elapsed().as_millis() >= ms { return false; }
        sleep(200);
    }
}
/// Na Refinery Tower a câmera não centraliza o char como nos outros mapas: medido em 16/11 com a
/// Elpis em (78,176) e o char em (75,176) (corpo em ~(535,420), o clique abriu) e em (81,176)
/// (corpo em ~(220,250)): char em ~(377,340) e o modelo dos outros mapas encolhido a 0,82.
pub(super) fn elpis_click_px(at: (i32, i32), npc: (i32, i32)) -> (i32, i32) {
    let (x, y) = npc_px(at, (npc.0 as f64, npc.1 as f64));
    (377 + ((x - CENTER.0) as f64 * 0.82).round() as i32, 340 + ((y - CENTER.1) as f64 * 0.82).round() as i32)
}
pub(super) fn elpis_dialog(hwnd: isize) -> bool { fixed_at(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT) }
/// Casas ocupadas da caixa do Refine (recorte de `ELPIS_BOX`): centro na janela.
pub(super) fn box_items_in(img: &capture::Image) -> Vec<(i32, i32)> {
    let w = img.w as usize;
    let mut out = Vec::new();
    for r in 0..4usize {
        for c in 0..8usize {
            let (mut e, mut n) = (0u32, 0u32);
            for y in r * 32 + 4..r * 32 + 28 {
                for x in c * 32 + 4..c * 32 + 28 {
                    let i = (y * w + x) * 4;
                    let Some(p) = img.bgra.get(i..i + 3) else { continue };
                    let g = (p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000;
                    e += g.abs_diff(ELPIS_BOX_EMPTY[y * 256 + x] as u32);
                    n += 1;
                }
            }
            if n > 0 && e / n > 12 {
                out.push((ELPIS_BOX[0] + c as i32 * 32 + 16, ELPIS_BOX[1] + r as i32 * 32 + 16));
            }
        }
    }
    out
}
pub(super) fn box_items(hwnd: isize) -> Option<Vec<(i32, i32)>> { capture::capture(hwnd, Some(ELPIS_BOX)).map(|img| box_items_in(&img)) }

/// Gemstones no inventário (recorte de `INV_GRID`): (centro na janela, avulsa?). Pilha e avulsa
/// têm o mesmo desenho embaixo; a pilha tem o número em cima.
pub(super) fn gems_in(img: &capture::Image) -> Vec<((i32, i32), bool)> {
    let w = img.w as usize;
    let g = |x: usize, y: usize| -> u32 {
        let i = (y * w + x) * 4;
        img.bgra.get(i..i + 3).map_or(0, |p| (p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000)
    };
    let diff = |cx: usize, cy: usize, ys: std::ops::Range<usize>, xs: std::ops::Range<usize>| -> u32 {
        let (mut e, mut n) = (0u32, 0u32);
        for y in ys.clone() { for x in xs.clone() {
            e += g(cx + x, cy + y).abs_diff(GEM_SINGLE[y * 32 + x] as u32);
            n += 1;
        }}
        e / n.max(1)
    };
    let mut out = Vec::new();
    for r in 0..8 {
        for c in 0..8 {
            let (cx, cy) = ((c as f64 * 32.1).round() as usize, (r as f64 * 32.1).round() as usize);
            if cx + 32 > w || cy + 32 > img.h as usize { continue; }
            if diff(cx, cy, 13..30, 3..29) > 8 { continue; }
            let single = diff(cx, cy, 1..12, 8..30) <= 10;
            out.push(((INV_GRID[0] + cx as i32 + 16, INV_GRID[1] + cy as i32 + 16), single));
        }
    }
    out
}
pub(super) fn gems(hwnd: isize) -> Vec<((i32, i32), bool)> {
    capture::capture(hwnd, Some(INV_GRID)).map(|img| gems_in(&img)).unwrap_or_default()
}

/// Fecha diálogo da Elpis / painel do Refine / inventário, conferindo na tela.
pub(super) fn elpis_close_all(hwnd: isize) -> Result<(), String> {
    for _ in 0..3 {
        if elpis_dialog(hwnd) {
            input::run(hwnd, &[Step::Click { x: ELPIS_DIALOG_CLOSE.0, y: ELPIS_DIALOG_CLOSE.1 }])?;
            sleep(700);
            continue;
        }
        if inventory_open(hwnd) || fixed_at(hwnd, ELPIS_PANEL_X, 40, 38, ELPIS_PANEL_X_AT) {
            input::run(hwnd, &[Step::Click { x: INV_CLOSE.0, y: INV_CLOSE.1 }])?;
            sleep(800);
            continue;
        }
        return Ok(());
    }
    Err("Elpis: não consegui fechar as janelas".into())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ElpisReport { pub refined: u32, pub failed: u32, pub ok: u32, pub gems_left: u32 }

/// Refina até `max` Gemstones na Elpis (uma por vez). Para no primeiro passo que não confere na
/// tela, sem cliques às cegas. As Harmonies ficam no inventário.
pub fn elpis_refine(hwnd: isize, char_name: &str, max: u32, status: &dyn Fn(String)) -> Result<ElpisReport, String> {
    let _session = input::Session::begin(hwnd)?;
    let mut rep = ElpisReport::default();
    let mut lift_ok: Option<i32> = None;
    while rep.refined < max {
        // O painel do Refine cobre as coordenadas do canto: fecha antes de ler a posição.
        elpis_close_all(hwnd)?;
        let pos = position(hwnd).ok_or("Elpis: não li a posição")?;
        let elpis = crate::win::objects::nearby_hwnd(hwnd, char_name, pos)
            .and_then(|v| v.into_iter().find(|o| o.kind == zen_core::objects::Kind::Npc && o.name.eq_ignore_ascii_case("Elpis")))
            .map(|o| (o.x, o.y))
            .ok_or("Elpis não encontrada na memória perto do char (leve o char até ela)")?;
        if cheb(pos, elpis) > 5 {
            return Err(format!("char em {pos:?}, longe da Elpis em {elpis:?} (leve o char até ela)"));
        }
        // 1) Inventário só: separa Gemstones se não houver avulsa.
        if !inventory_open(hwnd) {
            input::run(hwnd, &[Step::Key(zen_core::actions::VK_V)])?;
            sleep(900);
        }
        if !inventory_open(hwnd) { return Err("Elpis: o inventário não abriu".into()); }
        let mut list = gems(hwnd);
        if !list.iter().any(|g| g.1) {
            let Some(stack) = list.iter().find(|g| !g.1).map(|g| g.0) else {
                rep.gems_left = 0;
                break;
            };
            let before = list.len();
            let free = 64usize.saturating_sub(inventory_filled(hwnd).unwrap_or(64) as usize);
            let n = (max - rep.refined).min(free.saturating_sub(2) as u32).min(30);
            if n == 0 { return Err("Elpis: inventário sem espaço para separar Gemstones".into()); }
            status(format!("Elpis: separando {n} Gemstone(s) (R + clique na pilha)"));
            input::run(hwnd, &[Step::Key(VK_R)])?;
            sleep(400);
            for k in 0..n {
                input::run(hwnd, &[Step::Click { x: stack.0, y: stack.1 }])?;
                sleep(if k == 0 { 700 } else { 350 });
                if k == 0 {
                    let now = gems(hwnd);
                    if !now.iter().any(|g| g.0 == stack) {
                        // A pilha foi para o cursor (repair não ativou): devolve ao mesmo lugar.
                        input::run(hwnd, &[Step::Click { x: stack.0, y: stack.1 }])?;
                        sleep(700);
                        save_error_shot(hwnd, "Elpis: pilha pegou em vez de separar");
                        return Err("Elpis: o R não ativou o repair; a pilha foi devolvida ao lugar".into());
                    }
                    if now.len() <= before {
                        save_error_shot(hwnd, "Elpis: não separou");
                        return Err("Elpis: o clique na pilha não separou nenhuma Gemstone".into());
                    }
                }
            }
            list = gems(hwnd);
        }
        let Some(single) = list.iter().find(|g| g.1).map(|g| g.0) else {
            return Err("Elpis: não achei Gemstone avulsa depois de separar".into());
        };
        rep.gems_left = list.len() as u32;
        // 2) Abre a Elpis (clique no corpo; tenta 3 alturas e lembra a que funcionou). Se o
        // char andar até ela, o diálogo abre depois (21:30: abriu com 3 s): espera parar e
        // relê antes de clicar de novo, nunca clica com o diálogo já aberto.
        let mut opened = false;
        let mut at = pos;
        let mut moves = 0;
        let mut panel = false;
        for lift in lift_ok.into_iter().chain([0, 25, -25]) {
            if fixed_at(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT) { opened = true; panel = true; break; }
            if elpis_dialog(hwnd) { opened = true; break; }
            let (x, y) = elpis_click_px(at, elpis);
            input::run(hwnd, &[Step::Click { x, y: y - lift }])?;
            if wait_fixed(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT, 3000) { opened = true; lift_ok = Some(lift); break; }
            let now = npc_still(hwnd).unwrap_or(at);
            if wait_fixed(hwnd, ELPIS_DLG, 90, 16, ELPIS_DLG_AT, 1500) { opened = true; lift_ok = Some(lift); break; }
            if now != at {
                moves += 1;
                if moves > 2 || cheb(now, elpis) > 5 {
                    save_error_shot(hwnd, "Elpis: clique moveu o char");
                    return Err(format!("Elpis: o char andou para {now:?} sem abrir o diálogo; parei"));
                }
                at = now;
            }
        }
        if !opened {
            save_error_shot(hwnd, "Elpis: diálogo não abriu");
            return Err("Elpis: o diálogo não abriu (3 cliques)".into());
        }
        if !panel { input::run(hwnd, &[Step::Click { x: ELPIS_REFINE_OPTION.0, y: ELPIS_REFINE_OPTION.1 }])?; }
        if !wait_fixed(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT, 3000) {
            save_error_shot(hwnd, "Elpis: painel do Refine não abriu");
            return Err("Elpis: o painel do Refine não abriu".into());
        }
        // 3) Botão direito na avulsa -> caixa; confere "Refine Gemstone" amarelo.
        input::run(hwnd, &[Step::RightClick { x: single.0, y: single.1 }])?;
        if !wait_fixed(hwnd, ELPIS_READY, 86, 14, ELPIS_READY_AT, 2500) {
            save_error_shot(hwnd, "Elpis: Gemstone não entrou na caixa");
            return Err("Elpis: a Gemstone não entrou na caixa do Refine".into());
        }
        // 4) Refine -> OK.
        input::run(hwnd, &[Step::Click { x: ELPIS_REFINE.0, y: ELPIS_REFINE.1 }])?;
        if !wait_fixed(hwnd, ELPIS_CONFIRM, 50, 21, ELPIS_CONFIRM_AT, 2500) {
            save_error_shot(hwnd, "Elpis: sem confirmação");
            return Err("Elpis: a confirmação (OK) não apareceu".into());
        }
        input::run(hwnd, &[Step::Click { x: ELPIS_OK.0, y: ELPIS_OK.1 }])?;
        // O painel perde o botão quando o refino termina.
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_millis() < 3000 && fixed_at(hwnd, ELPIS_BTN, 54, 44, ELPIS_BTN_AT) { sleep(150); }
        sleep(500);
        rep.refined += 1;
        // Deu certo: a Harmony fica na caixa (dono, 16/11 20:38). Botão direito tira para o
        // inventário; confere que a caixa esvaziou antes de seguir.
        // Quem decide é a caixa: vazia = falhou (a linha "Refine has failed." do chat sobe quando
        // chega outra mensagem e não serve de prova; 16/11 21:18).
        let left = box_items(hwnd).unwrap_or_default();
        if left.is_empty() {
            rep.failed += 1;
        } else {
            rep.ok += 1;
            if rep.ok <= 2 { save_error_shot(hwnd, "Elpis: Harmony na caixa"); }
            for at in &left {
                input::run(hwnd, &[Step::RightClick { x: at.0, y: at.1 }])?;
                sleep(600);
            }
            let still = box_items(hwnd).unwrap_or_default();
            if !still.is_empty() {
                save_error_shot(hwnd, "Elpis: Harmony não saiu da caixa");
                return Err(format!("Elpis: {} item(ns) ficaram na caixa do Refine (tire à mão); parei para não perder a Harmony", still.len()));
            }
            status(format!("Elpis: Harmony! ({} até agora) tirada da caixa para o inventário", rep.ok));
        }
        rep.gems_left = rep.gems_left.saturating_sub(1);
        status(format!("Elpis: {} refino(s), {} Harmony(s), {} falha(s)", rep.refined, rep.ok, rep.failed));
    }
    elpis_close_all(hwnd)?;
    Ok(rep)
}

#[cfg(test)]
mod elpis_tests {
    use super::*;
    fn img(bytes: &[u8]) -> capture::Image {
        let mut d = png::Decoder::new(bytes);
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = d.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        let ch = match info.color_type { png::ColorType::Rgba => 4, png::ColorType::Rgb => 3, _ => panic!() };
        let bgra = buf[..info.buffer_size()].chunks_exact(ch).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
        capture::Image { w: info.width, h: info.height, bgra }
    }
    #[test]
    fn acha_pilhas_e_avulsas() {
        let a = gems_in(&img(include_bytes!("../../../tests/fixtures/elpis/inventario.png")));
        assert_eq!(a, vec![((764, 363), false), ((796, 363), false), ((828, 363), true)]);
        let b = gems_in(&img(include_bytes!("../../../tests/fixtures/elpis/inventario2.png")));
        assert_eq!(b, vec![((764, 363), false), ((796, 363), false), ((860, 363), true)]);
    }
    #[test]
    fn clique_na_elpis_bate_com_o_medido_no_jogo() {
        let a = elpis_click_px((75, 176), (78, 176));
        assert!((a.0 - 535).abs() <= 15 && (a.1 - 425).abs() <= 20, "{a:?}");
        let b = elpis_click_px((81, 176), (78, 176));
        assert!((b.0 - 220).abs() <= 15 && (b.1 - 250).abs() <= 20, "{b:?}");
    }
    #[test]
    fn acha_item_na_caixa_do_refine() {
        assert_eq!(box_items_in(&img(include_bytes!("../../../tests/fixtures/elpis/caixa_com.png"))), vec![(156, 219)]);
        assert!(box_items_in(&img(include_bytes!("../../../tests/fixtures/elpis/caixa_vazia.png"))).is_empty());
    }
    #[test]
    fn reconhece_telas_da_elpis() {
        let m = |b: &[u8], t: &[u8], w, h| gray_match(&img(b), t, w, h, 8).is_some();
        assert!(m(include_bytes!("../../../tests/fixtures/elpis/dlg.png"), ELPIS_DLG, 90, 16));
        // Diálogo semitransparente com cenário claro atrás (21:50): só o desenho do texto bate.
        assert!(mask_match(&img(include_bytes!("../../../tests/fixtures/elpis/dlg_cenario.png")), ELPIS_DLG, 90, 16));
        assert!(!mask_match(&img(include_bytes!("../../../tests/fixtures/elpis/dlg_sem.png")), ELPIS_DLG, 90, 16));
        assert!(m(include_bytes!("../../../tests/fixtures/elpis/ready.png"), ELPIS_READY, 86, 14));
        assert!(!m(include_bytes!("../../../tests/fixtures/elpis/notready.png"), ELPIS_READY, 86, 14));
        assert!(m(include_bytes!("../../../tests/fixtures/elpis/failed.png"), ELPIS_FAILED, 78, 13));
        assert!(!m(include_bytes!("../../../tests/fixtures/elpis/notfailed.png"), ELPIS_FAILED, 78, 13));
    }
}

// ---------------------------------------------------------------- Dark Raven (Trainer de Lorencia)
// Conferido no jogo em 11/11 com o x9901: o NPC "Trainer" (classe 226) está em (122,110) na
// lista de personagens da memória (igual ao guia oficial). Clicando nele abre a janela
// "Trainer" com "Restore life (durability)" e "Resurrect spirit"; o primeiro abre "Select the
// pet to recover life" com os botões "Dark horse" e "Dark Raven". A vida do Raven é a barra do
// topo da tela (zen_core::petbar), lida antes e depois para confirmar.
