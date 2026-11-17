//! Mobs, NPCs e jogadores em volta de cada char, lidos da memória do jogo. SÓ LEITURA.
//! Acha a entrada do próprio char (nome + posição da tela) uma vez por processo e depois lê
//! um bloco em volta dela a cada consulta (formato em `zen_core::objects`).

use crate::win::memory::Proc;
use std::collections::HashMap;
use std::sync::Mutex;
use zen_core::objects::{parse_block, parse_entry, Kind, Obj, STRIDE};

/// Endereço do NOME da entrada do char, por processo.
static HERO: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);
/// Última varredura completa sem sucesso, por processo.
static FAILED: Mutex<Option<HashMap<u32, std::time::Instant>>> = Mutex::new(None);
/// Entradas lidas antes e depois do char (a lista tem algumas centenas; os mobs ficaram a
/// -84 e -343 entradas nos testes).
const SPAN: usize = 700;

fn hero_ok(p: &Proc, addr: u64, name: &str, pos: (i32, i32)) -> bool {
    let mut e = vec![0u8; STRIDE];
    if !p.read(addr, &mut e) {
        return false;
    }
    match parse_entry(&e) {
        Some(o) => o.kind == Kind::Player && o.name == name && (o.x - pos.0).abs() <= 2 && (o.y - pos.1).abs() <= 2,
        None => false,
    }
}

/// Varredura completa atrás do nome do char com a posição certa (alguns segundos; uma vez).
fn find_hero(p: &Proc, name: &str, pos: (i32, i32)) -> Option<u64> {
    let pat: Vec<u8> = name.as_bytes().iter().copied().chain(Some(0)).collect();
    let mut buf = vec![0u8; 1 << 20];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = buf.len().min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                let mut i = 0;
                while i + pat.len() <= n {
                    if buf[i..i + pat.len()] == pat[..] {
                        let a = base + (off + i) as u64;
                        if hero_ok(p, a, name, pos) {
                            return Some(a);
                        }
                    }
                    i += 1;
                }
            }
            off += n.saturating_sub(pat.len()).max(1);
        }
    }
    None
}

/// Objetos vivos em volta do char `name` (posição `pos` lida na tela). O próprio char fica de fora.
pub fn nearby(pid: u32, name: &str, pos: (i32, i32)) -> Option<Vec<Obj>> {
    let p = Proc::open(pid)?;
    let cached = HERO.lock().ok()?.get_or_insert_with(HashMap::new).get(&pid).copied();
    let hero = match cached.filter(|&a| hero_ok(&p, a, name, pos)) {
        Some(a) => a,
        None => {
            // Varredura completa é cara (~1,4 s): depois de uma falha, espera 60 s por processo.
            {
                let mut g = FAILED.lock().ok()?;
                let m = g.get_or_insert_with(HashMap::new);
                if m.get(&pid).is_some_and(|t: &std::time::Instant| t.elapsed().as_secs() < 60) {
                    return None;
                }
                m.insert(pid, std::time::Instant::now());
            }
            let a = find_hero(&p, name, pos)?;
            if let Ok(mut g) = FAILED.lock() {
                g.get_or_insert_with(HashMap::new).remove(&pid);
            }
            tracing::info!(char = name, endereco = format!("{a:#x}"), "lista de personagens achada (mobs/NPCs)");
            HERO.lock().ok()?.get_or_insert_with(HashMap::new).insert(pid, a);
            a
        }
    };
    let start = hero.saturating_sub((SPAN * STRIDE) as u64);
    let len = 2 * SPAN * STRIDE;
    let mut buf = vec![0u8; len];
    // Lê por páginas: uma página ilegível no meio vira zeros e não derruba o bloco.
    for off in (0..len).step_by(4096) {
        let n = 4096.min(len - off);
        let _ = p.read(start + off as u64, &mut buf[off..off + n]);
    }
    let anchor = (hero - start) as usize;
    let mut v = parse_block(&buf, anchor);
    v.retain(|o| !(o.kind == Kind::Player && o.name == name));
    Some(v)
}

/// Mesmo que `nearby`, a partir da janela.
pub fn nearby_hwnd(hwnd: isize, name: &str, pos: (i32, i32)) -> Option<Vec<Obj>> {
    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(windows::Win32::Foundation::HWND(hwnd as *mut _), Some(&mut pid));
    }
    nearby(pid, name, pos)
}
