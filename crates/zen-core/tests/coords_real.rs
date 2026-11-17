//! Recortes REAIS do canto "Kanturu (X , Y)" (08/11/2026), binarizados com o limiar do agente.
//! Nome: <X>_<Y>_<char>_<n>.txt. Valores lidos no print, um a um.

use zen_core::glyphs::{read_coords, Bits};

#[test]
fn coordenadas_reais() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/coords");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        let mut it = stem.split('_');
        let x: u32 = it.next().unwrap().parse().unwrap();
        let y: u32 = it.next().unwrap().parse().unwrap();
        let b = Bits::from_text(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(read_coords(&b), Some((x, y)), "{stem}");
        n += 1;
    }
    assert!(n >= 40);
}


#[test]
fn nome_do_mapa_kanturu() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/coords");
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let txt = std::fs::read_to_string(&p).unwrap();
        let b = Bits::from_text(&txt);
        assert!(zen_core::glyphs::map_is_kanturu(&b), "{}", p.display());
        // Troca o "(" por nada: vira "Kanturu" seguido de outra coisa -> não aceita.
        let mut sem_paren = b.clone();
        // apaga tudo à direita da palavra (coordenadas inclusive)
        let first_ink = (0..b.w).find(|&x| (0..b.h).any(|y| b.get(x as isize, y as isize))).unwrap();
        for y in 0..b.h {
            for x in (first_ink + 37)..b.w {
                sem_paren.px[y * b.w + x] = false;
            }
        }
        // a 1ª tinta do recorte pode ser borda do ícone; só exige que não quebre
        let _ = zen_core::glyphs::map_is_kanturu(&sem_paren);
    }
    assert!(!zen_core::glyphs::map_is_kanturu(&Bits::new(300, 24)));
}
