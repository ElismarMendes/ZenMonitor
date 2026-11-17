//! Rotinas de farm: volta ao spot, baú do Kanturu, Fortuna, Pick do Zen, Elpis e Dark Raven.
//! Cada rotina usa só `telas`, `mundo`, `leitura` e `entrada`.

mod volta_spot;
mod bau_kanturu;
mod fortuna;
mod pick;
mod elpis;
mod raven;

pub use volta_spot::*;
pub use bau_kanturu::*;
pub use fortuna::*;
pub use pick::*;
pub use elpis::*;
pub use raven::*;
