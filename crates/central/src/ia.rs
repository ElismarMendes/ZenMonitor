//! Analista de falhas por IA (v5, 17/11/2026).
//!
//! Quando um agente avisa uma FALHA que ele fez sozinho (Fortuna, volta ao spot, Raven, Pick...),
//! a central espera o log daquele PC chegar, tira dele nomes de chars, PCs, IPs e usuário do
//! Windows, e pergunta a causa provável a uma IA. A resposta vai para o celular e para o painel.
//!
//! Regras:
//! - Só LÊ. Nunca manda comando a agente; o bot não depende disto para funcionar.
//! - Desligada por padrão (`[ia] ligada = false` no central.toml).
//! - Cadeia de provedores: tenta o 1º; erro, limite (429) ou demora -> o próximo. Todos falharam
//!   = nada além do aviso normal, que já foi enviado antes.
//! - As chaves ficam no central.toml (fora do git e do pacote dos agentes) e nunca vão para log
//!   nem para o painel.
//! - O central.toml em disco manda nesta seção: dá para colar as chaves com a central rodando.

use crate::config::CentralConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IaConfig {
    /// Liga a análise. Com tudo vazio ou desligado, a central se comporta como antes.
    pub ligada: bool,
    /// Teto de análises por dia (todas as IAs somadas).
    pub max_por_dia: u32,
    /// Espera antes de ler o log (o agente manda o log para a central a cada 2 min).
    pub espera_log_s: u64,
    /// Em ordem de preferência. Provedor sem chave é pulado.
    pub provedores: Vec<Provedor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Provedor {
    pub nome: String,
    /// Endpoint compatível com a API de chat da OpenAI.
    pub url: String,
    pub modelo: String,
    pub chave: String,
    pub max_por_dia: u32,
    pub timeout_s: u64,
}

impl Default for Provedor {
    fn default() -> Self {
        Self { nome: String::new(), url: String::new(), modelo: String::new(), chave: String::new(), max_por_dia: 50, timeout_s: 25 }
    }
}

impl Default for IaConfig {
    fn default() -> Self {
        let p = |nome: &str, url: &str, modelo: &str, max: u32| Provedor {
            nome: nome.into(),
            url: url.into(),
            modelo: modelo.into(),
            max_por_dia: max,
            ..Provedor::default()
        };
        Self {
            ligada: false,
            max_por_dia: 40,
            espera_log_s: 150,
            provedores: vec![
                p("groq", "https://api.groq.com/openai/v1/chat/completions", "openai/gpt-oss-120b", 200),
                p("gemini", "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions", "gemini-2.5-flash", 20),
                p("openrouter", "https://openrouter.ai/api/v1/chat/completions", "openrouter/free", 40),
            ],
        }
    }
}

/// Só a seção `[ia]` do central.toml (o resto do arquivo é ignorado aqui).
#[derive(Deserialize, Default)]
struct SoIa {
    ia: Option<IaConfig>,
}

/// Lê a seção `[ia]` do arquivo em disco (vale a última edição, sem reiniciar a central).
pub fn config_em_disco() -> Option<IaConfig> {
    let txt = std::fs::read_to_string(CentralConfig::dir().join("central.toml")).ok()?;
    toml::from_str::<SoIa>(&txt).ok()?.ia
}

// ---------------------------------------------------------------- contagem e repetição

#[derive(Default)]
struct Uso {
    dia: u64,
    total: u32,
    por_provedor: HashMap<String, u32>,
    /// Falha já analisada -> quando (não repete a mesma em 30 min).
    recentes: HashMap<String, u64>,
}
static USO: Mutex<Option<Uso>> = Mutex::new(None);

fn reservar(chave_falha: &str, agora: u64, cfg: &IaConfig) -> Result<(), &'static str> {
    let mut g = USO.lock().map_err(|_| "contador indisponível")?;
    let u = g.get_or_insert_with(Uso::default);
    if u.dia != agora / 86_400 {
        *u = Uso { dia: agora / 86_400, ..Uso::default() };
    }
    u.recentes.retain(|_, t| agora.saturating_sub(*t) < 1800);
    if u.recentes.contains_key(chave_falha) {
        return Err("mesma falha analisada há menos de 30 min");
    }
    if u.total >= cfg.max_por_dia {
        return Err("teto diário de análises atingido");
    }
    u.recentes.insert(chave_falha.to_string(), agora);
    u.total += 1;
    Ok(())
}

fn provedor_livre(p: &Provedor) -> bool {
    let Ok(mut g) = USO.lock() else { return false };
    let u = g.get_or_insert_with(Uso::default);
    let n = u.por_provedor.entry(p.nome.clone()).or_insert(0);
    if *n >= p.max_por_dia {
        return false;
    }
    *n += 1;
    true
}

// ---------------------------------------------------------------- anonimização

