//! Tokens dos agentes: emissão automática (pareamento), validação e renovação.
//! Sem I/O e sem relógio: o `main` grava no central.toml e passa o "agora".
//!
//! Modo automático (padrão, `auto_pairing = true`):
//! - Agente sem token se apresenta em /ws/pair com nome da máquina + `install_id`.
//! - Só aceita da rede local (o `main` checa o IP antes de chamar `pair`).
//! - Nome novo -> token novo. Nome existente com o MESMO install_id -> token reemitido
//!   (o PC perdeu o arquivo). Nome existente com install_id diferente -> recusado
//!   (outro PC tentando assumir o nome).
//! - Token com mais de `max_age_s` é trocado na conexão; o anterior continua valendo até
//!   o agente se conectar com o novo (assim uma queda no meio da troca não tranca o PC).
//!
//! Modo manual (`auto_pairing = false`): sem pareamento e sem renovação; vale o token
//! escrito à mão no central.toml.

#[derive(Debug, Clone, PartialEq)]
pub struct TokenEntry {
    pub machine: String,
    pub token: String,
    pub prev_token: Option<String>,
    pub install_id: Option<String>,
    pub issued_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PairResult {
    Issued(String),
    Reissued(String),
    Refused(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Auth {
    pub machine: String,
    /// O agente deve receber um token novo (`CentralMsg::NewToken`).
    pub send_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenStore {
    pub entries: Vec<TokenEntry>,
    pub blocked: Vec<String>,
}

pub const TOKEN_LEN: usize = 32;

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl TokenStore {
    fn find_mut(&mut self, machine: &str) -> Option<&mut TokenEntry> {
        self.entries
            .iter_mut()
            .find(|e| e.machine.eq_ignore_ascii_case(machine))
    }

    /// Valida o token de uma conexão de agente. `auto` liga a renovação.
    pub fn authenticate(
        &mut self,
        token: &str,
        auto: bool,
        now: u64,
        max_age_s: u64,
        gen: &mut dyn FnMut() -> String,
    ) -> Option<Auth> {
        if token.len() < 16 {
            return None;
        }
        let blocked = self.blocked.clone();
        let e = self.entries.iter_mut().find(|e| {
            ct_eq(e.token.as_bytes(), token.as_bytes())
                || e.prev_token
                    .as_deref()
                    .is_some_and(|p| ct_eq(p.as_bytes(), token.as_bytes()))
        })?;
        if blocked.iter().any(|b| b.eq_ignore_ascii_case(&e.machine)) {
            return None;
        }
        let using_current = ct_eq(e.token.as_bytes(), token.as_bytes());
        let mut send_token = None;
        if using_current {
            // Chegou com o atual: a troca anterior foi concluída.
            e.prev_token = None;
            if auto && now.saturating_sub(e.issued_at) >= max_age_s {
                e.prev_token = Some(e.token.clone());
                e.token = gen();
                e.issued_at = now;
                send_token = Some(e.token.clone());
            }
        } else {
            // Chegou com o anterior: a troca não foi gravada no agente; manda de novo.
            send_token = Some(e.token.clone());
        }
        Some(Auth {
            machine: e.machine.clone(),
            send_token,
        })
    }

    pub fn pair(
        &mut self,
        machine: &str,
        install_id: &str,
        auto: bool,
        from_lan: bool,
        now: u64,
        gen: &mut dyn FnMut() -> String,
    ) -> PairResult {
        if !auto {
            return PairResult::Refused("pareamento automático desligado no central".into());
        }
        if !from_lan {
            return PairResult::Refused("pareamento só pela rede local".into());
        }
        if !zen_core::protocol::valid_machine_name(machine) {
            return PairResult::Refused("nome de máquina inválido (use letras, números, - e _)".into());
        }
        if install_id.len() < 16 || install_id.len() > 64 {
            return PairResult::Refused("install_id inválido".into());
        }
        if self.is_blocked(machine) {
            return PairResult::Refused("máquina bloqueada no central.toml".into());
        }
        if let Some(e) = self.find_mut(machine) {
            return match e.install_id.as_deref() {
                Some(id) if ct_eq(id.as_bytes(), install_id.as_bytes()) => {
                    e.prev_token = None;
                    e.token = gen();
                    e.issued_at = now;
                    PairResult::Reissued(e.token.clone())
                }
                Some(_) => PairResult::Refused("já existe outro PC com esse nome".into()),
                None => PairResult::Refused(
                    "máquina cadastrada à mão no central.toml; copie o token de lá ou apague a entrada".into(),
                ),
            };
        }
        let token = gen();
        self.entries.push(TokenEntry {
            machine: machine.to_string(),
            token: token.clone(),
            prev_token: None,
            install_id: Some(install_id.to_string()),
            issued_at: now,
        });
        PairResult::Issued(token)
    }

    pub fn revoke(&mut self, machine: &str) -> bool {
        let n = self.entries.len();
        self.entries.retain(|e| !e.machine.eq_ignore_ascii_case(machine));
        n != self.entries.len()
    }

    /// "Remover PC" do painel: apaga o token e bloqueia o nome, para o pareamento automático
    /// não trazer a máquina de volta. Desfazer: tirar o nome de `blocked_machines`.
    pub fn remove_and_block(&mut self, machine: &str) -> bool {
        let removed = self.revoke(machine);
        if !self.is_blocked(machine) {
            self.blocked.push(machine.to_string());
        }
        removed
    }

    pub fn is_blocked(&self, machine: &str) -> bool {
        self.blocked.iter().any(|b| b.eq_ignore_ascii_case(machine))
    }
}

/// Rede local: loopback, 10/8, 172.16/12, 192.168/16, 169.254/16, 100.64/10 (Tailscale/CGNAT),
/// IPv6 loopback, ULA (fc00::/7) e link-local (fe80::/10).
pub fn is_lan(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_lan(IpAddr::V4(v4));
            }
            let s = v6.segments()[0];
            v6.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rede_local() {
        for ok in [
            "127.0.0.1",
            "192.168.0.10",
            "10.1.2.3",
            "172.20.0.1",
            "100.101.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.5",
        ] {
            assert!(is_lan(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:db8::1", "::ffff:8.8.8.8"] {
            assert!(!is_lan(bad.parse().unwrap()), "{bad}");
        }
    }

    fn gen() -> impl FnMut() -> String {
        let mut i = 0;
        move || {
            i += 1;
            format!("token{i:027}")
        }
    }

    const ID: &str = "install-id-0123456789";
    const DAY: u64 = 86_400;

    #[test]
    fn pareia_e_autentica() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        let a = s.authenticate(&t, true, 10, 30 * DAY, &mut g).unwrap();
        assert_eq!(
            a,
            Auth {
                machine: "PC1".into(),
                send_token: None
            }
        );
        assert!(s
            .authenticate("token-que-nao-existe-xxxxx", true, 10, 30 * DAY, &mut g)
            .is_none());
        assert!(s.authenticate("curto", true, 10, 30 * DAY, &mut g).is_none());
    }

    #[test]
    fn modo_manual_e_fora_da_rede_recusam() {
        let mut s = TokenStore::default();
        let mut g = gen();
        assert!(matches!(
            s.pair("PC1", ID, false, true, 0, &mut g),
            PairResult::Refused(_)
        ));
        assert!(matches!(
            s.pair("PC1", ID, true, false, 0, &mut g),
            PairResult::Refused(_)
        ));
        assert!(s.entries.is_empty());
    }

    #[test]
    fn nome_ja_usado_por_outro_pc_e_recusado() {
        let mut s = TokenStore::default();
        let mut g = gen();
        s.pair("PC1", ID, true, true, 0, &mut g);
        let r = s.pair("pc1", "outro-install-id-999999", true, true, 0, &mut g);
        assert!(matches!(r, PairResult::Refused(m) if m.contains("outro PC")));
    }

    #[test]
    fn mesmo_pc_que_perdeu_o_token_recebe_outro() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t1) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        let PairResult::Reissued(t2) = s.pair("PC1", ID, true, true, 5, &mut g) else {
            panic!()
        };
        assert_ne!(t1, t2);
        assert!(s.authenticate(&t1, true, 6, 30 * DAY, &mut g).is_none()); // o antigo morreu
        assert!(s.authenticate(&t2, true, 6, 30 * DAY, &mut g).is_some());
    }

    #[test]
    fn entrada_manual_nao_e_sequestrada() {
        let mut s = TokenStore::default();
        s.entries.push(TokenEntry {
            machine: "PC1".into(),
            token: "manual-token-0123456789abcdef".into(),
            prev_token: None,
            install_id: None,
            issued_at: 0,
        });
        let r = s.pair("PC1", ID, true, true, 0, &mut gen());
        assert!(matches!(r, PairResult::Refused(_)));
    }

    #[test]
    fn renovacao_com_periodo_de_carencia() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t1) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        // 31 dias depois: conecta com t1, recebe t2.
        let a = s.authenticate(&t1, true, 31 * DAY, 30 * DAY, &mut g).unwrap();
        let t2 = a.send_token.clone().unwrap();
        assert_ne!(t1, t2);
        // Caiu antes de gravar: volta com t1 e recebe t2 de novo (sem gerar t3).
        let a = s.authenticate(&t1, true, 31 * DAY + 5, 30 * DAY, &mut g).unwrap();
        assert_eq!(a.send_token.as_deref(), Some(t2.as_str()));
        // Conectou com t2: t1 deixa de valer.
        assert_eq!(
            s.authenticate(&t2, true, 31 * DAY + 9, 30 * DAY, &mut g)
                .unwrap()
                .send_token,
            None
        );
        assert!(s.authenticate(&t1, true, 31 * DAY + 10, 30 * DAY, &mut g).is_none());
    }

