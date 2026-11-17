//! Captura da janela com PrintWindow(PW_RENDERFULLCONTENT): funciona com a janela coberta.
//!
//! Coordenadas: a imagem é a JANELA INTEIRA (com barra de título), do canto superior
//! esquerdo da janela. É o mesmo sistema do PNG que o `zen-diag ocr` salva, então a
//! `zen_ocr_region` do agent.toml pode ser medida direto nesse PNG.

pub use crate::image::Image;

/// Onde fica o cliente do jogo dentro da janela "padrão" (1030x797, borda de 3 px e barra de
/// título de 26 px): todas as coordenadas do projeto foram medidas nesse quadro.
pub const CLIENT_AT: (i32, i32) = (3, 26);

/// Canto do quadro padrão na tela: cliente - CLIENT_AT. Janela com outra borda/título (visto em
/// 09/11 no DESKTOP-KIS9DG7: janela 1024x798, sem borda lateral) cai no mesmo quadro.
pub fn frame_origin(hwnd: isize) -> Option<(i32, i32)> {
    let mut p = windows::Win32::Foundation::POINT { x: 0, y: 0 };
    unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(HWND(hwnd as *mut _), &mut p) }.ok().ok()?;
    Some((p.x - CLIENT_AT.0, p.y - CLIENT_AT.1))
}
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

const PW_RENDERFULLCONTENT: u32 = 2;

pub(super) fn window_pid(hwnd: isize) -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    let h = HWND(hwnd as *mut _);
    if !unsafe { IsWindow(h) }.as_bool() { return None; }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)); }
    (pid != 0).then_some(pid)
}

// PrintWindow pode pedir renderização ao cliente. Não sobrepor capturas de threads
// diferentes, nem aceitar um quadro cujo HWND foi reutilizado/movido durante a leitura.
static CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn capture(hwnd: isize, region: Option<[i32; 4]>) -> Option<Image> {
    let _grab = CAPTURE.lock().ok()?;
    let pid = window_pid(hwnd)?;
    let layout_epoch = super::viewport::epoch();
    let origin = frame_origin(hwnd)?;
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() { return None; }
        let mut rc = RECT::default();
        GetWindowRect(hwnd, &mut rc).ok()?;
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w <= 0 || h <= 0 || w > 8192 || h > 8192 {
            return None; // minimizada ou absurda
        }
        let hdc = GetDC(hwnd);
        if hdc.0.is_null() { return None; }
        let mem = CreateCompatibleDC(hdc);
        if mem.0.is_null() {
            ReleaseDC(hwnd, hdc);
            return None;
        }
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        if bmp.0.is_null() {
            let _ = DeleteDC(mem);
            ReleaseDC(hwnd, hdc);
            return None;
        }
        let old = SelectObject(mem, bmp);
        let ok = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        // GetDIBits exige que o bitmap não esteja selecionado em um DC.
        SelectObject(mem, old);

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negativo = de cima para baixo
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(hwnd, hdc);

        let mut after = RECT::default();
        if !ok || lines != h
            || window_pid(hwnd.0 as isize) != Some(pid)
            || GetWindowRect(hwnd, &mut after).is_err()
            || (after.left, after.top, after.right, after.bottom) != (rc.left, rc.top, rc.right, rc.bottom)
            || frame_origin(hwnd.0 as isize) != Some(origin)
        {
            return None;
        }
        let raw = Image {
            w: w as u32,
            h: h as u32,
            bgra: buf,
        };
        // Reposiciona no quadro padrão (cliente em CLIENT_AT). Janela padrão: nada muda.
        let (fx, fy) = origin;
        let (dx, dy) = (rc.left - fx, rc.top - fy);
        let img = if dx == 0 && dy == 0 {
            raw
        } else {
            let (nw, nh) = ((w + dx).max(1), (h + dy).max(1));
            let mut out = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..h {
                let ty = y + dy;
                if ty < 0 || ty >= nh {
                    continue;
                }
                for x in 0..w {
                    let tx = x + dx;
                    if tx < 0 || tx >= nw {
                        continue;
                    }
                    let si = ((y * w + x) * 4) as usize;
                    let ti = ((ty * nw + tx) * 4) as usize;
                    out[ti..ti + 4].copy_from_slice(&raw.bgra[si..si + 4]);
                }
            }
            Image { w: nw as u32, h: nh as u32, bgra: out }
        };
        super::viewport::observe(hwnd.0 as isize,pid,layout_epoch,&img);
        match region {
            Some(r) => img.crop(r),
            None => Some(img),
        }
    }
}

/// Recorte do Zen já tratado para o OCR (preto no branco, ampliado 3x, com margem).
pub fn capture_for_ocr(hwnd: isize, region: [i32; 4]) -> Option<Image> {
    Some(capture(hwnd, Some(region))?.for_ocr())
}

/// Copia somente o recorte visível, sem pedir ao jogo para renderizar outra janela inteira.
/// Falha se o recorte estiver coberto/minimizado; o chamador decide se aceita PrintWindow.
pub fn visible_region(hwnd: isize, region: [i32; 4]) -> Option<Image> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, IsIconic, WindowFromPoint, GA_ROOT};
    let h = HWND(hwnd as *mut _);
    let [x, y, w, height] = region;
    if w <= 0 || height <= 0 { return None; }
    unsafe {
        if IsIconic(h).as_bool() { return None; }
        let (fx, fy) = frame_origin(hwnd)?;
        let mut rc = RECT::default();
        GetWindowRect(h, &mut rc).ok()?;
        // O quadro padrão pode ter 3 px de margem fora de uma janela sem borda.
        let left = (fx+x).max(rc.left);
        let top = (fy+y).max(rc.top);
        let right = (fx+x+w).min(rc.right);
        let bottom = (fy+y+height).min(rc.bottom);
        let (cw, ch) = (right-left, bottom-top);
        if cw <= 0 || ch <= 0 { return None; }
        for (px, py) in [(left,top), (right-1,top), (left,bottom-1), (right-1,bottom-1), ((left+right)/2,(top+bottom)/2)] {
            if GetAncestor(WindowFromPoint(POINT { x: px, y: py }), GA_ROOT) != h {
                return None;
            }
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, cw, ch);
        let old = SelectObject(mem, bmp);
        let copied = BitBlt(mem, 0, 0, cw, ch, screen, left, top, SRCCOPY).is_ok();
        SelectObject(mem, old);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cw, biHeight: -ch, biPlanes: 1, biBitCount: 32,
                biCompression: BI_RGB.0, ..Default::default()
            }, ..Default::default()
        };
        let mut pixels = vec![0; (cw*ch*4) as usize];
        let lines = GetDIBits(mem, bmp, 0, ch as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !copied || lines != ch { return None; }
        let mut bgra = vec![0; (w*height*4) as usize];
        let (dx, dy) = (left-fx-x, top-fy-y);
        for row in 0..ch {
            let src = (row*cw*4) as usize;
            let dst = (((row+dy)*w+dx)*4) as usize;
            bgra[dst..dst+(cw*4) as usize].copy_from_slice(&pixels[src..src+(cw*4) as usize]);
        }
        Some(Image { w: w as u32, h: height as u32, bgra })
    }
}
