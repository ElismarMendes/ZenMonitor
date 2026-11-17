//! Parser das linhas globais do chat.
//!
//! Formatos confirmados:
//! - `[PK] : <quem matou> killed <vítima>`   (quem matou pode ser monstro, com espaço e cortado)
//! - `<char> : [PickItem] Jewel of Soul`
//! - `[PickItem] Jewel of Soul`              (sem nome = cópia local; NÃO conta)
//!
//! Aviso importante: este parser só diz O QUE uma linha significa. Ele não diz se a
//! linha é um evento NOVO. Cada linha existe em 5 a 11 cópias na RAM; deduplicar é
//! problema de quem lê (ver `pk.rs` e docs/decisoes/0004-joias-desligadas.md).

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KillerKind {
    Player,
    Monster,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Jewel {
    Bless,
    Soul,
    Chaos,
    Life,
    Creation,
    Guardian,
    EmperorsFortune,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Pk {
        killer: String,
        killer_kind: KillerKind,
        victim: String,
    },
    Pick {
        char_name: String,
        jewel: Jewel,
    },
}

/// Configuração do parser. Fica fora do código porque o cliente mistura idiomas.
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Monstros de UMA palavra (os de várias palavras já são reconhecidos pelo espaço).
    /// Cuidado: um player pode ter exatamente esse nome. Ver docs/decisoes/0003.
    pub one_word_monsters: Vec<String>,
    /// (texto exato depois de "[PickItem] ", joia). Comparação sem diferenciar maiúsculas.
    pub jewel_names: Vec<(String, Jewel)>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        let jewels = [
            ("Jewel of Bless", Jewel::Bless),
            ("Jewel of Soul", Jewel::Soul),
            ("Jewel of Chaos", Jewel::Chaos),
            ("Jewel of Life", Jewel::Life),
            ("Jewel of Creation", Jewel::Creation),
            ("Jewel of Guardian", Jewel::Guardian),
            ("Fortuna do Imperador", Jewel::EmperorsFortune),
            ("Emperor's Fortune", Jewel::EmperorsFortune),
        ];
        Self {
            one_word_monsters: [
                "Spider", "Lich", "Balrog", "Hydra", "Yeti", "Gorgon", "Medusa", "Kundun", "Valkyrie", "Worm",
                "Bahamut", "Genocider",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            jewel_names: jewels.iter().map(|(s, j)| (s.to_string(), *j)).collect(),
        }
    }
}

const MAX_LINE: usize = 160;
const MAX_NAME: usize = 32;

pub fn parse_line(line: &str, cfg: &ChatConfig) -> Option<ChatEvent> {
    let line: String = line.chars().take(MAX_LINE).filter(|c| !c.is_control()).collect();
    let line = line.trim();

    if let Some(rest) = line.strip_prefix("[PK]") {
        return parse_pk(rest, cfg);
    }

    // "<char> : [PickItem] ..."  — exige nome antes; "[PickItem] ..." sozinho é local.
    let (name, rest) = line.split_once(" : ")?;
    let item = rest.trim().strip_prefix("[PickItem]")?.trim();
    let name = name.trim();
    if !is_player_name(name) {
        return None;
    }
    if item.to_ascii_lowercase().starts_with("enchanted ") {
        return None;
    }
    let jewel = cfg
        .jewel_names
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(item))
        .map(|(_, j)| *j)?;
    Some(ChatEvent::Pick {
        char_name: name.to_string(),
        jewel,
    })
}

fn parse_pk(rest: &str, cfg: &ChatConfig) -> Option<ChatEvent> {
    let rest = rest.trim_start().strip_prefix(':')?.trim();
    // A vítima é a ÚLTIMA palavra (player não tem espaço); o resto antes de " killed " é
    // quem matou. rsplit evita confundir monstro com "killed" no nome.
    let (killer, victim) = rest.rsplit_once(" killed ")?;
    let killer = crate::sanitize(killer, MAX_NAME);
    let victim = victim.trim();
    if killer.is_empty() || !is_player_name(victim) {
        return None;
    }
    let kind = if killer.contains(' ') || cfg.one_word_monsters.iter().any(|m| m.eq_ignore_ascii_case(&killer)) {
        KillerKind::Monster
    } else {
        KillerKind::Player
    };
    Some(ChatEvent::Pk {
        killer,
        killer_kind: kind,
        victim: victim.to_string(),
    })
}

/// Nome de player: sem espaço, 1..=16 chars, sem colchetes.
pub fn is_player_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=16).contains(&n) && !s.chars().any(|c| c.is_whitespace() || c == '[' || c == ']' || c == ':')
}

