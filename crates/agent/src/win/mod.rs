//! Implementação real de `GameAccess` com Win32/WinRT.

pub mod capture;
pub mod chat;
pub mod health;
pub mod hotkey;
pub mod input;
pub mod viewport;
pub mod memory;
pub mod nav;
pub mod navigation;
pub mod spots;
pub mod objects;
pub mod ocr;
pub mod tcp;
pub mod windows_list;

use crate::{GameAccess, GameWindow};

pub struct WinAccess {
    pub window_class: String,
    pub zen_ocr_region: Option<[i32; 4]>,
    pub allow_loopback_tcp: bool,
    /// Vazio = todos os chars. Com nomes, o agente IGNORA qualquer outra janela do jogo
    /// (não lê título, não faz OCR, não abre o processo).
    pub allowed_chars: Vec<String>,
    /// Recorte do número de HP (coordenadas da janela inteira). `None` = vida desligada.
    pub hp_region: Option<[i32; 4]>,
}

impl WinAccess {
    pub fn new(window_class: String, region: &[i32], allow_loopback_tcp: bool, allowed_chars: Vec<String>) -> Self {
        let zen_ocr_region = (region.len() == 4).then(|| [region[0], region[1], region[2], region[3]]);
        Self {
            window_class,
            zen_ocr_region,
            allow_loopback_tcp,
            allowed_chars,
            hp_region: None,
        }
    }

    pub fn with_hp_region(mut self, region: &[i32]) -> Self {
        self.hp_region = (region.len() == 4).then(|| [region[0], region[1], region[2], region[3]]);
        self
    }
}

impl GameAccess for WinAccess {
    fn list_windows(&self) -> Vec<GameWindow> {
        let all = windows_list::find_by_class(&self.window_class);
        if self.allowed_chars.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|w| {
                zen_core::title::parse(&w.title)
                    .name
                    .is_some_and(|n| self.allowed_chars.iter().any(|a| a.eq_ignore_ascii_case(&n)))
            })
            .collect()
    }

    fn has_external_tcp(&self, pid: u32) -> bool {
        tcp::has_established(pid, self.allow_loopback_tcp)
    }

    fn read_u32(&self, pid: u32, addr: u64) -> Option<u32> {
        memory::read_u32(pid, addr)
    }

    fn scan_u32(&self, pid: u32, value: u32) -> Vec<u64> {
        memory::scan_u32(pid, value)
    }

    fn zen_field_ready(&self) -> bool {
        self.zen_ocr_region.is_some()
    }

    fn read_zen_field(&self, hwnd: isize) -> Option<u32> {
        // Só o recorte do campo: a janela inteira tem números do chat e do chão que viram
        // semente errada (aconteceu: "162812 Zen Obtained").
        let region = self.zen_ocr_region?;
        let img = capture::capture(hwnd, Some(region))?;
        let bits = zen_core::glyphs::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
        let n = zen_core::glyphs::read_number(&bits)?;
        u32::try_from(n).ok()
    }

    fn screenshot_png(&self, hwnd: isize) -> Option<Vec<u8>> {
        capture::capture(hwnd, None)?.to_png()
    }

    fn hp_field_ready(&self) -> bool {
        self.hp_region.is_some()
    }

    fn read_hp_field(&self, hwnd: isize) -> Option<u32> {
        let img = capture::capture(hwnd, Some(self.hp_region?))?;
        let bits = zen_core::hp::binarize_bgra(&img.bgra, img.w as usize, img.h as usize);
        zen_core::hp::read_hp(&bits)
    }

    fn read_u16_many(&self, pid: u32, addrs: &[u64]) -> Vec<Option<u16>> {
        memory::read_u16_many(pid, addrs)
    }

    fn scan_u16(&self, pid: u32, value: u16) -> Vec<u64> {
        memory::scan_u16(pid, value, zen_core::hplock::MAX_CANDIDATES)
    }

    fn process_start(&self, pid: u32) -> Option<u64> {
        memory::process_start(pid)
    }

    fn death_shot(&self, hwnd: isize) -> Option<(Vec<u8>, Option<u32>)> {
        let full = capture::capture(hwnd, None)?;
        let hp = self.hp_region.and_then(|r| full.crop(r)).and_then(|c| {
            let bits = zen_core::hp::binarize_bgra(&c.bgra, c.w as usize, c.h as usize);
            zen_core::hp::read_hp(&bits)
        });
        Some((full.to_png()?, hp))
    }
}
