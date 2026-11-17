//! Leitura de memória. NUNCA escreve. Abre o processo só com VM_READ + QUERY_INFORMATION
//! e fecha o handle logo depois de cada operação.

use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READWRITE, PAGE_GUARD, PAGE_NOACCESS,
    PAGE_READWRITE, PAGE_WRITECOPY,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

pub struct Proc(HANDLE);

impl Proc {
    pub fn open(pid: u32) -> Option<Self> {
        unsafe {
            OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid)
                .ok()
                .map(Proc)
        }
    }

    pub fn read(&self, addr: u64, buf: &mut [u8]) -> bool {
        let mut n = 0usize;
        unsafe {
            ReadProcessMemory(
                self.0,
                addr as *const c_void,
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                Some(&mut n),
            )
            .is_ok()
                && n == buf.len()
        }
    }

    /// Regiões graváveis e commitadas: onde o Zen (e o chat) moram.
    pub fn writable_regions(&self) -> Vec<(u64, usize)> {
        let mut out = Vec::new();
        let mut addr: u64 = 0x10000;
        let max: u64 = 0x7FFF_FFFF_0000;
        while addr < max {
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            let got = unsafe {
                VirtualQueryEx(
                    self.0,
                    Some(addr as *const c_void),
                    &mut mbi,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if got == 0 {
                break;
            }
            let base = mbi.BaseAddress as u64;
            let size = mbi.RegionSize;
            let p = mbi.Protect;
            let writable = (p & (PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE)).0 != 0;
            let bad = (p & (PAGE_GUARD | PAGE_NOACCESS)).0 != 0;
            if mbi.State == MEM_COMMIT && writable && !bad && size <= 256 * 1024 * 1024 {
                out.push((base, size));
            }
            let next = base.saturating_add(size as u64);
            if next <= addr {
                break;
            }
            addr = next;
        }
        out
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn read_u32(pid: u32, addr: u64) -> Option<u32> {
    let p = Proc::open(pid)?;
    let mut b = [0u8; 4];
    p.read(addr, &mut b).then(|| u32::from_le_bytes(b))
}

/// Varredura completa. Cara: só chamada na trava.
pub fn scan_u32(pid: u32, value: u32) -> Vec<u64> {
    let Some(p) = Proc::open(pid) else { return vec![] };
    let mut out = Vec::new();
    // Blocos de 1 MiB: uma página ilegível no meio não derruba a região inteira,
    // e o tamanho múltiplo de 4 preserva o alinhamento entre blocos.
    const CHUNK: usize = 1 << 20;
    let mut buf = vec![0u8; CHUNK];
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = CHUNK.min(size - off);
            if p.read(base + off as u64, &mut buf[..n]) {
                out.extend(zen_core::ramlock::find_u32(&buf[..n], base + off as u64, value));
            }
            off += n;
        }
        if out.len() > 100_000 {
            break; // valor comum demais; a trava vai pedir outra semente
        }
    }
    out
}

/// Varredura completa por um u16 alinhado em 2. Para em `limit` (valor comum demais).
pub fn scan_u16(pid: u32, value: u16, limit: usize) -> Vec<u64> {
    let Some(p) = Proc::open(pid) else { return vec![] };
    let mut out = Vec::new();
    const CHUNK: usize = 1 << 20;
    let mut buf = vec![0u8; CHUNK];
    let want = value.to_le_bytes();
    for (base, size) in p.writable_regions() {
        let mut off = 0usize;
        while off < size {
            let n = CHUNK.min(size - off);
            let at = base + off as u64;
            if p.read(at, &mut buf[..n]) {
                for i in (0..n.saturating_sub(1)).step_by(2) {
                    if buf[i] == want[0] && buf[i + 1] == want[1] {
                        out.push(at + i as u64);
                    }
                }
            }
            off += n;
            if out.len() > limit {
                return out;
            }
        }
    }
    out
}

/// Lê u16 em vários endereços. Endereços próximos saem numa leitura só (blocos de até 64 KiB);
/// se o bloco falhar (passa do fim da região), cai para leitura um a um.
pub fn read_u16_many(pid: u32, addrs: &[u64]) -> Vec<Option<u16>> {
    let mut out = vec![None; addrs.len()];
    let Some(p) = Proc::open(pid) else { return out };
    let mut order: Vec<usize> = (0..addrs.len()).collect();
    order.sort_by_key(|&i| addrs[i]);
    const SPAN: u64 = 64 * 1024;
    let mut buf = vec![0u8; SPAN as usize + 2];
    let mut k = 0;
    while k < order.len() {
        let start = addrs[order[k]];
        let mut j = k;
        while j < order.len() && addrs[order[j]] + 2 <= start + SPAN {
            j += 1;
        }
        let end = addrs[order[j - 1]] + 2;
        let len = (end - start) as usize;
        if p.read(start, &mut buf[..len]) {
            for &i in &order[k..j] {
                let o = (addrs[i] - start) as usize;
                out[i] = Some(u16::from_le_bytes([buf[o], buf[o + 1]]));
            }
        } else {
            for &i in &order[k..j] {
                let mut b = [0u8; 2];
                if p.read(addrs[i], &mut b) {
                    out[i] = Some(u16::from_le_bytes(b));
                }
            }
        }
        k = j;
    }
    out
}

/// Hora de criação do processo (FILETIME em 100 ns). Junto com o pid, identifica o processo:
/// um pid reaproveitado por outro processo tem outra hora de criação.
pub fn process_start(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetProcessTimes, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let ok = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime))
    }
}

/// Caminho completo do .exe do processo (para achar a pasta Data do jogo).
pub fn process_path(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::System::Threading::{QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}
