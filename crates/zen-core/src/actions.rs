//! Ações de teclado/mouse que o agente pode fazer num char. LISTA FECHADA: o central e o
//! painel só escolhem uma ação pelo nome; a sequência de teclas é fixa aqui. Nada de texto
//! livre, nada de tecla vinda da rede. Ver docs/decisoes/0011.

/// Ações conhecidas. Cada variante nova precisa de sequência fixa em `steps` e teste.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ActionKind {
    /// Tecla V: abre/fecha o inventário (semente da trava do Zen).
    ToggleInventory,
    /// Recuperação completa até o spot salvo (M -> Kanturu se preciso, helper off, caminhada,
    /// helper on). Não é sequência fixa de teclas: o agente executa o fluxo testado.
    ReturnToSpot,
    /// Viagem da Fortuna do Imperador agora (Lorencia, Lugard, K -> J) e volta ao spot.
    Fortune,
    /// Igual, mas converte também todo o Zen do baú (menos a reserva de 1B da aba 1).
    FortuneAll,
    /// Só K -> inventário -> J (Deposit All), onde o char estiver, com prints de cada passo.
    FortuneBank,
    /// Dark Raven: Trainer de Lorencia (Restore life com Zen) e volta ao spot.
    RestoreRaven,
    /// Elpis (Refinery Tower): separa e refina Gemstones em Jewel of Harmony, em lotes, no
    /// lugar onde o char está (o dono leva o char até a Elpis).
    RefineGemstones,
}

impl ActionKind {
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::ToggleInventory => "abrir/fechar inventário (V)",
            ActionKind::ReturnToSpot => "voltar ao spot",
            ActionKind::Fortune => "Fortuna do Imperador agora",
            ActionKind::FortuneAll => "Fortuna: converter todo o baú",
            ActionKind::FortuneBank => "Fortuna: guardar K no J (aqui)",
            ActionKind::RestoreRaven => "restaurar o Dark Raven (Trainer)",
            ActionKind::RefineGemstones => "Elpis: Gemstone -> Jewel of Harmony",
        }
    }
}

/// Um passo. Coordenadas de clique em pixels da JANELA INTEIRA (as mesmas dos prints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Tecla virtual do Windows (VK_*), apertar e soltar.
    Key(u16),
    Click { x: i32, y: i32 },
    /// Botão direito (no MU: usa a skill atual no ponto do cursor).
    RightClick { x: i32, y: i32 },
    /// Botão direito SEGURADO por `ms` (a skill repete enquanto segura).
    RightHold { x: i32, y: i32, ms: u32 },
    WaitMs(u32),
    /// Só leva o cursor ao ponto, sem clicar (tooltip de um botão, ex.: custo do move no M).
    Hover { x: i32, y: i32 },
    /// Roda do mouse no ponto (-120 = um "dente" para baixo). Lista da Pick Window (16/11), cuja
    /// barra de rolagem não anda com clique e arraste.
    Wheel { x: i32, y: i32, delta: i32 },
}

pub const VK_V: u16 = 0x56;

pub fn steps(kind: ActionKind) -> Vec<Step> {
    match kind {
        ActionKind::ToggleInventory => vec![Step::Key(VK_V)],
        // Fluxo com leitura de tela a cada passo: fica no agente (win::nav), não aqui.
        ActionKind::ReturnToSpot | ActionKind::Fortune | ActionKind::FortuneAll | ActionKind::FortuneBank | ActionKind::RestoreRaven | ActionKind::RefineGemstones => vec![],
    }
}