/// Troca nomes de chars e PCs por apelidos, IPs por um marcador e o usuário do Windows nos
/// caminhos. Devolve o texto e a tabela para desfazer os apelidos na resposta.
pub fn anonimizar(texto: &str, pcs: &[String], chars: &[String]) -> (String, Vec<(String, String)>) {
    let mut tabela: Vec<(String, String)> = Vec::new();
    for (i, n) in pcs.iter().enumerate() {
        tabela.push((n.clone(), format!("PC_{}", i + 1)));
    }
    for (i, n) in chars.iter().enumerate() {
        tabela.push((n.clone(), format!("CHAR_{}", i + 1)));
    }
    // Nomes maiores primeiro: "ELISMAR-NOTE" antes de "ELISMAR".
    tabela.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut s = texto.to_string();
    for (real, apelido) in &tabela {
        if !real.is_empty() {
            s = s.replace(real.as_str(), apelido);
        }
    }
    (sem_usuario_windows(&sem_ips(&s)), tabela)
}

pub fn desfazer(texto: &str, tabela: &[(String, String)]) -> String {
    let mut ordem: Vec<&(String, String)> = tabela.iter().collect();
    // "CHAR_10" antes de "CHAR_1".
    ordem.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    let mut s = texto.to_string();
    for (real, apelido) in ordem {
        s = s.replace(apelido.as_str(), real);
    }
    s
}

fn sem_ips(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.')) {
            // tenta ler d+.d+.d+.d+
            let mut j = i;
            let mut grupos = 0;
            loop {
                let ini = j;
                while j < b.len() && b[j].is_ascii_digit() && j - ini < 3 {
                    j += 1;
                }
                if j == ini {
                    break;
                }
                grupos += 1;
                if grupos == 4 || j >= b.len() || b[j] != b'.' {
                    break;
                }
                j += 1;
            }
            let fim_ok = j >= b.len() || !b[j].is_ascii_digit();
            if grupos == 4 && fim_ok {
                out.push_str("<ip>");
                i = j;
                continue;
            }
        }
        let c = s[i..].chars().next().unwrap_or(' ');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn sem_usuario_windows(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(p) = resto.to_ascii_lowercase().find("\\users\\") {
        let corte = p + "\\users\\".len();
        out.push_str(&resto[..corte]);
        let depois = &resto[corte..];
        let fim = depois.find(['\\', ' ', '"']).unwrap_or(depois.len());
        out.push_str("<usuario>");
        resto = &depois[fim..];
    }
    out.push_str(resto);
    out
}

// ---------------------------------------------------------------- chamada

const INSTRUCOES: &str = "Você analisa falhas de um bot que automatiza personagens do jogo MU Online \
(servidor The Classic) em vários PCs. Rotinas do bot: volta ao spot de farm (teleporte pelo M e caminhada), \
Fortuna do Imperador no NPC Lugard (sacar Zen do baú de Lorencia, Gremory, guardar no J), restaurar o Dark \
Raven no Trainer de Lorencia, ativar o Pick do Zen pela janela Y, cofre do Kanturu. O bot lê a tela e a \
memória do jogo e usa teclado/mouse. Responda em português do Brasil, no máximo 4 linhas curtas: \
1) causa mais provável, 2) o que o dono deve fazer (ou 'nada, o bot tenta de novo'). Use só o que está no \
aviso e no log; se não der para saber, diga que não dá. Não invente números nem nomes.";

/// Pergunta à cadeia de provedores. Devolve (provedor, resposta) do primeiro que responder.
pub async fn perguntar(http: &reqwest::Client, cfg: &IaConfig, pergunta: &str) -> Result<(String, String), String> {
    let mut erros = Vec::new();
    for p in cfg.provedores.iter().filter(|p| !p.chave.trim().is_empty() && !p.url.is_empty()) {
        if !provedor_livre(p) {
            erros.push(format!("{}: teto diário", p.nome));
            continue;
        }
        let corpo = serde_json::json!({
            "model": p.modelo,
            "temperature": 0.2,
            "max_tokens": 400,
            "messages": [
                {"role": "system", "content": INSTRUCOES},
                {"role": "user", "content": pergunta},
            ],
        });
        let r = http
            .post(&p.url)
            .bearer_auth(p.chave.trim())
            .timeout(Duration::from_secs(p.timeout_s.max(5)))
            .header("content-type", "application/json")
            .body(corpo.to_string())
            .send()
            .await;
        match r {
            Ok(resp) if resp.status().is_success() => {
                let v: serde_json::Value = match resp.bytes().await.map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string())) {
                    Ok(v) => v,
                    Err(e) => {
                        erros.push(format!("{}: resposta ilegível ({e})", p.nome));
                        continue;
                    }
                };
                let txt = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
                if txt.is_empty() {
                    erros.push(format!("{}: resposta vazia", p.nome));
                    continue;
                }
                return Ok((p.nome.clone(), txt));
            }
            Ok(resp) => erros.push(format!("{}: HTTP {}", p.nome, resp.status().as_u16())),
            Err(e) if e.is_timeout() => erros.push(format!("{}: sem resposta em {} s", p.nome, p.timeout_s)),
            Err(e) => erros.push(format!("{}: {}", p.nome, if e.is_connect() { "sem conexão" } else { "erro de rede" })),
        }
    }
    if erros.is_empty() {
        Err("nenhum provedor com chave no central.toml".into())
    } else {
        Err(erros.join("; "))
    }
}

