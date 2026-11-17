//! Avisos amarelos do servidor (invasões, eventos, itens criados). SÓ LEITURA.
//!
//! Medido em 17/11/2026 no cliente The Classic: os avisos ficam numa fila FIXA dentro do
//! main.exe (região IMAGE alocada em 0x400000, sem ASLR), igual em todos os processos:
//! posições de 0x108 bytes a partir de 0x813ddd0 (6 linhas na tela: +0 em cima, +5 embaixo). Uma leitura de ~4,5 KB por ciclo, sem
//! varrer a memória. Se o cliente for atualizado e o endereço mudar, nada quebra: só não
//! aparece aviso (o texto tem de passar pelo filtro de `zen_core::avisos::clean`).

use crate::win::memory::Proc;
use zen_core::avisos::{clean, Watcher};

pub const ANCORA: u64 = 0x813ddd0;
pub const PASSO: u64 = 0x108;
/// Posições lidas antes e depois da âncora.
const LADO: i64 = 8;

/// Textos de aviso que estão na fila agora (`None` = não leu; vazio = tela sem aviso).
/// Posição apagada pelo jogo = primeiro byte 0 (o resto do texto velho fica lá): é ignorada.
pub fn read(pid: u32) -> Option<Vec<String>> {
    let p = Proc::open(pid)?;
    let ini = ANCORA - LADO as u64 * PASSO;
    let n = (2 * LADO as usize + 1) * PASSO as usize;
    let mut buf = vec![0u8; n];
    if !p.read(ini, &mut buf) {
        return None;
    }
    let v = buf.chunks(PASSO as usize)
        .filter_map(|slot| {
            // Alguns avisos começam com 1 byte de controle (cor/tipo) antes do texto.
            let skip = slot.iter().take(2).take_while(|&&b| b != 0 && !(32..127).contains(&b)).count();
            clean(&slot[skip..slot.len().min(skip + 0x100)])
        })
        .collect();
    Some(v)
}

/// Lê a fila de UM cliente por PC e devolve só os avisos que apareceram desde a leitura
/// anterior. Trocou o processo (relogou) = recomeça a referência.
#[derive(Default)]
pub struct NoticeReader {
    pid: Option<u32>,
    w: Watcher,
}

impl NoticeReader {
    pub fn poll(&mut self, pid: u32) -> Vec<String> {
        if self.pid != Some(pid) {
            self.pid = Some(pid);
            self.w = Watcher::default();
        }
        match read(pid) {
            Some(now) => self.w.update(now),
            None => Vec::new(),
        }
    }
}
