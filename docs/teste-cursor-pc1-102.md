# Cursor, captura e isolamento de entrada — PC1, 14/11/2026

Validação autorizada pela ponte com o jogo real. Os demais computadores permaneceram em +99 enquanto as revisões +100 e +101 eram testadas exclusivamente no PC1.

- `cargo test -p zen-agent --offline`: 53 testes aprovados. Build release em `target/publicar`, sem substituir o executável em uso durante a compilação.
- +100 instalada no PC1 às 18:37. Com automação pausada, cursor em (2481,951) permaneceu intacto e a ação de inventário id 22 foi recusada. Ao retomar, às 18:38:14 o agente registrou `CURSOR: reposicionado...` para HWND 327880/PID 26216 e levou o cursor a (2261,719), centro da janela do x9901 (origem 1746,319 + 515,400).
- Capturas antes/depois confirmaram Helper ativo, personagem vivo no mapa Kanturu e principal (160,197) preservado. Não foi enviada tecla Home durante os testes.
- A amostragem rápida durante ação de inventário do ReDMooN revelou uma falha da primeira revisão: foco retornava ao x9901 enquanto o cursor ainda estava em (1234,721), centro do ReDMooN. A correção periódica ocultava o atraso em verificações mais lentas. +100 não foi promovida ao canal estável.
- `restore_context` passou a usar `focus(previous)`, que aguarda a ativação e centraliza na janela confirmada. +101 recompilada, instalada e confirmada no PC1 às 18:46:39.
- `scripts/testar-cursor-troca.ps1` executou a ação id 33: observou saída ao ReDMooN, retorno ao x9901 em 1180 ms e cursor centralizado em 1364 ms, diferença de 184 ms. A reversão id 34 fechou o inventário e confirmou sucesso. Evidência: `target/cursor-switch-final.json`.
- Capturas finais dos dois personagens confirmaram inventário fechado e Helper ativo. Ambos permaneceram vivos, farmando nos próprios spots; mapa e principais preservados. PC1 voltou à configuração original de automação ligada.

O probe Rust `crates/agent/examples/cursor_live.rs --probe` apenas lê cursor/foco/origens. Os comandos de entrada efetivos foram executados pelo agente elevado, via API existente. O controle visual externo do MainMU foi recusado pela permissão da ferramenta; não houve teste de tecla F8 física nem de botão segurado no jogo nesta rodada. Esses bloqueios mantêm a cobertura automatizada; a pausa pelo painel foi validada ao vivo.

Após o sucesso da +101, o mesmo binário foi publicado pela ponte como +102 estável às 18:47:41, para todos os computadores. SHA256: `66F2DF19B648CAB0010C3F46B852736BB70A966CD0E7467B103C9C1A1CA6799D`. Backup estável +99 preservado em `target/release/atualizacoes/backup-cursor-*`. Somente PC1 permanece no canal de teste.

Esta validação cobre cursor, troca de foco, pausa e continuidade do farm. Os ajustes anteriores de NPC/obstáculo, incluídos no mesmo pacote, mantêm seus testes automatizados; não foi repetido um ciclo completo de Fortuna nesta rodada.

Às 18:51:39, instalação +102, conexão ativa, relatório recente e `last_error=null` confirmados nos cinco computadores: PC1, ELISMAR, ELISMAR-NOTE, DESKTOP-E1GLBEF e DESKTOP-RAG9LEL. Estado final salvo em `target/cursor-state-final.json`.
