//! Testes por propriedade sem dependência externa (gerador xorshift próprio).
//! Quando o proptest estiver disponível, estes casos migram para ele; a ideia é a mesma:
//! milhares de entradas aleatórias, uma verdade que nunca pode quebrar.

use zen_core::alerts::{evaluate, AlertConfig, AlertKind, CharSnapshot};
use zen_core::chat::{parse_line, ChatConfig};
use zen_core::pk::PkDeduper;
use zen_core::rate::ZenTracker;
use zen_core::title;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn text(&mut self, max: usize) -> String {
        const POOL: &[char] = &[
            'a', 'Z', ' ', ':', '|', '[', ']', 'k', 'i', 'l', 'e', 'd', '0', '9', ',', '.', 'P', 'K', '\n', '\u{0}',
            'é', '<', '>', '"', '\u{202e}',
        ];
        let n = self.below(max as u64) as usize;
        (0..n).map(|_| POOL[self.below(POOL.len() as u64) as usize]).collect()
    }
}

#[test]
fn zen_por_hora_nunca_negativo_e_bau_nunca_vira_farm() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..300 {
        let mut z = ZenTracker::new(3600);
        let mut zen: u64 = r.below(2_000_000_000);
        let mut t = 0;
        let mut farm_total = 0u64;
        for _ in 0..200 {
            t += 5;
            match r.below(10) {
                0 => zen = zen.saturating_sub(r.below(500_000_000)), // gasto
                1 => zen += 30_000_001 + r.below(500_000_000),       // baú
                _ => {
                    let g = r.below(2_000_000);
                    zen += g;
                    farm_total += g;
                }
            }
            z.push(t, zen);
            let rate = z.per_hour(t);
            assert!(rate >= 0.0 && rate.is_finite());
        }
        // A soma usada no Zen/h nunca passa do farm real.
        let span = (t - 5).clamp(60, 3600) as f64; // primeira leitura em t=5
        assert!(z.per_hour(t) * span / 3600.0 <= farm_total as f64 + 1.0);
    }
}

#[test]
fn mesmo_pk_dez_vezes_conta_um() {
    let mut r = Rng(42);
    for _ in 0..200 {
        let mut d = PkDeduper::new(1800);
        let base = r.below(1_000_000);
        let accepted = (0..10)
            .filter(|i| d.accept("Killer", "Victim", base + i * r.below(170)))
            .count();
        assert_eq!(accepted, 1);
    }
}

#[test]
fn parsers_nunca_entram_em_panico() {
    let mut r = Rng(7);
    let cfg = ChatConfig::default();
    for _ in 0..50_000 {
        let s = r.text(200);
        let _ = parse_line(&s, &cfg);
        let _ = parse_line(&format!("[PK] : {s}"), &cfg);
        let _ = parse_line(&format!("{s} : [PickItem] {s}"), &cfg);
        let t = title::parse(&s);
        if let Some(n) = t.name {
            assert!(!n.contains(' ') && n.chars().count() <= 16);
        }
    }
}

#[test]
fn offline_e_sempre_sozinho() {
    let mut r = Rng(99);
    let cfg = AlertConfig::default();
    for _ in 0..20_000 {
        let now = 10_000 + r.below(10_000);
        let s = CharSnapshot {
            last_report_at: if r.below(5) == 0 {
                None
            } else {
                Some(now.saturating_sub(r.below(60)))
            },
            disconnected: r.below(2) == 0,
            zen: if r.below(4) == 0 {
                None
            } else {
                Some(r.below(2_100_000_000))
            },
            zen_per_hour: r.below(500_000_000) as f64,
            idle_secs: Some(r.below(1000)),
            last_death_at: Some(now.saturating_sub(r.below(2000))),
        };
        let a = evaluate(&s, now, &cfg);
        if a.contains(&AlertKind::Offline) {
            assert_eq!(a, vec![AlertKind::Offline]);
        }
        // Lista sempre ordenada por prioridade e sem repetição.
        assert!(a.windows(2).all(|w| w[0] < w[1]));
        // 2 BI e PAROU nunca juntos; 2 BI e QUASE 2 BI nunca juntos.
        assert!(!(a.contains(&AlertKind::Limit2B) && a.contains(&AlertKind::Stopped)));
        assert!(!(a.contains(&AlertKind::Limit2B) && a.contains(&AlertKind::Near2B)));
    }
}
