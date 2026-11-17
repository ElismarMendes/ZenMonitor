# ZenMonitor — Base de conhecimento técnica

> Evolução de 15/11/2026: [radar, navegação e editor de rotas](navegacao-editor.md),
> na branch `codex/navegacao`. A base de produção preservada é a [main +106](base-main-v106.md).
> A descrição histórica abaixo permanece datada; os contratos novos estão no documento da navegação.

Documento de referência para qualquer sessão de desenvolvimento (humana ou com IA).
Estado descrito: 12/11/2026, branch `v4`, publicação estável 76, canal de teste 78.
Leia junto com `docs/ROTEIRO.md` e `docs/decisoes/` (16 registros de decisão).

---

## 1. Visão geral

### 1.1 Propósito
Monitorar e manter farmando, sem intervenção, vários personagens de MU Online (servidor The Classic) em 5 PCs:
- telemetria de Zen, vida, posição, mapa, mobs em volta e vida do Dark Raven;
- alertas (parou, DC, morto, perto de 2 bi) no painel e no celular (ntfy);
- automação por teclado e mouse: volta ao spot, Fortuna do Imperador, restauração do Raven, limpeza de mobs.

### 1.2 Regras que não mudam
| Regra | Motivo |
|---|---|
| Memória do jogo é **só leitura** (`ReadProcessMemory`). Nunca escrever, injetar DLL nem tocar em pacotes | Segurança da conta e decisão do dono |
| Nunca ler `Settings.ini` do jogo | Tem senha em texto puro |
| Token de admin e chave de assinatura não passam por chat nem saem do PC1 | Segurança |
| Toda ação de automação é conferida na tela antes e depois | Clique às cegas já travou chars |
| Comandos remotos são uma lista fechada (`protocol::Command`) | Sem execução arbitrária |
| Publicação: primeiro canal de teste (PC1), produção só com OK explícito do dono | Regra do dono, 11/11 |
| Teste nunca termina com char fora do spot | O x9901 perdeu a vaga por isso |

### 1.3 Componentes (workspace Rust, ~19 mil linhas, 156 testes)
| Crate | Papel | Tamanho |
|---|---|---|
| `zen-core` | Regras puras, sem I/O e sem dependências (serde opcional): glifos, vida, trava de RAM, terreno e rotas, recuperação, combate, alertas, protocolo, barra do Raven | ~5 mil linhas |
| `agent` (1 por PC, Windows) | Lê memória e tela, executa a automação, fala com o central por WebSocket, se atualiza sozinho | ~7 mil linhas |
| `central` (PC1) | Recebe os agentes, guarda histórico (SQLite), calcula alertas, serve o painel web, publica atualizações assinadas | ~3 mil linhas + painel |
| `diag` | Ferramenta de teste manual pela ponte (passos, objetos, mem-dump) | ~2 mil linhas |
| `fake-client` | Janela falsa do jogo para testes sem o jogo real | ~300 linhas |

Arquivos centrais do agente:
- `agent/src/main.rs` — laço principal (ciclo de 5 s; vida a cada 0,5 s), decisões de automação, comandos.
- `agent/src/monitor.rs` — travas de Zen/vida na RAM, DC, detecção de morte.
- `agent/src/win/nav.rs` — toda a navegação: teleporte pelo M, caminhada, baú, Fortuna, Trainer, skill.
- `agent/src/win/input.rs` — `SendInput` com foco conferido, sessão, trava manual (ganchos de mouse/teclado).
- `agent/src/win/objects.rs` — lista de personagens da memória (mobs, NPCs, jogadores).

