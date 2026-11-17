# 0002 — Painel em HTML + JS puro, sem Svelte/Vite

**Decisão:** o painel é um único `index.html` embutido no binário do central (`include_str!`).

**Por quê (contrariando o doc):** são 4 PCs e um painel. Svelte + Vite + npm acrescentam uma cadeia de
build inteira, dependências a atualizar e mais um lugar para quebrar, sem ganho visível aqui. O HTML atual
já recebe por WebSocket, escapa todo texto do jogo (`textContent`), cabe sem rolar em 3440×1440 e 1920×1080
e foi testado no Chrome headless. Gráfico é um canvas de 20 linhas em vez do uPlot.

**Quando rever:** se o painel passar de ~1.000 linhas de JS ou ganhar várias telas novas.
