# 0005 — PK pelo chat: conflito entre duas regras suas  ⚠ PRECISA DA SUA DECISÃO

O prompt mestre diz ao mesmo tempo:
1. "Varredura completa de memória nunca no ciclo normal", e
2. alerta MORTO e ranking de PK (item 4 do foco), que hoje só funciona varrendo a memória atrás de "[PK] : ".

Não dá para ter as duas até acharmos a lista do chat. Implementei assim:
- `chat_enabled = false` por padrão: sem varredura, sem PK, sem MORTO.
- Se ligar: varre a cada **60 s** (não a cada 5 s). Na primeira varredura tudo que já está na memória é
  marcado como visto, sem reportar. Texto igual visto nas últimas 2 h não é reportado de novo — então
  uma segunda morte idêntica (mesmo assassino, mesma vítima) em 2 h não gera alerta.

Opções: (a) deixar desligado até o `listachat`; (b) ligar em 1 PC e aceitar a varredura a cada 60 s;
(c) ligar em todos. Minha recomendação: (a) até rodar o `listachat`, depois decidir com o resultado.
