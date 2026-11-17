//! Recuperação até o spot (Partes 1-4), testada em 08/11/2026 nos dois chars depois de um DC:
//! Lorencia -> M -> Kanturu (3 s) -> segurar + cliques até o tile exato -> helper ligado.
//!
//! Tudo lido na TELA (coordenadas, nome do mapa, ícone do helper) e conferido a cada passo.
//! Input só pelo `input` (SendInput, foco conferido). Nada de memória do jogo é escrito.

use crate::win::{capture, input};
use zen_core::actions::{helper_on, Step, HELPER_ICON, HELPER_WHITE};
use zen_core::glyphs::{binarize_white, map_is_kanturu, read_coords, Bits, COORDS_WHITE};
use zen_core::terrain::Terrain;

mod telas;
mod mundo;
mod rotinas;
#[cfg(test)]
mod testes;

pub use telas::*;
pub use mundo::*;
pub use rotinas::*;

