# Roteiro

> Etapa experimental atual: [radar, navegação e editor de rotas](navegacao-editor.md).
> Canal estável preservado na [main +106](base-main-v106.md). PC1 é o teste oficial;
> ELISMAR-NOTE participa como teste adicional autorizado para FirstHand e YouSee.

| Fase | Situação | Critério de pronto |
|---|---|---|
| F0 Leitura do legado e dados reais | Fontes e evidências locais disponíveis; golden integral ainda pendente | Golden com as 48 linhas de PK e 424 de joia |
| F1 Workspace, protocolo, regras puras | Implementados; validação atual na documentação da navegação | Testes de regras, compatibilidade e regressão aprovados |
| F2 fake-client + agente (título, TCP, OCR, RAM) | Compilado e em uso nos cinco PCs; leitura real e automações testadas | Preservar leitores existentes e diagnosticar leitura desconhecida sem agir às cegas |
| F3 Central + SQLite + painel + replay | Central/painel em uso; radar, editor e histórico de rotas implementados; replay integral pendente | Persistência, isolamento, primeira exibição e atualização sem perda de edição |
| F4 Atualização assinada, canais, rollback, TLS, login | Ed25519, pareamento, canais e rollback em uso; TLS/login de usuário ainda pendentes | Preservar canal estável, recusar assinatura inválida e downgrade |
| F5 Notificações, Saúde, PK, carga/caos/24 h | Parcial: ntfy/Telegram, aba Saúde e PK escritos | CPU média < 1% em 24 h |
| F6 Joias | Desligada (decisão 0004) | Método exato provado com dados reais |
| F7 Implantação dos agentes | Cinco PCs conectados; experimento restrito a PC1 e ELISMAR-NOTE | Homologar piloto antes de publicação geral |

Falta da estratégia de testes: replay de um dia, Playwright no CI, carga com 50 agentes, caos, 24 h,
mutação, fuzzing com cargo-fuzz, cobertura. O teste de "parsers nunca entram em pânico" (50 mil entradas
aleatórias) cobre parte do fuzzing até lá.

## PENDENTE (lembrar o dono) — Arena / Lorencia
Registro histórico de 09/11: não substitui configurações ou autorizações posteriores
do dono. A revisão da navegação não altera as regras de depósito desses personagens.
Pedido em 09/11 12:49: 0MAGIC e xWizard (DESKTOP-RAG9LEL) farmam na Arena; o baú deles fica em
Lorencia e haverá "outro esquema" no mapa de Lorencia. O dono chama quando for a hora — mas pediu
para ser lembrado. Até lá: sem depósito automático para esses dois (sem spot no Kanturu).
