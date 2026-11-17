# 0011 — Automação de teclado e mouse

Data: 08/11/2026. Substitui a regra "o agente nunca envia teclas" do documento mestre.
Motivo: pedido do dono do projeto, com ticket da staff do The Classic (Coordenação de
Segurança & Anti-Cheat) liberando ferramentas de input na máquina dele. O ticket não foi
verificado por nós; a responsabilidade pela conta é do dono.

## O que foi medido
- Mensagens de janela (PostMessage KEYDOWN/KEYUP, com CHAR, com ACTIVATE): o jogo IGNORA.
  `zen-diag tecla`. Logo, não dá para mandar input com a janela em segundo plano.
- SendInput (input padrão do Windows) com a janela em foco: FUNCIONA. `zen-diag tecla-input`
  abriu e fechou o inventário do ReDMooN e devolveu o foco.
- Conclusão: o driver Interception (kernel) NÃO é necessário. Fica de fora: precisa reiniciar,
  fica carregado para todos os jogos (anti-cheats de outros jogos bloqueiam) e tem os mesmos
  limites de foco.

## Regras
- Desligada por padrão (`automation_enabled = false`). Interruptor por PC no painel.
- Lista fechada de ações (`zen_core::actions::ActionKind`), sequência fixa no código.
  O painel/central só escolhe o NOME da ação. Nenhuma tecla ou texto livre vem da rede.
- Foco conferido antes de CADA passo; perdeu o foco, para. Foco e mouse voltam ao final.
- Automático só com o PC sem uso há `automation_idle_s` (60 s). Ações do painel não esperam.
- Tudo vai para o log com a palavra AUTOMAÇÃO.
- Continua proibido: escrever na memória do jogo, injetar DLL, mexer em pacotes, ler o
  Settings.ini.

## Ações
- `toggle_inventory` (V): manual pelo painel e automática para travar o Zen
  (abre depois de 90 s sem trava, fecha quando trava; se não travar em 15 min fecha e espera 10).
- Em aberto, dependem de informação do dono: voltar ao farm depois de morrer; reconectar
  depois de DC (senha guardada com DPAPI, nunca no central).