### 1.4 Fluxo de dados
```
jogo (memória + tela)
   │  ReadProcessMemory (Zen u32, vida u16, lista de personagens)
   │  PrintWindow (canto com mapa/coords, vida, helper, barras, janelas)
   ▼
agent ── a cada 5 s: AgentMsg::Report (chars, eventos, saúde, catálogos) ──► central
agent ◄── CentralMsg::Command (lista fechada) ── central ◄── POST /api/command (token) ── painel
                                                   │
                                                   ├─ SQLite: histórico de Zen, mortes, PKs, auditoria
                                                   ├─ alertas → ntfy (celular)
                                                   └─ /ws/panel: estado inteiro em JSON a cada mudança
atualização: zen.cmd publicar → exe assinado (Ed25519) → agentes baixam, conferem, trocam e
             confirmam; se o novo não subir, voltam ao anterior
```

### 1.5 Como o agente sabe as coisas
| Dado | Fonte | Observação |
|---|---|---|
| Zen | RAM, com semente lida na tela do inventário | Trava por processo; refaz se divergir da tela |
| Vida | RAM (u16), semente pelo número da esfera | Confere com a tela a cada 30 s (5 s se a RAM diz 0) |
| Mapa e posição | Tela (canto superior esquerdo), modelos de pixel | 13 mapas reconhecidos |
| Helper ligado | Tela (ícone play/pause) | |
| Mobs, NPCs, jogadores | RAM: lista de entradas de 0x598 bytes | Só leitura; offsets validados no The Classic |
| Terreno andável | Arquivo do cliente `Data\WorldN\EncTerrainN.att` | Rota por A* em `zen-core::terrain` |
| Vida do Dark Raven | Tela (barra no topo, achada pelo rótulo "Raven") | |
| Janelas de NPC, baú, caixas de aviso | Tela (moldura ou texto) | |

### 1.6 Operação
- **Ponte:** jobs `.ps1` em `C:\ZenMonitorNovo\ponte\entrada` (UTF-8 com BOM, 1ª linha `# limite: N` minutos). Saída em `ponte\saida`. A ponte fecha sozinha depois de 8 h.
- **Build:** `zen.cmd build` (testes + release), `zen.cmd start`, `zen.cmd publicar-teste`, `zen.cmd publicar`.
- **Canais:** `test_machines` no `central.toml` define quem recebe o canal de teste.
- **Base congelada:** `versoes\base-v28-2026-11-10` com `restaurar-base.ps1`.

---

## 2. Diagnóstico: erros, lacunas e dívida técnica

Ordenado por impacto. "Visto em produção" significa que já causou problema real.

### 2.1 Alto impacto
| # | Problema | Evidência | Direção |
|---|---|---|---|
| 1 | **Tempo de parede em decisões.** Recuperação, bloqueios e hub usam a hora do relógio do PC | Visto em produção em 12/11: relógio acertado 5 h para trás deixou o AMUMU 46 min parado e o painel com "PAROU" falso. Corrigido pontualmente | Usar relógio monotônico (`Instant`) em toda lógica de espera; hora de parede só para exibir |
| 2 | **Uma automação por PC de cada vez, sem vigia dos outros chars.** Enquanto um char faz Fortuna (2–4 min), o outro fica sem recuperação | Morte do ReDMooN em 11/11 08:43, provável | Fila com prioridade: problema de sobrevivência (helper desligado) interrompe ou passa na frente de tarefa de conveniência (Fortuna) |
| 3 | **Dependência de pixels exatos e de taxa de quadros.** Coordenadas fixas para janela 1030×797; teclas e cliques curtos se perdem com o jogo lento | Visto em produção: notebook a 7 FPS perdeu V/Home a noite inteira; barra do Raven muda de lugar; "9" com outro desenho | Toda leitura de tela por busca com tolerância (já feito em parte); toda ação conferida e repetida; medir o FPS e adaptar os tempos |
| 4 | **Arquivos gigantes.** `nav.rs` (~2.700 linhas), `agent/main.rs` (~1.900), `diag/main.rs` (~2.200) | Cada mudança mexe em funções longas com muitos casos especiais | Dividir `nav.rs` em módulos (`teleport`, `walk`, `vault`, `fortune`, `trainer`, `skill`, `dialogs`); transformar o laço do `main.rs` em tarefas com estado próprio |
| 5 | **Mapa do spot espalhado em três campos** (`spots`, `arena`, `spot_maps`) | Visto em produção em 12/11: spot do Kanturu ficou marcado "Crywolf" e o char foi levado ao mapa errado | Um tipo só: `Spot { world, x, y }` por char, com migração do config antigo |