/// Quem matou `victim`, a partir das linhas `[PK] : X killed <victim>` lidas na memória do
/// cliente logo depois da morte. `known` = linhas (endereço, texto) já vistas antes (mortes
/// anteriores): só contam as novas, porque a mesma linha velha fica minutos na RAM.
/// Várias novas com quem-matou diferente: vence a que tem mais cópias; empate = player
/// (o lado seguro: na dúvida, não volta sozinho).
pub fn death_cause(
    lines: &[(u64, String)],
    victim: &str,
    known: &std::collections::HashSet<(u64, String)>,
    cfg: &ChatConfig,
) -> Option<(String, KillerKind)> {
    let mut count: Vec<((String, KillerKind), usize)> = Vec::new();
    for (a, t) in lines {
        if known.contains(&(*a, t.clone())) {
            continue;
        }
        let Some(ChatEvent::Pk { killer, killer_kind, victim: v }) = parse_line(t, cfg) else { continue };
        if !v.eq_ignore_ascii_case(victim) {
            continue;
        }
        match count.iter_mut().find(|(k, _)| k.0 == killer && k.1 == killer_kind) {
            Some((_, n)) => *n += 1,
            None => count.push(((killer, killer_kind), 1)),
        }
    }
    count.sort_by(|a, b| b.1.cmp(&a.1).then((a.0 .1 == KillerKind::Monster).cmp(&(b.0 .1 == KillerKind::Monster))));
    count.into_iter().next().map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<ChatEvent> {
        parse_line(s, &ChatConfig::default())
    }

    #[test]
    fn pk_player() {
        assert_eq!(
            p("[PK] : Assassino killed ReDMooN"),
            Some(ChatEvent::Pk {
                killer: "Assassino".into(),
                killer_kind: KillerKind::Player,
                victim: "ReDMooN".into()
            })
        );
    }

    #[test]
    fn pk_monstro_cortado() {
        for m in ["Golden Saty", "Death Rider", "Canon Trap"] {
            let ev = p(&format!("[PK] : {m} killed ReDMooN")).unwrap();
            assert!(
                matches!(
                    ev,
                    ChatEvent::Pk {
                        killer_kind: KillerKind::Monster,
                        ..
                    }
                ),
                "{m}"
            );
        }
    }

    #[test]
    fn pk_monstro_de_uma_palavra() {
        let ev = p("[PK] : Balrog killed ReDMooN").unwrap();
        assert!(matches!(
            ev,
            ChatEvent::Pk {
                killer_kind: KillerKind::Monster,
                ..
            }
        ));
    }

    #[test]
    fn pk_malformado() {
        assert_eq!(p("[PK] : killed ReDMooN"), None);
        assert_eq!(p("[PK] Assassino killed ReDMooN"), None);
        assert_eq!(p("[PK] : Assassino killed Re D"), None);
        assert_eq!(p("[PK] : Assassino matou ReDMooN"), None);
    }

    #[test]
    fn joias() {
        assert_eq!(
            p("ReDMooN : [PickItem] Jewel of Soul"),
            Some(ChatEvent::Pick {
                char_name: "ReDMooN".into(),
                jewel: Jewel::Soul
            })
        );
        assert_eq!(
            p("ReDMooN : [PickItem] Fortuna do Imperador"),
            Some(ChatEvent::Pick {
                char_name: "ReDMooN".into(),
                jewel: Jewel::EmperorsFortune
            })
        );
    }

    #[test]
    fn joias_ignoradas() {
        assert_eq!(p("[PickItem] Jewel of Soul"), None); // cópia local
        assert_eq!(p("ReDMooN : [PickItem] Enchanted Jewel Of Soul"), None);
        assert_eq!(p("ReDMooN : [PickItem] Box of Luck"), None);
        assert_eq!(p("Re D : [PickItem] Jewel of Soul"), None);
    }

    fn known(v: &[(u64, &str)]) -> std::collections::HashSet<(u64, String)> {
        v.iter().map(|(a, t)| (*a, t.to_string())).collect()
    }

    #[test]
    fn causa_monstro_e_player() {
        let c = ChatConfig::default();
        let l = vec![(1, "[PK] : Golden Saty killed LOBOLIXO".to_string()), (2, "[PK] : Death Rider killed ReDMooN".into())];
        assert_eq!(death_cause(&l, "ReDMooN", &known(&[]), &c), Some(("Death Rider".into(), KillerKind::Monster)));
        let l = vec![(3, "[PK] : KceTinHoO killed ReDMooN".to_string()), (4, "[PK] : KceTinHoO killed redmoon".into())];
        assert_eq!(death_cause(&l, "ReDMooN", &known(&[]), &c), Some(("KceTinHoO".into(), KillerKind::Player)));
    }

    #[test]
    fn causa_ignora_linhas_velhas() {
        let c = ChatConfig::default();
        let l = vec![(3, "[PK] : KceTinHoO killed ReDMooN".to_string())];
        assert_eq!(death_cause(&l, "ReDMooN", &known(&[(3, "[PK] : KceTinHoO killed ReDMooN")]), &c), None);
        // mesmo texto em endereço novo = morte nova
        let l = vec![(3, "[PK] : KceTinHoO killed ReDMooN".to_string()), (9, "[PK] : KceTinHoO killed ReDMooN".into())];
        assert!(death_cause(&l, "ReDMooN", &known(&[(3, "[PK] : KceTinHoO killed ReDMooN")]), &c).is_some());
    }

    #[test]
    fn causa_empate_fica_com_player() {
        let c = ChatConfig::default();
        let l = vec![(1, "[PK] : Death Rider killed ReDMooN".to_string()), (2, "[PK] : KceTinHoO killed ReDMooN".into())];
        assert_eq!(death_cause(&l, "ReDMooN", &known(&[]), &c).unwrap().1, KillerKind::Player);
        assert_eq!(death_cause(&l, "x9901", &known(&[]), &c), None);
    }
}
