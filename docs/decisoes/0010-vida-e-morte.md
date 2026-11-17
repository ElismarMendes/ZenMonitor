# 0010 — Vida na memória e detecção de morte

Data: 08/11/2026. Pedido: "detectar quando o char morrer, tirar print e mostrar no painel" e
"quero que leia a memória da vida".

## O que foi medido (não suposto)
- Número de HP na tela: fonte própria, com contorno, dígitos encostando. Modelos tirados de
  24 recortes rotulados e validados em 48 não vistos (72/72). `tests/fixtures/hp_field/`.
- `zen-diag vida-mem`: o HP mora num **u16**. Semente da tela acha ~18 mil u16; filtrando pelas
  mudanças sobra 1 endereço (acompanhou 42 mudanças seguidas no x9901).
- A tela e a memória ficam às vezes 1 quadro defasadas: descartar por falhas SOMADAS perdeu o
  endereço certo do ReDMooN. Por isso: leitura antes E depois da tela, descarte só com 3 falhas
  seguidas ou taxa de acerto < 50%, trava com 8 acertos e taxa >= 80%.
- Em produção (2 chars), HP e Zen estão a uma distância fixa: Zen = HP + 0x1722 nos dois.
  Ainda NÃO é usado (ver "em aberto").

## Como funciona
- Agente lê a vida travada a cada 500 ms (2 bytes por char). Vida 0 depois de > 0 = suspeita.
- Confirmação: print da janela e leitura do número NA MESMA captura. Tela com vida > 0 (2x,
  400 ms) = endereço errado: refaz a trava e não avisa. Tela 0 ou ilegível = morte.
- `AgentMsg::Death` com PNG -> central grava (tabela `deaths`, 30 dias), liga MORTO (10 min),
  painel mostra o print no card e na lista "Mortes" (clique amplia). Celular: alerta MORTO.
- Travas (Zen e vida) gravadas em `travas.json` e restauradas no reinício do agente só se for
  o mesmo processo (pid + hora de início + nome do char).
- Desligar: `hp_region = []` no agent.toml.

## Em aberto
- Usar o deslocamento HP -> Zen para travar o Zen sem abrir o inventário (precisa de mais
  chars/relogs para provar que é fixo; conferir sempre com a leitura da tela).
- As 2 mortes do char "Teste" (teste automático de 08/11) ficam na lista até sair dos 30 dias.
