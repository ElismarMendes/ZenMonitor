//! Decisões de combate no caminho e no spot (v4, 11/11), puras e testadas. A posição dos mobs
//! vem da lista de personagens da memória do jogo (só leitura, `objects`); aqui só tiles.

fn cheb(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// Mob que bloqueia o caminho: o mais perto do char entre os que estão a até 1 tile de algum
/// dos próximos `look` tiles da rota (sem contar o tile atual) e a até `reach` tiles do char.
pub fn blocking_mob(pos: (i32, i32), path: &[(i32, i32)], mobs: &[(i32, i32)], look: usize, reach: i32) -> Option<(i32, i32)> {
    let ahead: Vec<(i32, i32)> = path.iter().copied().filter(|p| *p != pos).take(look).collect();
    mobs.iter()
        .copied()
        .filter(|m| cheb(*m, pos) <= reach && ahead.iter().any(|a| cheb(*a, *m) <= 1))
        .min_by_key(|m| cheb(*m, pos))
}

/// Onde soltar a skill de área: com `focus` (o mob que bloqueia), nele; senão o mob, a até
/// `range` do char, com mais mobs a até `radius` em volta (empate: o mais perto do char).
/// Sem mobs ao alcance: no próprio char. Devolve (tile, mobs que a área pega).
pub fn aim(pos: (i32, i32), mobs: &[(i32, i32)], focus: Option<(i32, i32)>, range: i32, radius: i32) -> ((i32, i32), usize) {
    let inr: Vec<(i32, i32)> = mobs.iter().copied().filter(|m| cheb(*m, pos) <= range).collect();
    let count = |c: (i32, i32)| inr.iter().filter(|m| cheb(**m, c) <= radius).count();
    if let Some(f) = focus.filter(|f| cheb(*f, pos) <= range) {
        return (f, count(f));
    }
    let mut best = (pos, count(pos));
    for &c in &inr {
        let n = count(c);
        if n > best.1 || (n == best.1 && cheb(c, pos) < cheb(best.0, pos)) {
            best = (c, n);
        }
    }
    best
}

/// Limpeza do spot ao chegar: mobs a até `radius` do spot. Continua enquanto houver mob e a
/// rodada anterior tiver diminuído a contagem (senão é mob que não morre / renasce na hora:
/// liga o helper e deixa ele farmar), no máximo `max_rounds`.
pub fn keep_clearing(round: u32, max_rounds: u32, before: Option<usize>, now: usize) -> bool {
    now > 0 && round < max_rounds && before.map_or(true, |b| now < b)
}

/// Mobs a até `r` tiles de `c`.
pub fn mobs_within(c: (i32, i32), mobs: &[(i32, i32)], r: i32) -> usize {
    mobs.iter().filter(|m| cheb(**m, c) <= r).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mob_no_caminho_bloqueia_e_mob_de_lado_nao() {
        let pos = (10, 10);
        let path = [(10, 10), (11, 10), (12, 10), (13, 10), (14, 10)];
        // em cima do próximo tile
        assert_eq!(blocking_mob(pos, &path, &[(11, 10)], 3, 3), Some((11, 10)));
        // colado na rota (1 tile ao lado do tile 2)
        assert_eq!(blocking_mob(pos, &path, &[(12, 11)], 3, 3), Some((12, 11)));
        // longe da rota
        assert_eq!(blocking_mob(pos, &path, &[(11, 14)], 3, 3), None);
        // atrás do char não conta
        assert_eq!(blocking_mob(pos, &path, &[(8, 10)], 3, 3), None);
        // além do alcance
        assert_eq!(blocking_mob(pos, &path, &[(14, 10)], 3, 3), None);
        // o mais perto de dois
        assert_eq!(blocking_mob(pos, &path, &[(13, 10), (11, 11)], 3, 3), Some((11, 11)));
    }

    #[test]
    fn mira_no_bloqueio_ou_no_grupo() {
        let pos = (50, 50);
        let mobs = [(52, 50), (56, 50), (56, 51), (55, 52)];
        // grupo de 3 (56,50)/(56,51)/(55,52): o ponto que pega os 3 e está mais perto do char
        assert_eq!(aim(pos, &mobs, None, 6, 2), ((55, 52), 3));
        // com foco: no mob que bloqueia
        assert_eq!(aim(pos, &mobs, Some((52, 50)), 6, 2).0, (52, 50));
        // foco fora do alcance: volta ao grupo
        assert_eq!(aim(pos, &mobs, Some((70, 70)), 6, 2).0, (55, 52));
        // sem mobs: no char
        assert_eq!(aim(pos, &[], None, 6, 4), (pos, 0));
    }

    #[test]
    fn limpeza_para_quando_nao_diminui() {
        assert!(keep_clearing(0, 3, None, 5));
        assert!(keep_clearing(1, 3, Some(5), 2));
        assert!(!keep_clearing(1, 3, Some(5), 5), "não diminuiu: helper assume");
        assert!(!keep_clearing(1, 3, Some(5), 0), "limpo");
        assert!(!keep_clearing(3, 3, Some(9), 2), "limite de rodadas");
    }
}
