use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `agent.toml`, ao lado do .exe. Gravação atômica (arquivo temporário + rename).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Coordenação e fallback usam os leitores e o movimento existentes.
    pub spot_fallback: bool,
    /// Antigo (lista única por PC). v5: não é mais usado; ver `spot_maps`.
    pub spot_secondary_maps: Vec<String>,
    /// v5 (17/11): mapas extras que cada char pode usar quando o spot dele está ocupado, além do
    /// mapa do próprio spot (F1), em ordem de preferência. Char fora da lista = só o mapa do F1.
    /// Ex.: `[spot_maps]` / `AMUMU = ["Dungeon"]`.
    pub spot_maps: std::collections::BTreeMap<String, Vec<String>>,
    pub spot_primary_retry_s: u64,
    /// Personagens próprios adicionais, mesmo sem agente conectado.
    pub friendly_chars: Vec<String>,

    /// Ex.: "ws://192.168.0.10:8787/ws/agent". TLS (wss) entra na fase 4.
    pub central_url: String,
    /// Token deste PC. Com `auto_token = true` é obtido e renovado sozinho; deixe vazio.
    pub token: String,
    /// AUTOMAÇÃO DE TOKEN. true: sem token, o agente se cadastra no central sozinho e grava
    /// o token aqui; recebe renovações; se o token for recusado, pede outro. false: só usa
    /// o token escrito à mão. Também por ZEN_AUTO_TOKEN=0/1 no ambiente ou no .env.
    pub auto_token: bool,
    /// Identidade desta instalação (gerada uma vez). Impede outro PC de assumir este nome.
    pub install_id: String,
    /// Nome que aparece no painel. Vazio = nome do computador.
    pub machine: String,
    pub channel: String,
    pub cycle_ms: u64,
    /// Classe da janela do jogo.
    pub window_class: String,
    /// Região do VALOR do Zen no inventário: [x, y, largura, altura], em coordenadas da
    /// JANELA INTEIRA (as do PNG do `zen-diag ocr`). Vazio = agente não trava.
    /// Padrão medido no cliente 1024x768 do The Classic (campo vermelho "797,124,492").
    pub zen_ocr_region: Vec<i32>,
    /// Recorte do NÚMERO DE HP (em cima da esfera vermelha), coordenadas da janela inteira.
    /// É a semente da trava da vida na memória e a conferência da morte. Vazio = sem vida
    /// e sem detecção de morte. Padrão medido no cliente 1030x797 (07/11/2026).
    pub hp_region: Vec<i32>,
    /// Só estes chars são lidos. Vazio = todos. Use para testar com o fake-client sem
    /// tocar no jogo de verdade: allowed_chars = ["Teste"].
    pub allowed_chars: Vec<String>,
    /// Interruptor de pânico: não abre o processo do jogo para ler RAM.
    pub ram_paused: bool,
    /// Leitura do chat por varredura. DESLIGADO por padrão: ver docs/decisoes/0005.
    pub chat_enabled: bool,
    /// AUTOMAÇÃO DE TECLADO/MOUSE (SendInput, janela em foco). Desligada por padrão.
    /// Liga só ações da lista fechada; ver docs/decisoes/0011. Painel: botão por PC.
    pub automation_enabled: bool,
    /// A automação só age com o PC sem uso há pelo menos isto (segundos). Ações pedidas
    /// pelo painel não esperam.
    pub automation_idle_s: u64,
    /// Campo legado; a pausa manual agora usa F8.
    #[serde(default = "default_manual_hold")]
    pub manual_hold_s: u64,
    /// Pausa manual acaba sozinha depois disto (s) sem tocar em mouse/teclado.
    #[serde(default = "default_manual_release")]
    pub manual_release_s: u64,
    /// Dark Raven (11/11): com a vida do Raven em `raven_min_pct`% ou menos, vai ao Trainer de
    /// Lorencia restaurar (Zen) e volta ao spot. Só para quem tem a barra do Raven na tela.
    #[serde(default = "default_true_cfg")]
    pub raven_auto: bool,
    #[serde(default = "default_raven_min")]
    pub raven_min_pct: u8,
    /// Abre o inventário (V) sozinho quando o Zen está sem trava e fecha depois de travar.
    pub auto_inventory: bool,
    /// Leva o char de volta ao spot (DC, morte, helper desligado). Precisa da automação ligada
    /// e de spot salvo. Ver docs/decisoes/0012.
    pub recovery_enabled: bool,
    /// Só para teste com o fake-client (que conecta em 127.0.0.1).
    pub allow_loopback_tcp: bool,
    /// Quantos relatórios guardar se o central cair (720 = 1 h com ciclo de 5 s).
    pub buffer_reports: usize,
    /// Zen chegou a 1,99 bi: vai ao baú do Kanturu, deposita tudo menos 40 mi e volta ao spot.
    pub auto_deposit: bool,
    /// Volta automática (painel): no máximo `return_max` recuperações em `return_window_min`
    /// minutos; depois de morrer no spot, espera um tempo sorteado entre os dois `return_delay`.
    pub return_max: u32,
    pub return_window_min: u64,
    /// Farmando (helper ligado) a mais de tantos tiles do spot = "longe do spot": o bot volta.
    /// Dono, 15/11: 10 está bom para quem já está no spot; 1-2 tiles vale só na chegada.
    pub spot_drift_tiles: i32,
    /// Liga a coleta de Zen na Pick Window (Y) toda vez que o char volta ao spot (dono, 16/11).
    pub zen_pick: bool,
    pub return_delay_min_s: u64,
    pub return_delay_max_s: u64,
    /// Chars SEM automação nenhuma (só monitorados): volta ao spot, baú e tecla V desligados.
    /// Para chars em mapas que o agente ainda não conhece.
    pub no_automation: Vec<String>,
    /// Chars travados no spot: o spot não troca (painel, F1) nem é excluído.
    pub locked_spots: Vec<String>,
    /// Chars que NÃO depositam sozinhos no baú (botão no painel, por char).
    pub no_auto_deposit: Vec<String>,
    /// Chars que NÃO voltam sozinhos ao spot depois de morrer (botão no painel, por char).
    pub no_return_on_death: Vec<String>,
    /// Chars cujo spot é na Arena (e não no Kanturu 1). A volta vai pelo M -> Stadium.
    #[serde(default)]
    pub arena: Vec<String>,
    /// Catálogo de spots do Kanturu 1 (F2 no jogo), NA ORDEM em que foram salvos.
    /// É a ordem usada para procurar um spot livre.
    pub catalog: Vec<[i32; 2]>,
    /// Tecla (1..9, 0) da skill de área de cada char (ex.: Ice Storm). Andando, se travar no
    /// meio dos mobs, o char aperta a tecla e SEGURA o botão direito em cima de si por alguns
    /// segundos para limpar, em vez de ligar o helper. Sem tecla = usa o helper como antes.
    #[serde(default)]
    pub skill_keys: std::collections::BTreeMap<String, u8>,
    /// Mapa do spot quando não é Kanturu nem Arena (11/11): nome -> "Lost Tower" / "Dungeon".
    #[serde(default)]
    pub spot_maps: std::collections::BTreeMap<String, String>,
    /// Catálogo de spots (F2) dos outros mapas: nome do mapa -> tiles, na ordem.
    #[serde(default)]
    pub catalog_maps: std::collections::BTreeMap<String, Vec<[i32; 2]>>,
    /// Spot de cada char no Kanturu 1: nome -> [x, y]. Gravado pelo botão "Salvar spot" do
    /// painel. Último campo de propósito: no TOML vira a tabela [spots], que tem de vir no fim.
    pub spots: std::collections::BTreeMap<String, [i32; 2]>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            spot_fallback: true,
            spot_secondary_maps: vec!["Arena".into(), "Lost Tower".into(), "Dungeon".into()],
            spot_primary_retry_s: 600,
            friendly_chars: Vec::new(),
            spot_maps: std::collections::BTreeMap::new(),

            central_url: "ws://127.0.0.1:8787/ws/agent".into(),
            token: String::new(),
            auto_token: true,
            install_id: String::new(),
            machine: String::new(),
            channel: "stable".into(),
            cycle_ms: 5000,
            window_class: "The Classic MU Season 2".into(),
            zen_ocr_region: vec![786, 609, 78, 14],
            hp_region: vec![165, 752, 100, 36],
            allowed_chars: vec![],
            ram_paused: false,
            chat_enabled: false,
            automation_enabled: false,
            automation_idle_s: 60,
            manual_hold_s: 5,
            manual_release_s: 180,
            raven_auto: true,
            raven_min_pct: 25,
            auto_inventory: true,
            recovery_enabled: true,
            allow_loopback_tcp: false,
            buffer_reports: 720,
            auto_deposit: true,
            return_max: 3,
            return_window_min: 10,
            spot_drift_tiles: 10,
            zen_pick: true,
            return_delay_min_s: 30,
            return_delay_max_s: 120,
            no_automation: Vec::new(),
            locked_spots: Vec::new(),
            no_auto_deposit: Vec::new(),
            no_return_on_death: Vec::new(),
            catalog: Vec::new(),
            arena: Vec::new(),
            skill_keys: std::collections::BTreeMap::new(),
            spot_maps: std::collections::BTreeMap::new(),
            catalog_maps: std::collections::BTreeMap::new(),
            spots: std::collections::BTreeMap::new(),
        }
    }
}

