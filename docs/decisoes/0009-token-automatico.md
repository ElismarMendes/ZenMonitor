# 0009 — Automação de token (com liga/desliga)

**Ligada (padrão):** `auto_pairing = true` no central e `auto_token = true` no agente.
- Agente sem token conecta em `/ws/pair` com o nome da máquina e um `install_id` gerado uma vez.
- O central só aceita **da rede local** (loopback, 10/8, 172.16/12, 192.168/16, 100.64/10 do
  Tailscale, IPv6 local) e grava o token no `central.toml`.
- Mesmo nome + mesmo `install_id` = mesmo PC que perdeu o arquivo: recebe token novo.
  Mesmo nome + `install_id` diferente = outro PC tentando assumir o nome: recusado.
  Entrada escrita à mão (sem `install_id`) nunca é sobrescrita pelo pareamento.
- Renovação: token com mais de `token_max_age_days` (30) é trocado na conexão; o anterior vale
  até o agente voltar com o novo (queda no meio da troca não tranca o PC).
- Token recusado (401): o agente apaga e pareia de novo.

**Desligada:** sem pareamento, sem renovação; só valem tokens escritos à mão.

**Como trocar:** `.\zen.cmd token on|off` (central e agente), botão na aba MÁQUINAS do painel
(exige o token de admin), `auto_pairing`/`auto_token` nos `.toml`, ou `ZEN_AUTO_PAIRING` /
`ZEN_AUTO_TOKEN` no `.env` (ambiente manda sobre o arquivo).

**O custo, dito claramente:** com a automação ligada, qualquer aparelho da sua rede de casa
consegue se cadastrar como máquina nova e mandar dados falsos ao painel. Ele não consegue
mandar comandos (exigem o token de admin) nem tomar o lugar de um PC já cadastrado. Para 4 PCs
numa rede de casa é aceitável; se a rede tiver visitas ou aparelhos que você não controla,
deixe ligado só enquanto cadastra os PCs e depois desligue.

**Tirar um PC:** botão **Remover PC** na aba MÁQUINAS (pede o token de admin). Ele apaga o token,
bloqueia o nome (o pareamento automático não traz de volta), derruba a conexão e tira o PC do
painel. Para desbloquear: tire o nome de `blocked_machines` no `central.toml`.

**Atenção ao editar o `central.toml` à mão:** faça com o central PARADO (`.\zen.cmd stop`).
O central guarda os tokens em memória e regrava o arquivo a cada pareamento, renovação ou
mudança no painel; uma edição feita com ele rodando é sobrescrita e não tem efeito.