/// Últimas linhas relevantes (avisos/erros) do log que o agente mandou para a central.
pub fn trecho_do_log(machine: &str, linhas: usize) -> String {
    let p = CentralConfig::dir().join("logs").join("agentes").join(format!("{machine}.log"));
    let Ok(bytes) = std::fs::read(&p) else { return String::new() };
    let ini = bytes.len().saturating_sub(96 * 1024);
    let txt = String::from_utf8_lossy(&bytes[ini..]);
    let rel: Vec<&str> = txt.lines().filter(|l| l.contains(" WARN ") || l.contains(" ERROR ")).collect();
    let de = rel.len().saturating_sub(linhas);
    rel[de..]
        .iter()
        .map(|l| l.chars().take(300).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Monta a pergunta (já anônima). Separado para teste.
pub fn montar_pergunta(aviso: &str, log: &str, pcs: &[String], chars: &[String]) -> (String, Vec<(String, String)>) {
    let bruto = format!("AVISO DO BOT:\n{aviso}\n\nÚLTIMOS AVISOS E ERROS DO LOG DESSE PC:\n{log}");
    anonimizar(&bruto, pcs, chars)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn anonimiza_nomes_ips_e_usuario_e_desfaz_na_resposta() {
        let pcs = vec!["ELISMAR".to_string(), "ELISMAR-NOTE".to_string()];
        let chars = vec!["ClassicSM".to_string(), "x9901".to_string()];
        let txt = r"ELISMAR-NOTE: ClassicSM falhou em 192.168.31.176; print C:\Users\elism\Downloads\x.png; ELISMAR ok; versão 3.0.0+140";
        let (a, t) = anonimizar(txt, &pcs, &chars);
        assert!(!a.contains("ELISMAR") && !a.contains("ClassicSM") && !a.contains("192.168") && !a.contains("elism"), "{a}");
        assert!(a.contains("PC_2: CHAR_1") && a.contains("PC_1 ok") && a.contains("<ip>") && a.contains(r"\Users\<usuario>\"), "{a}");
        assert!(a.contains("3.0.0+140"), "versão não é IP: {a}");
        assert_eq!(desfazer("CHAR_1 no PC_2", &t), "ClassicSM no ELISMAR-NOTE");
    }

    #[test]
    fn apelido_dez_nao_vira_apelido_um() {
        let chars: Vec<String> = (1..=10).map(|i| format!("c{i}x")).collect();
        let (_, t) = anonimizar("", &[], &chars);
        assert_eq!(desfazer("CHAR_10 e CHAR_1", &t), "c10x e c1x");
    }

    #[test]
    fn mesma_falha_nao_repete_e_teto_diario_vale() {
        let cfg = IaConfig { max_por_dia: 2, ..IaConfig::default() };
        let dia = 9_000 * 86_400;
        assert!(reservar("A", dia, &cfg).is_ok());
        assert!(reservar("A", dia + 60, &cfg).is_err(), "mesma falha em 30 min");
        assert!(reservar("B", dia + 60, &cfg).is_ok());
        assert!(reservar("C", dia + 60, &cfg).is_err(), "teto de 2 por dia");
        assert!(reservar("C", dia + 86_400, &cfg).is_ok(), "dia novo zera");
    }

    #[test]
    fn padrao_vem_desligado_e_sem_chaves() {
        let c = IaConfig::default();
        assert!(!c.ligada);
        assert!(c.provedores.iter().all(|p| p.chave.is_empty()));
        assert_eq!(c.provedores[0].nome, "groq");
    }

    #[test]
    fn secao_ia_do_toml_e_lida_sem_o_resto() {
        let txt = "bind = \"0.0.0.0:8787\"\n[ia]\nligada = true\n[[ia.provedores]]\nnome = \"groq\"\nchave = \"k\"\n";
        let d: SoIa = toml::from_str(txt).unwrap();
        let ia = d.ia.unwrap();
        assert!(ia.ligada);
        assert_eq!(ia.provedores[0].chave, "k");
        assert_eq!(ia.provedores[0].timeout_s, 25, "campos ausentes usam o padrão");
    }
}

/// Reserva uma análise (teto diário e repetição). Exposto para a central.
pub fn reservar_publico(chave_falha: &str, agora: u64, cfg: &IaConfig) -> Result<(), &'static str> {
    reservar(chave_falha, agora, cfg)
}
