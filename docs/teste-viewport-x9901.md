# Cursor com painéis laterais — x9901, 14/11/2026

O ponto fixo `(515,400)` da +102 é correto com a área de mundo inteira. A captura real com inventário mostrou o personagem deslocado para a esquerda enquanto o cursor permanecia no centro anterior. O mesmo ocorre com atributos e inventário expandido.

A ponte elevada executou `layout_snapshot` no PC1 com os seguintes estados, sem Home, caminhada ou teleporte: tela livre, inventário, inventário expandido, atributos, atributos junto ao inventário e fechamento de todos os painéis. O Helper permaneceu ativo durante toda a sequência. O principal `(160,197)` e o mapa Kanturu foram preservados; a movimentação do próprio Helper permaneceu no entorno do spot.

O estacionamento agora usa o centro da área de mundo restante. Na janela normalizada de 1024 pixels de cliente, cada painel dockado ocupa 304 pixels:

| Estado observado no x9901 | Limite direito do mundo | Cursor relativo à janela |
|---|---:|---:|
| Sem painel | 1027 | (515,400) |
| Inventário ou atributos | 723 | (363,400) |
| Expandido ou atributos + inventário | 419 | (211,400) |

A detecção reconhece as molduras dos painéis, independentemente do texto, Zen ou personagem, e conta apenas painéis contíguos às bordas. Capturas já existentes de Zyro e Chaos Goblin também validam um e três painéis, respectivamente. Não altera o `CENTER`, a projeção da caminhada, os cliques do trajeto, a leitura de memória ou os detectores de jogadores/monstros.

O novo módulo `win/viewport.rs` recebe o quadro completo já obtido em `capture`, antes de seu recorte para OCR. A posição fica em cache por HWND/PID e época da interface. Teclas ou cliques de interface invalidam o estado antes/depois do envio; quadros iniciados antes da invalidação não podem repor estado antigo. Cliques físicos e teclado físico também invalidam; movimento simples de mouse não invalida. O gancho só incrementa um contador atômico.

Uma conferência adicional é feita após uma ação que muda painéis ou quando o cache está frio/vencido. Cliques de caminhada dentro do mundo e `Hold::steer` não solicitam captura adicional para este cálculo. O novo ponto respeita as mesmas proteções de foco, identidade, botão pressionado e pausa.

Validação automatizada: 59 testes aprovados, incluindo os quadros reais dos cinco estados do x9901, Zyro/Chaos e rejeição de captura anterior à tecla/PID diferente. O exemplo elevado corrigido confirmou os pontos esperados nos seis estados da sequência e terminou com os painéis fechados e Helper ligado (`ponte/saida/codex-layout-fix-live.*`).

A produção +103 foi instalada e conferida no PC1. Com um escopo temporário de foco no x9901, `scripts/testar-cursor-inventario.ps1` acionou a API existente do agente: id 51 abriu o inventário e confirmou `(363,400)`; id 52 fechou e confirmou `(515,400)`. O escopo devolveu a aplicação anterior ao terminar. A captura final confirmou inventário fechado, Helper ativo e principal preservado. Prova: `target/cursor-inventory-installed.json`, capturas `target/layout-x9901-fixed/*` e `target/layout-x9901-release-103.png`.

O mesmo binário foi promovido pela ponte a +104 estável às 19:15, com backup da +102 em `target/release/atualizacoes/backup-viewport-*`. SHA256: `D8E578968A23AA743323AFB91D4B9FE6EF9448C1DCCC653AFE4DC4C7C18B3BAB`. Só PC1 continua no canal de teste.

Às 19:19, +104 instalada, conexão ativa, relatórios recentes e `last_error=null` confirmados nos cinco computadores: PC1, ELISMAR, ELISMAR-NOTE, DESKTOP-E1GLBEF e DESKTOP-RAG9LEL. O x9901 estava em Kanturu (160,197), vivo, com principal (160,197) preservado. Estado final em `target/viewport-state-final.json`.
