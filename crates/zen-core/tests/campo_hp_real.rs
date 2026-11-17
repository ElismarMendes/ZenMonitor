//! Recortes REAIS do número de HP (07/11/2026), já binarizados (só a faixa de 12 linhas).
//! Nome do arquivo: <valor>_<personagem>_<n>.txt. 24 rotulados à mão + 48 conferidos olhando.

use zen_core::glyphs::Bits;
use zen_core::hp::read_hp;

#[test]
fn prints_reais_de_vida() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hp_field");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        let want: u32 = stem.split('_').next().unwrap().parse().unwrap();
        let b = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(read_hp(&b), Some(want), "{}", p.display());
        n += 1;
    }
    assert!(n >= 70, "só {n} recortes");
}
