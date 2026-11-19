//! Arquivos de terreno REAIS do cliente The Classic (08/11/2026).

use zen_core::terrain::{world_to_tile, Terrain};

fn load(name: &str) -> Terrain {
    let p = format!("{}/tests/fixtures/terrain/{name}", env!("CARGO_MANIFEST_DIR"));
    Terrain::from_client_file(&std::fs::read(p).unwrap()).unwrap()
}

#[test]
fn lorencia_confere_com_o_cliente_aberto() {
    let t = load("EncTerrain1.att");
    assert_eq!(t.world, 1);
    // MuMain: TerrainWall[123*256+135] == 5 em Lorencia.
    assert_eq!(t.attr(135, 123), Some(5));
}

#[test]
fn kanturu_tiles_onde_os_chars_andaram_sao_piso() {
    let t = load("EncTerrain38.att");
    assert_eq!(t.world, 38);
    // Lidos na tela (ReDMooN e x9901 farmando, 08/11/2026).
    for (x, y) in [(158, 195), (159, 197), (160, 197), (161, 198), (162, 199), (163, 196), (122, 234), (123, 233), (124, 232), (122, 235)] {
        assert!(t.walkable(x, y), "({x},{y})");
    }
}

#[test]
fn kanturu_rota_da_zona_segura_ate_o_spot() {
    let t = load("EncTerrain38.att");
    // Zona segura grande do canto inferior esquerdo (entrada do mapa): tiles 16..47 x 202..213.
    let safe = (30, 210);
    assert!(t.is_safezone(safe.0, safe.1) && t.walkable(safe.0, safe.1));
    let path = t.path(safe, (160, 197)).expect("existe caminho da zona segura até o spot");
    assert!(path.len() > 50);
    let wp = t.waypoints(safe, &path, 6);
    assert_eq!(*wp.last().unwrap(), (160, 197));
    assert!(wp.len() < path.len());
    let mut cur = safe;
    for w in &wp {
        assert!(t.line_clear(cur, *w));
        cur = *w;
    }
}

#[test]
fn posicao_do_mundo_vira_tile() {
    assert_eq!(world_to_tile(15950.0), 159);
    assert_eq!(world_to_tile(16000.0), 160);
}

#[test]
fn kanturu_corredor_dos_cristais_fica_fora_da_rota() {
    let t = load("EncTerrain38.att");
    assert!(!t.walkable(227, 148), "(227,148) travava os chars: bloqueado de propósito");
    let rota = t.path((222, 156), (225, 137)).expect("ainda há caminho pelo meio do corredor");
    assert!(rota.iter().all(|&(x, y)| !(226..=227).contains(&x) || !(144..=152).contains(&y)), "rota passou colada nos cristais: {rota:?}");
}

#[test]
fn a_estrela_no_kanturu_inteiro_em_menos_de_1_ms_em_media() {
    let t = load("EncTerrain38.att");
    let (a, b) = ((30, 210), (225, 137));
    let n = 50;
    let t0 = std::time::Instant::now();
    let mut len = 0;
    for _ in 0..n {
        len = t.path(a, b).expect("rota").len();
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("A* {a:?}->{b:?}: {len} passos, {ms:.3} ms");
    // Em modo debug é bem mais lento; o limite vale para release (cargo test --release).
    if !cfg!(debug_assertions) {
        assert!(ms < 1.0, "{ms} ms");
    }
    let s = t.smooth(a, &t.path(a, b).unwrap());
    let mut cur = a;
    for p in s {
        assert!(t.los_clear(cur, p));
        cur = p;
    }
}
