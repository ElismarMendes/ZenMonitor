# 0015 — Atualização automática dos agentes (e logs de todos no PC principal)

Pedido (08/11 18:44): 5 PCs em casa; atualizar sem copiar exe à mão. 18:47: o PC principal
precisa ter os logs de todos os agentes.

## Atualização
- `zen.cmd chave` (uma vez): chave Ed25519. Privada em `%USERPROFILE%\ZenMonitor-chave\assinatura.key`
  (fora do projeto; nunca vai no pacote nem no chat). Pública em `chave-atualizacao.txt`, ao lado
  do central e dentro do pacote dos agentes.
- `zen.cmd publicar`: compila o agente em `target\publicar` e assina CONTEXTO + número da
  publicação + exe. O central serve `/api/update/agent` (número, tamanho, assinatura) e `/bin`.
- Agente: 60 s depois de subir e a cada 10 min pergunta; baixa só número MAIOR que o instalado e
  não recusado; confere tamanho, "MZ" e a assinatura com a chave do pacote. Sem a chave: não atualiza.
- Troca só com a automação parada (nunca no meio de volta ao spot/depósito): renomeia o exe em uso
  para `zen-agent.anterior.exe`, põe o novo, sai; o vigia relança em 5 s.
- Em teste até conectar ao central. Caiu 3 vezes ou 10 min sem conectar: volta a anterior e essa
  publicação nunca mais é tentada (`atualizacao.json`). O vigia antigo continua até o próximo logon.
- Painel (Máquinas): versão "3.0.0-alpha.1+N" com ✓ ou "atualizando para M…".

Risco aceito: HTTP sem TLS na rede local. Quem intercepta só consegue impedir a atualização;
trocar o exe exige a chave privada.

## Logs
A cada 2 min o central pede os logs de todos os agentes conectados e grava
`logs\agentes\<máquina>.log` no PC principal (últimos 64 KB + vigia.txt + estado da atualização).
Não aparece em Resultados.
