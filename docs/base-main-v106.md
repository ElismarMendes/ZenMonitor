# Main de referência — publicação +106

O usuário definiu em 14/11/2026 a +106 como base estável antes dos experimentos de navegação.

- Referência Git: branch `main` e tag anotada `main-v106`.
- Agente publicado/instalado nos cinco computadores: `3.0.0-alpha.1+106`.
- SHA256 do agente: `07B582921EF50FC262F0A0A356C9894F352A28AE8AB2C8AB6E755CA508AB7A64`.
- Validação do agente: 62 testes aprovados; interferência de mouse/pausa testadas no x9901. Não foi refeito um ciclo completo de Fortuna nessa rodada.
- Canal oficial de teste: somente PC1.
- Backup de arquivos em `versoes/main-v106-2026-11-14/`, com fonte, bundle Git, binários, publicação assinada, configuração local e snapshot consistente da base do central.

A pasta `runtime` do backup contém configurações e dados locais e deve permanecer privada. Esses arquivos, chaves e tokens não entram no ZIP de fonte nem no bundle Git. Configurações de outros computadores não são sobrescritas pelo backup/restauração do agente.

Para recuperar o código, consulte `main-v106` ou o bundle do backup. Novos experimentos devem partir de `codex/navegacao`, sem mover a tag da base. Para recuperar o agente nos computadores, republique o binário do backup: o publicador gera uma sequência nova e assinatura válida, pois agentes não aceitam simplesmente baixar o número da versão. `restaurar-agente.ps1` no backup verifica o hash e publica inicialmente no canal de teste; `-Canal estavel` publica para todos quando solicitado.

Os experimentos devem preservar o motor atual de cliques/botão segurado, validação de mapa, tolerância máxima de dois tiles para spots, leitura de memória/tela e detecção de jogadores/monstros. A evolução da navegação deve ser validada no PC1 antes de qualquer implantação geral.