### 2.2 Médio impacto
| # | Problema | Direção |
|---|---|---|
| 6 | Estado crítico só em memória (bloqueios, contagem de tentativas, `hold`): reiniciar o agente (toda atualização) zera | Persistir o mínimo em arquivo ao lado do `travas.json` |
| 7 | Painel recebe o estado inteiro a cada mudança e redesenha tudo | Enviar só diferenças; componentes que não se recriam |
| 8 | Cada leitura de tela faz um `PrintWindow` próprio; num ciclo são várias por char | Um quadro por char por ciclo, recortado em memória |
| 9 | Config do agente (`agent.toml`) regravada inteira a cada mudança, sem versão de esquema | Campo `schema_version` e migrações explícitas |
| 10 | Mensagens de status são texto livre, e o painel e os testes fazem `match` por texto ("voltou ao spot", "depósito") | Enum de estado no protocolo; texto só para exibição |
| 11 | NPC "aprendido" errado quando uma caixa de aviso abre no caminho | Só aprender quando a lista da memória confirma um NPC naquele tile |
| 12 | Detecção de DC pela tabela TCP do Windows dá falso no notebook | Já mitigado (exige Zen e vida parados); validar em produção |
| 13 | Saídas da ponte em PowerShell 5.1 leem UTF-8 como Latin-1 | Forçar UTF-8 nas chamadas REST dos jobs |

### 2.3 Riscos de segurança e operação
| # | Risco | Situação |
|---|---|---|
| 14 | Painel e API sem TLS, autenticados só por um token fixo | Aceitável na rede local; não expor na internet sem proxy com TLS |
| 15 | Chave de assinatura das atualizações num único PC, sem cópia | Fazer cópia offline; documentar a troca de chave |
| 16 | Atualização vai para todos os PCs do canal de uma vez | Já existe canal de teste; falta liberação gradual (um PC por vez) |
| 17 | Sem cópia do banco fora do PC1 | Há backup diário local; copiar para outro PC |
| 18 | Uso de automação e leitura de memória pode violar as regras do servidor | Decisão e risco do dono; o projeto não tenta esconder nada |

### 2.4 Lacunas de teste
- Navegação (`nav.rs`) e entrada (`input.rs`) não têm teste automático: só teste real no jogo.
- Não há teste de regressão com prints reais para janelas (Trainer, baú, caixas de aviso) além de vida, Zen, coordenadas e helper.
- Mob parado no meio da rota: lógica testada só em `combat.rs`, nunca vista no jogo.
- Custo em Zen do Trainer e Raven em 0% (barra some) não tratados.

---

## 3. Stack e padrões recomendados

### 3.1 Manter
| Item | Por quê |
|---|---|
| Rust, workspace com `zen-core` sem dependências | As regras puras são testáveis fora do Windows; foi o que permitiu 156 testes |
| `tokio` + `axum` + `tokio-tungstenite` | Já em uso, estável |
| SQLite via `sqlx` | Volume pequeno, um arquivo, backup simples |
| Atualização assinada com `ed25519-dalek` e rollback | Funciona em produção |
| Painel em HTML/JS puro, sem build (decisão 0002) | Um arquivo embutido no binário; nada para instalar |
| `tracing` com arquivos rotativos | Os logs resolveram todos os diagnósticos até aqui |