impl AgentConfig {
    pub fn dir() -> PathBuf {
        if let Ok(d) = std::env::var("ZEN_AGENT_DIR") {
            if !d.trim().is_empty() {
                return PathBuf::from(d);
            }
        }
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn path() -> PathBuf {
        Self::dir().join("agent.toml")
    }

    /// Lê o `.env` da pasta (sem sobrescrever o que já está no ambiente).
    pub fn load_env_file() {
        if let Ok(txt) = std::fs::read_to_string(Self::dir().join(".env")) {
            for (k, v) in zen_core::envfile::parse(&txt) {
                if std::env::var_os(&k).is_none() {
                    std::env::set_var(k, v);
                }
            }
        }
    }

    /// Modo de teste (`zen-agent.exe --teste`): só o char "Teste" do fake-client, TCP local
    /// aceito. Vale só para esta execução; não grava no arquivo.
    pub fn apply_test_mode(&mut self) {
        self.allowed_chars = vec!["Teste".into()];
        self.allow_loopback_tcp = true;
    }

    /// Grava só o token, preservando o resto do arquivo como está em disco.
    pub fn save_token(token: &str) -> anyhow::Result<()> {
        Self::edit(|c| c.token = token.to_string())
    }

    /// Arquivo + ambiente (ZEN_AUTO_TOKEN, ZEN_CENTRAL_URL). É o que o agente usa.
    pub fn load_or_create() -> anyhow::Result<Self> {
        let mut c = Self::load_file()?;
        if let Some(v) = std::env::var("ZEN_AUTO_TOKEN")
            .ok()
            .and_then(|v| zen_core::envfile::parse_bool(&v))
        {
            c.auto_token = v;
        }
        if let Ok(v) = std::env::var("ZEN_CENTRAL_URL") {
            if !v.trim().is_empty() {
                c.central_url = v.trim().to_string();
            }
        }
        Ok(c)
    }

    /// Só o arquivo, sem o ambiente. Usado para regravar sem "vazar" variáveis para o disco.
    pub fn load_file() -> anyhow::Result<Self> {
        let p = Self::path();
        if !p.exists() {
            Self::default().save()?;
        }
        let txt = std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("não consegui ler (salve o arquivo como UTF-8): {e}"))?;
        let mut c: Self = toml::from_str(&txt).map_err(|e| anyhow::anyhow!("erro de sintaxe no agent.toml: {e}"))?;
        if c.machine.trim().is_empty() {
            c.machine = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
        }
        // Nome aceito pelo central: letras, números, - e _ (até 32).
        c.machine = c
            .machine
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '-'
                }
            })
            .take(32)
            .collect();
        c.cycle_ms = c.cycle_ms.clamp(1000, 60_000);
        let mut dirty = false;
        if c.install_id.len() < 16 {
            c.install_id = new_install_id();
            dirty = true;
        }
        if dirty {
            c.save()?;
        }
        Ok(c)
    }

    /// Erros que impedem o agente de funcionar, com a mensagem que o usuário vai ler.
    pub fn validate(&self) -> Result<(), String> {
        if self.token.trim().is_empty() && !self.auto_token {
            return Err(
                "O campo token está vazio e a automação de token está desligada (auto_token = false). \
                 Copie o token desta máquina do central.toml ou ligue auto_token."
                    .into(),
            );
        }
        if !self.token.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!(
                "O token tem caracteres inválidos ({:?}). Ele deve ser só letras e números, \
                 copiado do central.toml, sem os sinais < >.",
                self.token
            ));
        }
        if !(self.hp_region.is_empty() || self.hp_region.len() == 4) {
            return Err("hp_region deve ter 4 números: [x, y, largura, altura] (ou [] para desligar).".into());
        }
        if !(self.zen_ocr_region.is_empty() || self.zen_ocr_region.len() == 4) {
            return Err("zen_ocr_region deve ter 4 números: [x, y, largura, altura].".into());
        }
        if !(self.central_url.starts_with("ws://") || self.central_url.starts_with("wss://")) {
            return Err(format!(
                "central_url deve começar com ws:// (está {:?}).",
                self.central_url
            ));
        }
        Ok(())
    }

    /// Serializa leitura + alteração + troca atômica entre as threads do agente.
    pub fn edit(f: impl FnOnce(&mut Self)) -> anyhow::Result<()> {
        static EDIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _edit = EDIT.lock().unwrap_or_else(|e| e.into_inner());
        let mut saved = Self::load_file()?;
        f(&mut saved);
        saved.save()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let p = Self::path();
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }
}

