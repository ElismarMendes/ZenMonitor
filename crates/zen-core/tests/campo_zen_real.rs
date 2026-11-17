//! Os recortes REAIS do campo do Zen (prints do jogo em 07/11/2026), já binarizados.
//! O nome do arquivo é o valor que aparece na tela.

use zen_core::glyphs::{read_number, Bits};

#[test]
fn prints_reais() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/zen_field");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let want: u64 = p.file_stem().unwrap().to_str().unwrap().parse().unwrap();
        let b = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(read_number(&b), Some(want), "{}", p.display());
        n += 1;
    }
    assert!(n >= 3);
}