### 3.2 Adicionar
| Necessidade | Sugestão | Observação |
|---|---|---|
| Tempo monotônico testável | Trait `Clock` em `zen-core` (implementação real com `Instant`, falsa nos testes) | Resolve o item 1 sem biblioteca |
| Máquina de estados da automação | Enum de tarefas + fila com prioridade em `zen-core` (puro) | Resolve os itens 2 e 10; sem biblioteca externa |
| Testes com prints reais | Pasta `tests/fixtures/` por tela + teste de "golden" por detector | Já é o padrão de vida/Zen; estender a janelas |
| Testes de propriedade | `proptest` (já há testes de propriedade manuais em `zen-core/tests`) | Para parsers e rotas |
| Erros tipados | `thiserror` nos módulos do agente, no lugar de `String` | Permite decidir "tentar de novo" ou "desistir" pelo tipo |
| Lint e formatação no build | `cargo clippy -D warnings` e `cargo fmt --check` no `zen.cmd build` | Hoje só roda teste |
| Diferenças de estado no painel | JSON Patch (RFC 6902) gerado no central, ou eventos por char | Resolve o item 7 |
| Captura única por ciclo | Estrutura `Frame` compartilhada no agente | Resolve o item 8; sem biblioteca |
| Métricas | Contadores simples no relatório (`Health`): tentativas, falhas por motivo, tempo médio de volta | Vira painel de saúde sem Prometheus |

### 3.3 Não recomendado agora
- **Framework de front-end (React, Svelte):** exigiria etapa de build e quebra a decisão 0002; o painel tem ~600 linhas.
- **Banco cliente-servidor (Postgres):** volume não justifica.
- **OCR genérico (Tesseract) para tudo:** os modelos de pixel são mais rápidos e previsíveis; OCR só onde a fonte varia.
- **Qualquer biblioteca que escreva na memória do jogo ou injete código:** proibido pelas regras do projeto.

---

## 4. Diretrizes para sessões futuras

1. **Antes de mudar:** ler este documento, o `ROTEIRO.md` e as pendências; conferir o tamanho dos arquivos enviados ao PC1 antes de compilar.
2. **Regra pura primeiro:** toda decisão nova entra em `zen-core` com teste; o agente só observa e executa.
3. **Todo detector de tela nasce de um print real** guardado como fixture, com teste que prova que ele não dispara nos outros prints.
4. **Toda ação tem conferência e limite:** confere o efeito na tela, repete no máximo N vezes, registra o motivo da desistência e deixa o char em estado seguro (helper ligado ou zona segura).
5. **Nada de endereço ou offset de memória sem validação** em pelo menos dois processos do jogo.
6. **Publicar:** build + testes → canal de teste → observar no uso normal → pedir o OK do dono → produção.
7. **Testes no jogo:** pausar a automação dos dois chars do PC, e terminar com cada char de volta ao spot.
8. **Relatar com honestidade:** dizer o que foi testado no jogo, o que só passou em teste de código e o que não foi testado.

## 5. Estado atual e próximos passos sugeridos
1. Fila de automação com prioridade (item 2).
2. Tipo único de spot com mundo (item 5).
3. Relógio monotônico em toda espera (item 1).
4. Dividir `nav.rs` (item 4), sem mudar comportamento, com os testes reais repetidos em seguida.
5. Fixtures de janelas e teste do ciclo do Raven com os tempos novos.

## 6. Referência rápida
- **Janela do jogo:** 1030×797; centro (515,400); modelo pixel↔tile em `nav.rs` (`MODEL`).
- **Worlds:** Lorencia 1, Dungeon 2, Devias 3, Noria 4, Lost Tower 5, Arena 7, Atlans 8, Tarkan 9, Icarus 11, Valley of Loren 31, Aida 34, Crywolf 35, Kanturu 38.
- **Lista de personagens (RAM):** entradas de 0x598 bytes; a partir do nome: +0x4C classe, +0x2D4 vivo, +0x2E8 tipo (1 jogador, 2 monstro, 4 NPC), +0x390 posição (f32 x,y; tile×100).
- **NPCs conferidos:** Safety Guardian (Kanturu 34,217; Lorencia 146,110; Devias 218,63), Trainer (Lorencia 122,110), Chaos Goblin (Lorencia 129,117), Pasi (Lorencia 118,113).
- **Ice Storm:** skill 39, alcance 6 tiles, raio 4.