/// 32 caracteres hexadecimais aleatórios, sem dependência extra: as chaves do `RandomState`
/// vêm do gerador do sistema operacional.
fn new_install_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::AgentConfig;

    #[test]
    fn spots_vao_e_voltam_do_toml() {
        let mut c = AgentConfig::default();
        c.spots.insert("ReDMooN".into(), [160, 197]);
        c.spots.insert("x9901".into(), [123, 233]);
        c.catalog = vec![[30, 210], [123, 233], [160, 197]];
        let txt = toml::to_string_pretty(&c).unwrap();
        let back: AgentConfig = toml::from_str(&txt).unwrap();
        assert_eq!(back.spots, c.spots);
        assert_eq!(back.catalog, c.catalog, "a ordem do catálogo é a ordem de busca");
        assert!(back.recovery_enabled);
        // agent.toml antigo, sem os campos novos, continua abrindo
        let old: AgentConfig = toml::from_str("central_url = \"ws://1.2.3.4:8787/ws/agent\"\n").unwrap();
        assert!(old.spots.is_empty());
    }
}

fn default_manual_hold() -> u64 {
    5
}

fn default_manual_release() -> u64 {
    180
}

fn default_true_cfg() -> bool {
    true
}

fn default_raven_min() -> u8 {
    25
}
