use crate::win::nav::*;

/// Texto "If move to SafeZone it will / be necessary to reactivate!" (150x42 em (160,208)).
pub(in crate::win::nav) const PICK_OPEN: &[u8; 150 * 42] = include_bytes!("../../modelos/pick_open.gray");
/// Texto "Zen" do botão (66x16; no print em (200,544), centro do botão em (233,551)).
pub(in crate::win::nav) const PICK_ZEN: &[u8; 66 * 16] = include_bytes!("../../modelos/pick_zen.gray");
/// A janela NÃO fica parada: com o inventário (V) aberto ela vai para o meio da tela (texto em
/// (404,189) em vez de (160,208); x9901/ReDMooN 16/11 16:59-17:51, 6 falhas). Procura o texto
/// numa faixa larga e calcula a lista e a roda a partir dele.
pub(in crate::win::nav) const PICK_SEARCH_AREA: [i32; 4] = [100, 180, 600, 72];
/// Lista de botões relativa ao texto: (-10, +52), 170x335.
pub(in crate::win::nav) const PICK_LIST_FROM_TEXT: (i32, i32, i32, i32) = (-10, 52, 170, 335);
/// Roda do mouse relativa ao texto: meio da lista (fora da janela a roda mexe o zoom da câmera).
pub(in crate::win::nav) const PICK_WHEEL_FROM_TEXT: (i32, i32) = (73, 212);
pub(in crate::win::nav) const VK_Y: u16 = 0x59;