/// Abertura automática do inventário para travar o Zen. Pura: o agente informa o estado e
/// executa o que ela pedir.
///
/// - Zen sem trava há `wait_s` e inventário fechado e PC sem uso há `idle_min_s` -> abre.
/// - Trava fechou e fomos NÓS que abrimos -> fecha (também só com o PC sem uso).
/// - Ficou aberto `max_open_s` sem travar -> fecha e espera `cooldown_s` antes de tentar de novo.
/// - Você mesmo fechou ou abriu: respeita, não briga com você.
#[derive(Debug, Clone, Default)]
pub struct AutoInventory {
    need_since: Option<u64>,
    opened_at: Option<u64>,
    cooldown_until: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct AutoInvConfig {
    pub wait_s: u64,
    pub idle_min_s: u64,
    pub max_open_s: u64,
    pub cooldown_s: u64,
}

impl Default for AutoInvConfig {
    fn default() -> Self {
        Self { wait_s: 90, idle_min_s: 60, max_open_s: 15 * 60, cooldown_s: 10 * 60 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvDecision {
    Nothing,
    Open,
    Close,
}

impl AutoInventory {
    pub fn decide(&mut self, now: u64, zen_locked: bool, inv_visible: bool, idle_s: u64, c: &AutoInvConfig) -> InvDecision {
        let idle = idle_s >= c.idle_min_s;
        if let Some(t) = self.opened_at {
            if !inv_visible {
                // Fechado por você (ou nunca abriu de fato): esquece e respeita o intervalo.
                self.opened_at = None;
                self.cooldown_until = self.cooldown_until.max(now + c.cooldown_s.min(120));
                return InvDecision::Nothing;
            }
            if zen_locked {
                if idle {
                    self.opened_at = None;
                    self.need_since = None;
                    return InvDecision::Close;
                }
                return InvDecision::Nothing;
            }
            if now.saturating_sub(t) >= c.max_open_s && idle {
                self.opened_at = None;
                self.cooldown_until = now + c.cooldown_s;
                return InvDecision::Close;
            }
            return InvDecision::Nothing;
        }
        if zen_locked || inv_visible {
            self.need_since = None;
            return InvDecision::Nothing;
        }
        let since = *self.need_since.get_or_insert(now);
        if now.saturating_sub(since) >= c.wait_s && idle && now >= self.cooldown_until {
            self.opened_at = Some(now);
            return InvDecision::Open;
        }
        InvDecision::Nothing
    }

    /// A tecla não chegou (foco não veio, etc.): desfaz a marcação e espera um pouco.
    pub fn failed(&mut self, now: u64) {
        self.opened_at = None;
        self.cooldown_until = now + 120;
    }

    pub fn opened_by_us(&self) -> bool {
        self.opened_at.is_some()
    }
}

/// Recorte do ícone do helper (coordenadas da janela inteira), à direita do nome do mapa.
pub const HELPER_ICON: [i32; 4] = [244, 30, 36, 24];
/// Limiar do ícone: menor canal RGB >= 120 (medido em 08/11/2026).
pub const HELPER_WHITE: u8 = 120;

/// Estado do helper pelo ícone: `Some(true)` = pause (helper LIGADO), `Some(false)` = play
/// (DESLIGADO), `None` = não reconheci (não aperte Home às cegas).
/// Pause: duas barras verticais (colunas 17-18 e 24-25) com o meio vazio.
/// Play: triângulo, com tinta no meio (colunas 20-22).
pub fn helper_on(b: &crate::glyphs::Bits) -> Option<bool> {
    let ink = |x: isize| (3..14).filter(|&y| b.get(x, y)).count();
    let left = ink(17).min(ink(18));
    let right = ink(24).min(ink(25));
    let middle = ink(20) + ink(21) + ink(22);
    if left >= 9 && right >= 9 && middle <= 1 {
        return Some(true);
    }
    if middle >= 12 && right <= 3 && ink(17) >= 6 {
        return Some(false);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use InvDecision::*;

    const C: AutoInvConfig = AutoInvConfig { wait_s: 90, idle_min_s: 60, max_open_s: 900, cooldown_s: 600 };

    #[test]
    fn abre_depois_da_espera_e_so_com_pc_parado_e_fecha_quando_trava() {
        let mut a = AutoInventory::default();
        assert_eq!(a.decide(0, false, false, 999, &C), Nothing);
        assert_eq!(a.decide(89, false, false, 999, &C), Nothing);
        assert_eq!(a.decide(90, false, false, 5, &C), Nothing, "você está usando o PC");
        assert_eq!(a.decide(91, false, false, 70, &C), Open);
        assert_eq!(a.decide(95, false, true, 70, &C), Nothing, "aberto, esperando travar");
        assert_eq!(a.decide(200, true, true, 10, &C), Nothing, "travou mas você está no PC");
        assert_eq!(a.decide(260, true, true, 80, &C), Close);
        assert_eq!(a.decide(300, true, false, 80, &C), Nothing);
    }

    #[test]
    fn inventario_aberto_por_voce_nunca_e_fechado() {
        let mut a = AutoInventory::default();
        for t in 0..2000 {
            assert_eq!(a.decide(t, t > 1000, true, 999, &C), Nothing);
        }
    }

    #[test]
    fn nao_trava_em_15_min_fecha_e_espera() {
        let mut a = AutoInventory::default();
        a.decide(0, false, false, 999, &C);
        assert_eq!(a.decide(90, false, false, 999, &C), Open);
        assert_eq!(a.decide(989, false, true, 999, &C), Nothing);
        assert_eq!(a.decide(990, false, true, 999, &C), Close);
        assert_eq!(a.decide(991, false, false, 999, &C), Nothing);
        assert_eq!(a.decide(1500, false, false, 999, &C), Nothing, "intervalo de 10 min");
        assert_eq!(a.decide(1590, false, false, 999, &C), Open);
    }

    #[test]
    fn voce_fechou_respeita() {
        let mut a = AutoInventory::default();
        a.decide(0, false, false, 999, &C);
        assert_eq!(a.decide(90, false, false, 999, &C), Open);
        assert_eq!(a.decide(100, false, false, 999, &C), Nothing);
        assert!(!a.opened_by_us());
        assert_eq!(a.decide(150, false, false, 999, &C), Nothing, "espera 2 min");
        assert_eq!(a.decide(220, false, false, 999, &C), Open);
    }

    #[test]
    fn sequencias_fixas() {
        assert_eq!(steps(ActionKind::ToggleInventory), vec![Step::Key(VK_V)]);
    }
}