    #[test]
    fn modo_manual_nao_renova() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t1) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        let a = s.authenticate(&t1, false, 365 * DAY, 30 * DAY, &mut g).unwrap();
        assert_eq!(a.send_token, None);
    }

    #[test]
    fn bloqueio_e_revogacao() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        s.blocked.push("pc1".into());
        assert!(s.authenticate(&t, true, 1, 30 * DAY, &mut g).is_none());
        assert!(matches!(
            s.pair("PC1", ID, true, true, 1, &mut g),
            PairResult::Refused(_)
        ));
        s.blocked.clear();
        assert!(s.revoke("PC1"));
        assert!(s.authenticate(&t, true, 1, 30 * DAY, &mut g).is_none());
    }

    #[test]
    fn remover_pc_nao_volta_pelo_pareamento() {
        let mut s = TokenStore::default();
        let mut g = gen();
        let PairResult::Issued(t) = s.pair("PC1", ID, true, true, 0, &mut g) else {
            panic!()
        };
        assert!(s.remove_and_block("pc1"));
        assert!(s.is_blocked("PC1"));
        assert!(s.authenticate(&t, true, 1, 30 * DAY, &mut g).is_none());
        assert!(matches!(
            s.pair("PC1", ID, true, true, 2, &mut g),
            PairResult::Refused(_)
        ));
        // Remover de novo não duplica o bloqueio.
        assert!(!s.remove_and_block("PC1"));
        assert_eq!(s.blocked.len(), 1);
    }

    #[test]
    fn nome_invalido() {
        let mut s = TokenStore::default();
        assert!(matches!(
            s.pair("<script>", ID, true, true, 0, &mut gen()),
            PairResult::Refused(_)
        ));
        assert!(matches!(
            s.pair("PC1", "curto", true, true, 0, &mut gen()),
            PairResult::Refused(_)
        ));
    }
}
