//! Ícone do helper em prints reais (08/11/2026): pause = ligado, play = desligado.

use zen_core::actions::helper_on;
use zen_core::glyphs::Bits;

#[test]
fn icones_reais() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/helper");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_str().unwrap().to_string();
        let b = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
        let want = name.starts_with("ligado");
        assert_eq!(helper_on(&b), Some(want), "{name}");
        n += 1;
    }
    assert!(n >= 5);
    assert_eq!(helper_on(&Bits::new(36, 24)), None, "ícone ausente não é estado");
}
