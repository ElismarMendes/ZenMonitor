//! Contrato entre agente, central e painel. Versionado: o central aceita
//! `PROTOCOL_MIN..=PROTOCOL_VERSION`. Mudou um campo de forma incompatível -> sobe a versão.

use crate::actions::ActionKind;
use crate::alerts::AlertKind;
use crate::chat::ChatEvent;
use crate::title::TitleInfo;

pub const PROTOCOL_VERSION: u32 = 3;
pub const PROTOCOL_MIN: u32 = 1;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ZenSource {
    /// Endereço travado e conferido.
    Ram,
    /// Só OCR (inventário aberto, trava em andamento).
    Ocr,
    /// Não confiável agora; o painel mostra "—".
    Unknown,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct CharReport {
    #[cfg_attr(feature = "serde", serde(default))]
    pub primary_retry_at: Option<u64>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub home_spot: Option<crate::spots::Spot>,
    pub pid: u32,
    pub title: TitleInfo,
    pub zen: Option<u64>,
    pub zen_source: ZenSource,
    /// Estado da trava, para a aba Saúde ("travado", "filtrando 12", "precisa de OCR"...).
    pub lock_status: String,
    pub tcp_connected: bool,
    /// true depois de 2 ciclos seguidos sem conexão TCP externa.
    pub disconnected: bool,
    /// HP lido na memória (endereço travado). `None` enquanto a trava da vida não fecha.
    #[cfg_attr(feature = "serde", serde(default))]
    pub hp: Option<u32>,
    /// Estado da trava da vida ("vida travada 0x...", "vida: filtrando 3"...).
    #[cfg_attr(feature = "serde", serde(default))]
    pub hp_status: String,
    /// Automação/recuperação deste char ("spot (160,197)", "recuperando: andando"...).
    #[cfg_attr(feature = "serde", serde(default))]
    pub auto_status: String,
    /// Tile atual lido na tela, só quando o mapa é o Kanturu 1 (o único com mini-mapa).
    /// v4: em qualquer mapa conhecido, junto com `map`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub pos: Option<(i32, i32)>,
    /// v4: nome do mapa atual lido no canto ("Kanturu", "Lorencia", "Arena"); None = outro.
    #[cfg_attr(feature = "serde", serde(default))]
    pub map: Option<String>,
    /// v4: mapa do spot ("Kanturu" ou "Arena").
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot_map: Option<String>,
    /// Spot de retorno deste char (F1 no jogo ou "Usar este spot" no painel).
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot: Option<(i32, i32)>,
    /// Volta sozinho ao spot depois de morrer (PK ou monstro). `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub return_on_death: Option<bool>,
    /// Deposita no baú sozinho ao chegar a 1,99 bi. `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub auto_deposit: Option<bool>,
    /// Char travado no spot (não troca nem exclui o spot). `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot_locked: Option<bool>,
    /// Automação ligada PARA ESTE CHAR (volta ao spot, baú, tecla V). `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub automation: Option<bool>,
    /// Tecla da skill de limpar mobs no caminho (0 = sem, usa o helper). `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub skill_key: Option<u8>,
    /// Vida do Dark Raven em % (barra no topo da tela). `None` = sem Raven ou não leu.
    #[cfg_attr(feature = "serde", serde(default))]
    pub pet_hp: Option<u8>,
    /// Mobs, NPCs e jogadores em volta (memória do jogo, só leitura). `None` = sem leitura.
    #[cfg_attr(feature = "serde", serde(default))]
    pub nearby: Option<Vec<crate::objects::Obj>>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Health {
    pub cpu_percent: f32,
    pub mem_mb: f32,
    pub uptime_s: u64,
    pub last_error: Option<String>,
    pub ram_paused: bool,
    pub os: String,
    /// Automação de teclado/mouse ligada neste PC.
    #[cfg_attr(feature = "serde", serde(default))]
    pub automation: bool,
    /// Regras da volta automática deste PC (painel). `None` = agente antigo.
    #[cfg_attr(feature = "serde", serde(default))]
    pub death_return: Option<DeathReturn>,
    /// Trava manual ligada neste PC (pausa por F8): automação pausada.
    #[cfg_attr(feature = "serde", serde(default))]
    pub manual: bool,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum AgentMsg {
    NavigationSnapshot { snapshot: crate::routes::RadarSnapshot },
    RouteResult { id:u64, ok:bool, message:String },
    SpotLease { request_id: u64, char_name: String, spot: crate::spots::Spot, release: bool },
    /// Primeira mensagem em `/ws/pair` (sem token): pede um token para esta máquina.
    /// `install_id` é gerado uma vez por instalação e impede outro PC de "roubar" o nome.
    Pair {
        protocol: u32,
        machine: String,
        install_id: String,
    },
    Hello {
        protocol: u32,
        agent_version: String,
        machine: String,
        channel: String,
    },
    Report {
        seq: u64,
        sent_at: u64,
        chars: Vec<CharReport>,
        events: Vec<ChatEvent>,
        health: Health,
        /// Spots catalogados (F2) no Kanturu 1, NA ORDEM do catálogo.
        #[cfg_attr(feature = "serde", serde(default))]
        catalog: Vec<(i32, i32)>,
        /// Spots catalogados (F2) nos outros mapas: (World, x, y).
        #[cfg_attr(feature = "serde", serde(default))]
        catalog_other: Vec<(u8, i32, i32)>,
    },
    CommandResult {
        id: u64,
        ok: bool,
        message: String,
    },
    /// Print de um char (PNG em base64), resposta ao comando `Screenshot`.
    Screenshot {
        id: u64,
        char_name: String,
        png_base64: String,
    },
    Logs {
        id: u64,
        text: String,
    },
    /// O HP do char foi a 0 (lido na memória e conferido na tela). Vai com o print da janela.
    Death {
        char_name: String,
        at: u64,
        /// HP na última leitura antes do zero.
        hp_before: u32,
        /// O que a tela mostrava no print (`Some(0)` = confirmado; `None` = não deu para ler).
        screen_hp: Option<u32>,
        png_base64: Option<String>,
    },
}

/// LISTA FECHADA. Não existe variante que carregue comando de shell ou caminho de arquivo.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", serde(tag = "cmd", rename_all = "snake_case"))]
pub enum Command {
    RunRoute { char_name:String, route:crate::routes::RouteDefinition, context:crate::routes::ObserverContext, install_for_recovery:bool },
    RevokeRoute { route_id:String },
    SyncRoutes { active_ids:Vec<String> },
    CheckUpdate,
    RestartAgent,
    RelockRam {
        char_name: String,
    },
    Screenshot {
        char_name: String,
    },
    SendLogs,
    SetConfig {
        stopped_after_s: Option<u64>,
        chat_enabled: Option<bool>,
        /// Liga/desliga a automação de teclado/mouse no PC (interruptor de pânico remoto).
        #[cfg_attr(feature = "serde", serde(default))]
        automation_enabled: Option<bool>,
        /// Experimento de rotas: opção explícita, independente da automação legada.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        navigation_enabled: Option<bool>,
    },
    /// Grava a posição ATUAL do char (lida na tela, tem de ser Kanturu) como o spot dele.
    SaveSpot {
        char_name: String,
    },
    /// Define o spot de retorno de um char (um spot do catálogo, escolhido no painel).
    AssignSpot {
        char_name: String,
        x: i32,
        y: i32,
    },
    /// Regras da volta automática (todas as recuperações): no máximo `max_returns` em
    /// `window_min` minutos; depois de morrer no spot espera um tempo sorteado entre
    /// `delay_min_s` e `delay_max_s`.
    SetDeathReturn {
        max_returns: u32,
        window_min: u64,
        delay_min_s: u64,
        delay_max_s: u64,
    },
    /// Liga/desliga TODA a automação de um char (ex.: char num mapa que o agente ainda não conhece).
    SetCharAutomation {
        char_name: String,
        enabled: bool,
    },
    /// Tecla da skill de área para limpar mobs no caminho (0 = nenhuma, usa o helper).
    SetSkillKey {
        char_name: String,
        key: u8,
    },
    /// Trava/destrava um char no spot dele.
    SetSpotLock {
        char_name: String,
        locked: bool,
    },
    /// Liga/desliga o depósito automático no baú, para UM char.
    SetAutoDeposit {
        char_name: String,
        enabled: bool,
    },
    /// Liga/desliga a volta automática ao spot depois de morrer, para UM char.
    SetReturnOnDeath {
        char_name: String,
        enabled: bool,
    },
    /// Tira um spot do catálogo (e dos chars que voltavam para ele).
    DeleteSpot {
        x: i32,
        y: i32,
        /// World do spot (None = Kanturu, painel antigo).
        #[cfg_attr(feature = "serde", serde(default))]
        world: Option<u8>,
    },
    /// v4 (mapas): leva o char até (x, y) no World `world` pelo M + caminhada (só mapas
    /// validados). `make_spot`: grava como o spot dele; senão o char fica lá (sem voltar sozinho)
    /// até "Voltar ao spot".
    GoTo {
        char_name: String,
        world: u8,
        x: i32,
        y: i32,
        #[cfg_attr(feature = "serde", serde(default))]
        make_spot: bool,
    },
    /// Executa UMA ação da lista fechada (`ActionKind`) num char. Sem teclas livres.
    RunAction {
        char_name: String,
        action: ActionKind,
    },
    PauseRam,
    ResumeRam,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum CentralMsg {
    SpotDirectory { directory: crate::spots::Directory },
    SpotLeaseResult { request_id: u64, accepted: bool, reason: String },
    Welcome {
        protocol: u32,
        server_time: u64,
    },
    /// Resposta ao `Pair`: o token desta máquina.
    Paired {
        machine: String,
        token: String,
    },
    /// Renovação automática: o agente grava e usa este token na próxima conexão.
    NewToken {
        token: String,
    },
    Command {
        id: u64,
        command: Command,
    },
    Reject {
        reason: String,
    },
}

/// O que o painel recebe (estado já calculado pelo central).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PanelChar {
    #[cfg_attr(feature = "serde", serde(default))]
    pub home_spot: Option<crate::spots::Spot>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub primary_retry_at: Option<u64>,
    pub machine: String,
    pub name: String,
    pub level: Option<u32>,
    pub resets: Option<u32>,
    pub zen: Option<u64>,
    pub zen_source: ZenSource,
    pub zen_per_hour: f64,
    pub idle_secs: Option<u64>,
    pub eta_limit_s: Option<u64>,
    pub alerts: Vec<AlertKind>,
    /// Últimas 3 h, 1 ponto/min: (hora, zen).
    pub spark: Vec<(u64, u64)>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub hp: Option<u32>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub last_death_at: Option<u64>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub auto_status: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub pos: Option<(i32, i32)>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub map: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot_map: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot: Option<(i32, i32)>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub return_on_death: Option<bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub auto_deposit: Option<bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub spot_locked: Option<bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub automation: Option<bool>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub skill_key: Option<u8>,
    /// Vida do Dark Raven em % (barra no topo da tela). `None` = sem Raven ou não leu.
    #[cfg_attr(feature = "serde", serde(default))]
    pub pet_hp: Option<u8>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub nearby: Option<Vec<crate::objects::Obj>>,
}

/// Endereço de pareamento a partir do endereço do agente: `.../ws/agent` -> `.../ws/pair`.
pub fn pair_url(agent_url: &str) -> Option<String> {
    let base = agent_url.trim().trim_end_matches('/');
    let base = base.strip_suffix("/ws/agent")?;
    if !(base.starts_with("ws://") || base.starts_with("wss://")) {
        return None;
    }
    Some(format!("{base}/ws/pair"))
}

/// Nome de máquina aceito no pareamento: 1..=32 de [A-Za-z0-9_-].
pub fn valid_machine_name(s: &str) -> bool {
    (1..=32).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn version_accepted(v: u32) -> bool {
    (PROTOCOL_MIN..=PROTOCOL_VERSION).contains(&v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn url_de_pareamento() {
        assert_eq!(
            super::pair_url("ws://192.168.0.10:8787/ws/agent").as_deref(),
            Some("ws://192.168.0.10:8787/ws/pair")
        );
        assert_eq!(super::pair_url("wss://x/ws/agent/").as_deref(), Some("wss://x/ws/pair"));
        assert_eq!(super::pair_url("http://x/ws/agent"), None);
        assert_eq!(super::pair_url("ws://x/outra"), None);
    }

    #[test]
    fn nomes_de_maquina() {
        assert!(super::valid_machine_name("DESKTOP-VJHNVN4"));
        assert!(!super::valid_machine_name(""));
        assert!(!super::valid_machine_name("PC 1"));
        assert!(!super::valid_machine_name("<script>"));
        assert!(!super::valid_machine_name(&"a".repeat(33)));
    }

    #[test]
    fn versoes() {
        assert!(super::version_accepted(1));
        assert!(!super::version_accepted(0));
        assert!(super::version_accepted(2));
        assert!(super::version_accepted(3));
        assert!(!super::version_accepted(4));
    }
}

/// Regras da volta automática, como aparecem no painel.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathReturn {
    pub max_returns: u32,
    pub window_min: u64,
    pub delay_min_s: u64,
    pub delay_max_s: u64,
}

impl DeathReturn {
    /// Limites aceitos (o painel e o agente conferem): evita "0 voltas" ou espera de horas.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=20).contains(&self.max_returns) {
            return Err("voltas: de 1 a 20".into());
        }
        if !(1..=240).contains(&self.window_min) {
            return Err("janela: de 1 a 240 minutos".into());
        }
        if self.delay_max_s > 1800 || self.delay_min_s > self.delay_max_s {
            return Err("espera: mínimo <= máximo, e máximo até 1800 s (30 min)".into());
        }
        Ok(())
    }
}