/// Melhor posição do modelo cinza dentro de `img` com erro médio <= `max_err` (0..255).
pub(in crate::win::nav) fn gray_match(img: &capture::Image, template: &[u8], w: usize, h: usize, max_err: u32) -> Option<(i32, i32)> {
    let (iw, ih) = (img.w as usize, img.h as usize);
    if iw < w || ih < h { return None; }
    let gray: Vec<u8> = img.bgra.chunks_exact(4)
        .map(|p| ((p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000) as u8).collect();
    let limit = max_err * (w * h) as u32;
    let mut best: Option<(u32, i32, i32)> = None;
    for y in 0..=ih - h {
        'x: for x in 0..=iw - w {
            let mut err = 0u32;
            for ty in 0..h {
                let row = (y + ty) * iw + x;
                for tx in 0..w {
                    err += (gray[row + tx] as u32).abs_diff(template[ty * w + tx] as u32);
                }
                if err > limit { continue 'x; }
            }
            if best.map_or(true, |(e, _, _)| err < e) { best = Some((err, x as i32, y as i32)); }
        }
    }
    best.map(|(_, x, y)| (x, y))
}
/// Origem do texto da Pick Window (coordenadas da janela do jogo), a partir do recorte de
/// `PICK_SEARCH_AREA`.
pub(in crate::win::nav) fn pick_text_in(img: &capture::Image) -> Option<(i32, i32)> {
    let (x, y) = gray_match(img, PICK_OPEN, 150, 42, 8)?;
    Some((PICK_SEARCH_AREA[0] + x, PICK_SEARCH_AREA[1] + y))
}
pub(in crate::win::nav) fn pick_list_area(text: (i32, i32)) -> [i32; 4] {
    let (dx, dy, w, h) = PICK_LIST_FROM_TEXT;
    [text.0 + dx, text.1 + dy, w, h]
}
/// Centro do botão Zen em coordenadas da janela, a partir do recorte da lista em `area`.
pub(in crate::win::nav) fn pick_zen_in(img: &capture::Image, area: [i32; 4]) -> Option<(i32, i32)> {
    let (x, y) = gray_match(img, PICK_ZEN, 66, 16, 6)?;
    Some((area[0] + x + 33, area[1] + y + 7))
}
/// Onde está a Pick Window (origem do texto), se aberta.
pub fn pick_window_at(hwnd: isize) -> Option<(i32, i32)> {
    pick_text_in(&capture::capture(hwnd, Some(PICK_SEARCH_AREA))?)
}
pub fn pick_window_open(hwnd: isize) -> bool { pick_window_at(hwnd).is_some() }
pub(in crate::win::nav) fn pick_zen_button(hwnd: isize, text: (i32, i32)) -> Option<(i32, i32)> {
    let area = pick_list_area(text);
    pick_zen_in(&capture::capture(hwnd, Some(area))?, area)
}
pub(in crate::win::nav) fn wait_pick(hwnd: isize, open: bool, ms: u128) -> Option<Option<(i32, i32)>> {
    let t0 = std::time::Instant::now();
    loop {
        let at = pick_window_at(hwnd);
        if at.is_some() == open { return Some(at); }
        if t0.elapsed().as_millis() >= ms { return None; }
        sleep(250);
    }
}
/// Fecha a Pick Window com Y (nunca ESC). Confere na tela.
pub(in crate::win::nav) fn close_pick(hwnd: isize) -> bool {
    for _ in 0..2 {
        if !pick_window_open(hwnd) { return true; }
        let _ = input::run(hwnd, &[Step::Key(VK_Y)]);
        if wait_pick(hwnd, false, 2000).is_some() { return true; }
    }
    !pick_window_open(hwnd)
}

/// Liga a coleta de Zen na Pick Window. Só com o char no spot, fora da zona segura e farmando.
/// Abre com Y (2 tentativas, 5 s entre elas), acha o botão "Zen" (rola a lista com a roda se
/// precisar), clica UMA vez, fecha com Y e confere que a tela ficou limpa e o helper ligado.
pub fn enable_zen_pick(hwnd: isize, world: u8, status: &dyn Fn(String)) -> Result<String, String> {
    let t = world_terrain_of(hwnd, world)?;
    let p = position(hwnd).ok_or("Pick: não li a posição")?;
    if t.is_safezone(p.0, p.1) {
        return Err(format!("Pick: char em zona segura em {p:?}; não ativo (o jogo desligaria)"));
    }
    let _session = input::Session::begin(hwnd)?;
    let mut opened = pick_window_at(hwnd);
    for attempt in 0..2 {
        if opened.is_some() { break; }
        if attempt > 0 {
            status("Pick: a janela não abriu; nova tentativa em 5 s".into());
            sleep(5000);
            opened = pick_window_at(hwnd);
            if opened.is_some() { break; }
        }
        status("Pick: Y para abrir a Pick Window".into());
        input::run(hwnd, &[Step::Key(VK_Y)])?;
        opened = wait_pick(hwnd, true, 3000).flatten();
    }
    let Some(text) = opened else {
        save_error_shot(hwnd, "Pick Window não abriu");
        return Err("a Pick Window (Y) não abriu em 2 tentativas".into());
    };
    sleep(400);
    // Relê a posição depois de assentar (a janela pode ter mudado ao abrir).
    let text = pick_window_at(hwnd).unwrap_or(text);
    status(format!("Pick: janela aberta (texto em {text:?})"));
    let wheel = (text.0 + PICK_WHEEL_FROM_TEXT.0, text.1 + PICK_WHEEL_FROM_TEXT.1);
    let mut btn = pick_zen_button(hwnd, text);
    let mut scrolls = 0;
    while btn.is_none() && scrolls < 8 {
        status(format!("Pick: botão Zen fora da lista visível; rolando ({})", scrolls + 1));
        input::run(hwnd, &[Step::Wheel { x: wheel.0, y: wheel.1, delta: -120 }])?;
        sleep(500);
        scrolls += 1;
        btn = pick_zen_button(hwnd, text);
    }
    let Some((bx, by)) = btn else {
        save_error_shot(hwnd, "Pick: botão Zen não achado");
        close_pick(hwnd);
        return Err(format!("Pick: botão Zen não achado ({scrolls} rolagens)"));
    };
    status(format!("Pick: clicando em Zen ({bx}, {by})"));
    input::run(hwnd, &[Step::Click { x: bx, y: by }])?;
    sleep(800);
    if !close_pick(hwnd) {
        save_error_shot(hwnd, "Pick Window não fechou");
        return Err("Pick: Zen clicado, mas a janela não fechou com Y".into());
    }
    // A janela não pode ter mexido no helper (ESC fechava). Confere e religa se preciso.
    if helper_state(hwnd) == Some(false) {
        set_helper(hwnd, true)?;
    }
    Ok(format!("Pick Zen ativado em {p:?}{}", if scrolls > 0 { format!(" após {scrolls} rolagem(ns)") } else { String::new() }))
}

/// Saiu do mapa do spot (cidade, teleporte, morte): o pick do Zen desligou e precisa de Y de novo.
/// Só conta leitura completa (nome do mapa E posição): leitura falhada não é "saiu".
pub fn pick_left_spot_map(map: Option<&str>, pos: Option<(i32, i32)>, spot_map: &str) -> bool {
    matches!((map, pos), (Some(m), Some(_)) if m != spot_map)
}
/// Hora de ativar: precisa (saiu do mapa), farmando no spot há `delay_s` e fora da espera.
pub fn pick_due(needed: bool, farm_since: Option<u64>, helper_on: Option<bool>, now: u64, retry_at: u64, delay_s: u64) -> bool {
    needed && helper_on == Some(true) && now >= retry_at && farm_since.is_some_and(|t| now.saturating_sub(t) >= delay_s)
}

#[cfg(test)]
mod pick_tests {
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
    fn acha_a_janela_parada_ou_deslocada_pelo_inventario() {
        assert_eq!(pick_text_in(&img(include_bytes!("../../../../tests/fixtures/pick/busca_aberta.png"))), Some((160, 208)));
        assert_eq!(pick_text_in(&img(include_bytes!("../../../../tests/fixtures/pick/busca_deslocada.png"))), Some((404, 189)));
        assert_eq!(pick_text_in(&img(include_bytes!("../../../../tests/fixtures/pick/busca_fechada.png"))), None);
    }
    #[test]
    fn acha_zen_na_lista_deslocada_e_depois_de_rolar() {
        let a = pick_list_area((160, 208));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../../../tests/fixtures/pick/lista_aberta.png")), a), Some((233, 551)));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../../../tests/fixtures/pick/lista_rolada.png")), a), Some((233, 445)));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../../../tests/fixtures/pick/lista_fechada.png")), a), None);
        let d = pick_list_area((404, 189));
        assert_eq!(pick_zen_in(&img(include_bytes!("../../../../tests/fixtures/pick/lista_deslocada.png")), d), Some((478, 531)));
    }
    #[test]
    fn ida_a_cidade_e_volta_reativam_uma_vez() {
        // No spot (Kanturu): não marca. Leitura falhada: não marca. Lorencia: marca.
        assert!(!pick_left_spot_map(Some("Kanturu"), Some((160, 197)), "Kanturu"));
        assert!(!pick_left_spot_map(None, Some((160, 197)), "Kanturu"));
        assert!(!pick_left_spot_map(Some("Lorencia"), None, "Kanturu"));
        assert!(pick_left_spot_map(Some("Lorencia"), Some((146, 112)), "Kanturu"));
        // De volta: só depois de 30 s farmando, helper ligado e fora da espera de nova tentativa.
        assert!(!pick_due(true, Some(1000), Some(true), 1010, 0, 30));
        assert!(pick_due(true, Some(1000), Some(true), 1030, 0, 30));
        assert!(!pick_due(true, Some(1000), Some(false), 1030, 0, 30));
        assert!(!pick_due(true, None, Some(true), 1030, 0, 30));
        assert!(!pick_due(true, Some(1000), Some(true), 1030, 1100, 30));
        // Já ativado nesta volta: não clica de novo (o botão alterna e não mostra o estado).
        assert!(!pick_due(false, Some(1000), Some(true), 5000, 0, 30));
    }
}
