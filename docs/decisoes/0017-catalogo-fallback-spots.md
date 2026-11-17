# 0017 — Catálogo, F1/F2 e fallback coordenado

## Integração

As políticas ficam em `zen-core/src/spots.rs` e `ks.rs`. O agente usa `spot_service.rs` para comunicação/persistência e `win/spots.rs` para orquestrar a navegação existente. Os leitores de memória, jogadores e monstros e o executor SendInput/Hold não foram substituídos.

F1 atribui a coordenada capturada ao principal e a inclui no catálogo do mapa. F2 só cataloga. Ctrl+D/Ctrl+S não acionam mais esses atalhos. Apenas eventos físicos, sem modificadores e com o jogo em foco são aceitos; repetição por tecla segurada é descartada. Os pixels são capturados no evento. A confirmação de F1 depende da reserva do central (prazo de 5 s), sem reler uma posição posterior.

As listas existentes `catalog` e `catalog_maps` são preservadas. A posição na lista define a prioridade. A identificação é `world:x:y`; os dados estão também em `spot_catalog` no estado do painel. O catálogo compartilhado conserva as fontes por PC: excluir uma entrada na origem a retira da busca, salvo se outro PC ainda a catalogou.

## Grupo e exclusividade

O central associa personagens ao socket autenticado de cada máquina e mantém nomes, principais, ativos e catálogos em `spot-directory.json`. Desconectar não apaga a propriedade. Remover explicitamente um PC libera seus spots, mas conserva os nomes como próprios.

A reserva passa por um único mutex e é persistida antes do ACK. Outro personagem não pode reservar até dois tiles de um principal/ativo/reserva do grupo. Reservas em trânsito duram 90 s, renovadas a cada 20 s. O pedido só pode usar um personagem informado pelo próprio PC autenticado.

O agente conserva o grupo em `spot-group.json` e o temporário em `spot-fallback.json`. Um diretório sem atualização há 45 s impede novas decisões coordenadas. Ausência de leitura de jogadores não significa ausência de jogadores.

## Chegada e movimento

Clique, botão segurado, projeção e detecção existentes são reutilizados. A aproximação consulta o mesmo leitor de objetos a até 12 tiles do destino; um jogador externo a até seis tiles caracteriza ocupação na chegada. Nomes do grupo, nomes locais e `friendly_chars` não são invasores.

O move exige nome do mapa e região calibrada da entrada (spawn pode variar até 16 tiles). O retorno ao spot exige mapa correto e posi??o dentro da margem de tr?s tiles antes de ligar o Helper. Com monstros, dois tiles j? encerram a aproxima??o; sem eles, aceita um tile ap?s dez segundos de ajuste. Não se substitui posição ilegível pelo destino desejado.

Uma chegada nova ocupada libera a reserva e tenta o próximo spot que não pertença ao grupo, na ordem do catálogo. Esgotado o mapa, consulta os secundários configurados. Cada movimento tem no máximo uma retentativa externa: registra motivo e janelas, fecha obstruções com as rotinas existentes e repete. O move conserva duas tentativas internas. Cancelamento, pausa, leitura ilegível ou falta de reserva não são sucesso. A varredura é finita; sem destino livre confirmado informa o motivo e aguarda a recuperação, sem escolher às cegas.

F1 cancela a geração anterior do trajeto e remove o hold de “Ir até aqui”. Spots explicitamente travados não mudam por fallback.

## Principal e temporário

O principal permanece em `agent.toml`; fallback não o sobrescreve. Recuperações, retorno da Fortuna/Raven e relatórios usam o ativo. Em outro mapa, o agente agenda nova busca no principal após `spot_primary_retry_s`. Não inicia a varredura durante outra automação, pausa, desconexão ou retorno pendente após morte.

Configuração padrão, com serde/default e sem migrar as listas existentes:

```toml
spot_fallback = true
spot_secondary_maps = ["Arena", "Lost Tower", "Dungeon"]
spot_primary_retry_s = 600
friendly_chars = []
```

Os secundários precisam de entradas catalogadas por F2 e terreno/teleporte reconhecidos. O principal é o mapa do spot definido pelo usuário com F1. Em cada mapa vale primeiro a ordem local; entradas compartilhadas adicionais vêm depois. Ao voltar ao principal, o temporário é liberado pelo novo relatório.

## KS e PK

KS enquanto o char já farma não chama movimento. O status compara uma base limpa de até cinco minutos (mínimo 120 s) com 120 s de presença externa. Queda de pelo menos 20% é informada como coincidente com KS; não é prova causal nem gatilho de fuga. Sem base, leitura confiável ou com Zen no limite, o resultado é inconclusivo.

Retomada no spot já estabelecido e retorno após morte preservam a posse, inclusive com jogadores externos. A detecção da morte e as opções existentes de retorno/pausa continuam iguais; não há fuga baseada na causa PK. Chegada nova em trânsito verifica ocupação normalmente.

## Compatibilidade e validação

Protocolo 2. O novo central aceita protocolo 1 para os recursos anteriores e só envia o diretório novo a agentes de protocolo 2. Atualizar o central antes dos agentes é necessário para a coordenação. Um agente novo não deve ser publicado sozinho contra o central antigo.

Os testes cobrem remapeamento de atalhos, F2 preservando retorno, reservas concorrentes entre PCs, identidade autenticada, propriedade principal/temporária offline, expiração, remoção, fallback entre mapas, agenda de retorno, KS/PK e mapa/coordenada incorretos. O teste de ponta a ponta usa servidor e WebSocket locais temporários; não movimenta personagens reais. A implementação ainda precisa de validação no jogo antes de publicação.

## Painel web — atalhos e operação compartilhada

A tela Mapas usa o mesmo catálogo compartilhado para a tabela e o desenho, com prioridade e ID por mapa/coordenada. Posse principal, posse ativa e reserva vigente identificam nome e computador. A leitura de jogadores externos desconsidera os nomes do diretório, sem diferenciar maiúsculas/minúsculas. Verde exige um observador online próximo com leitura de objetos disponível; cinza significa ausência de confirmação, e não vaga livre. Spots próprios mantêm a indicação de reserva mesmo offline ou com presença externa.

Cards, tabela e ficha mostram a análise de KS enviada pelo agente, sem converter medições inconclusivas em afirmações de perda. O painel apresenta busca no catálogo, retorno após morte/PK e uso temporário de outro mapa. O campo opcional `primary_retry_at` transmite o prazo real do agente, junto do `home_spot`; agentes anteriores continuam compatíveis e mostram prazo não informado em vez de uma contagem inventada.

Atualizações de rede respeitam controles em foco, menus, o intervalo de interação e botões sob o mouse, inclusive quando houver resultado de atalho ou alteração do catálogo. O redesenho preserva rolagem do painel, catálogo, log e ficha. O filtro restaura foco e seleção no elemento novo. F1/F2 conservam as notificações imediatas de sucesso e erro.

Validação: `node scripts/test-painel-atalhos.mjs` cobre avisos, duplicação de eventos, edição em foco, posse entre computadores, reservas expiradas, leitura offline, catálogo real, KS, morte e contador de fallback. `cargo test --workspace --offline` valida compatibilidade e integração local; nenhum desses testes movimenta personagens reais.

## Correção de chegada exata — ReDMooN, 13/11 23:47

O log mostrou a caminhada encerrando em `(121,232)` por detectar mobs perto de `(123,233)`, enquanto a validação exigia distância zero. O mesmo encerramento antecipado se repetia depois de limpar e refazer o trajeto. A finalização também aceitava um tile de diferença após dez segundos, incompatível com a mesma validação.

A aproximação não encerra mais o movimento. Ao detectar mobs nos dois tiles finais, tenta uma limpeza com a skill existente uma única vez na aproximação e continua os cliques finais existentes. As correções por bloqueio e seus limites continuam ativos. Só a coordenada exata encerra a caminhada, e a confirmação independente de mapa/coordenada continua obrigatória antes de ligar o helper ou assumir o spot. O botão segurado no trajeto, projeção dos cliques, leitura e regras de ocupação permanecem iguais.

Os testes de regressão reproduzem a posição do log, verificam a transição limpeza → ajuste → chegada, a permanência da exigência de mapa correto e que mobs persistentes não causam um ciclo de limpeza antecipada. `cargo test -p zen-agent --offline`: 33 testes aprovados. Isso valida o fluxo em código; não substitui a observação da próxima chegada no jogo.

## Trainer — YouSee em tile NOMOVE de Lorencia

Os logs de 14/11 mostraram falhas repetidas antes de abrir o Trainer: `sem rota de (121,108) até (122,108)`. O terreno real do cliente marca a origem `(121,108)` com atributo 5 (zona segura + NOMOVE), embora o servidor permita o personagem parar nela. A posição de interação `(122,108)` é andável. O cálculo comum de caminhada rejeitava a origem; repetir o mesmo caminho não resolvia.

A restauração do Raven agora usa `walk_near`, a aproximação de NPC existente, que mantém botão segurado no trajeto, ajuste curto, limites de progresso e saída controlada de uma origem NOMOVE. Para o Trainer, aceita a posição de interação verificada a até um tile do ponto calibrado; isso não altera a chegada exata dos spots de farm. Cada tentativa de clicar no Trainer reconfirma Lorencia e essa posição de interação. A leitura da janela, botões de restauração e exigência de vida final do Raven de pelo menos 90% permanecem iguais.

`cargo test -p zen-agent --offline`: 34 testes aprovados. A regressão usa o arquivo real de Lorencia, reproduz a origem bloqueada do log e verifica rota de saída, posição próxima válida, rejeição de mapa errado, posição ausente e distância excessiva.

Validação no ELISMAR-NOTE com a publicação +92: uma primeira tentativa parou corretamente após um clique deslocar o personagem para fora da área de interação, sem declarar restauração. Na tentativa controlada seguinte, o YouSee abriu o Trainer em Lorencia, executou Restore life → Dark Raven e retornou à Dungeon. O agente confirmou `Raven restaurado: 0% -> 100%, de volta ao spot` às 00:45 de 14/11. O painel também informou `pet_hp = 100`. A falha original de rota na origem NOMOVE não se repetiu nas duas tentativas com a correção.


## Chegada ?gil ao spot ? 14/11

A pol?tica espec?fica de spots volta a aceitar chegada imediata a at? dois tiles com monstros pr?ximos. Sem monstros, busca o tile exato durante no m?ximo dez segundos; ap?s esse prazo, um tile ? aceito, e um eventual bloqueio dentro da margem segue diretamente ? confirma??o final (tr?s tiles), sem reiniciar a caminhada. Deslocamentos gen?ricos preservam a valida??o exata; a rotina de NPC/Trainer permanece independente.

A chegada confirma o mapa e a margem, rel? propriedade/ocupa??o e liga o Helper para assumir a limpeza. N?o executa novas caminhadas ou rodadas de skill na margem. As retentativas antigas de desobstru??o continuam limitadas e s? ocorrem fora dos tr?s tiles finais. Leitura inv?lida ou jogador externo detectado em chegada nova continua interrompendo a ativa??o do Helper e permite selecionar o pr?ximo spot pelo cat?logo. A regra de perman?ncia no spot j? estabelecido sob KS ? preservada.

A checagem de ocupa??o recebe a posi??o j? lida pela caminhada: consulta o leitor existente na primeira entrada do raio de 12 tiles e novamente ao concluir a chegada, antes do Helper. N?o captura a tela por conta pr?pria. Uma leitura de monstros nos dois tiles finais ? reutilizada durante os ajustes, sem repeti??o por clique. Cancelamento de movimento continua sendo verificado de forma leve a cada ciclo.

Valida??o: 37 testes do agente aprovados. As regress?es cobrem toler?ncia com e sem monstros, dez segundos, limite final de tr?s tiles, mapa incorreto, posi??o ausente, duas leituras de ocupa??o mesmo com cem ciclos de ajuste, falha de leitura n?o tratada como vaga e jogador que aparece entre a entrada e a finaliza??o.

## Move direto, mapas e Raven manual — 14/11

O Move abre M diretamente, sem desligar o Helper ou fechar o inventário como preparação. Fortuna e Raven deixam o Helper sob o desligamento automático do jogo ao entrar na zona segura; o retorno só o desliga para caminhar quando a posição confirmada está fora da zona segura. Fechamentos de inventário necessários às leituras de Zen e às operações de baú/NPC são independentes da preparação do Move.

Antes de caminhar, a navegação garante o mapa do terreno por teleporte e confirmação. Lugard, Guardião e Trainer também conferem mapa e posição de interação antes de clicar. O Trainer usa a caminhada completa com A*, desvios e tratamento NOMOVE existente para atravessar a cidade desde o Baú; somente o ajuste final usa o passo proporcional de NPC e tolerância de um tile.

O disparo manual do Raven é explícito e aceita qualquer porcentagem de vida. A condição de vida baixa permanece no disparo automático. Uma leitura ausente da barra é repetida até três vezes, com 150 ms entre tentativas, antes de recusar por ausência de barra; a mesma confirmação é usada após restaurar. Só uma restauração confirmada aplica 30 minutos de intervalo, inclusive se o retorno ao spot falhar depois da restauração. Falhas e tentativas automáticas que nem iniciaram têm apenas 60 segundos para evitar repetição contínua; pedidos manuais podem tentar novamente sem depender desse intervalo.

Validação: 40 testes do agente aprovados, incluindo os novos casos de pedido manual com vida alta, leitura transitória da barra, limite de três leituras, rota Baú → Trainer e cooldown apenas após restauração. A validação no jogo desta revisão ainda depende da instalação no canal oficial PC1.

## Proteções sem fallback e liberação de reservas — 14/11

`spot_fallback=false` agora restringe a seleção ao destino atual e passa pelo mesmo fluxo de reserva confirmada, cancelamento e checagem de ocupação. A opção desliga somente a escolha de alternativas; não permite entrar em um spot ocupado ou reservado para outro personagem.

Ao abandonar um destino, o agente remove sua renovação local antes de enfileirar a liberação. A liberação pode ser enviada mesmo com diretório desatualizado. Falha de envio, fila cheia ou ausência de ACK não recoloca o destino no heartbeat. Se o central não receber a liberação, a reserva remota expira após o prazo normal contado da última renovação recebida. Liberar um destino antigo não remove a renovação de um destino novo do mesmo personagem. Falhas remotas de liberação são registradas em log.

Regressões cobrem a seleção sem fallback, guardas de chegada, liberação sem diretório fresco, fila cheia, ausência de ACK e preservação de um destino novo ao liberar o antigo. `cargo test -p zen-agent -p zen-core --offline` passou: 43 testes do agente e as suítes do núcleo. Alterações ainda não publicadas nesta revisão.

## Auditoria dos requisitos de spots e limite de 2 tiles

A regra vigente de chegada é `zen_core::spots::ARRIVAL_RADIUS = 2`. A confirmação final, o reconhecimento de residência no spot, a retomada e a amostragem de KS usam esse limite compartilhado. Três tiles não confirmam chegada. A menção a três tiles nas decisões anteriores é histórica e foi substituída por esta correção explícita do usuário. A chegada com monstros continua imediata a até dois tiles; sem monstros, aceita um tile após dez segundos. Leitura e projeção dos cliques não foram alteradas nesta auditoria.

Amostras de KS de um personagem que continua farmando não são mais descartadas só porque outro personagem está em uma viagem. O reconhecimento automático inicial de posse continua aguardando o fim da automação em curso; a mudança afeta apenas a continuidade da medição e do status, sem gerar movimentos por KS.

Requisitos conferidos no código: F1 define/cataloga o principal; F2 apenas cataloga; teclas antigas não estão no gancho; reservas são confirmadas pelo central e a liberação abandonada interrompe renovação mesmo com falha de rede; checagem de ocupação ocorre na aproximação e na finalização; nomes do grupo são amigáveis; KS residente não provoca troca; PK preserva retorno conforme as opções de automação/morte do usuário; novas chegadas ocupadas tentam o catálogo e secundários e têm agenda de retorno ao principal. Retentativas são controladas e limitadas: se a leitura, o mapa, a posição ou a reserva não puderem ser confirmados, informa falha em vez de avançar cegamente.

O fallback exige spots secundários cadastrados, mapas reconhecidos, central disponível e opção de fallback habilitada. Desligar a opção restringe a seleção ao destino atual, mantendo as proteções de ocupação e reserva. Pausa manual, automação desligada e trava explícita de spot continuam sendo respeitadas. As alterações desta auditoria ainda estão apenas no código local e dependem de nova publicação/validação em jogo para chegar aos computadores.

## Desvio local de obstáculo na saída do bar de Lorencia

O log do ReDMooN de 14/11 às 14:06 registrou Move confirmado para Lorencia e bloqueio em `(130,130)` durante o trajeto para o Lugard. O usuário identificou uma estatueta nessa saída. O arquivo de terreno considera esse tile livre, com tiles NOMOVE próximos; portanto a identificação exata da colisão visual ainda requer validação no jogo.

A aproximação de NPC agora solta o botão ao detectar ausência de progresso, confirma mapa/posição e permite até dois desvios locais distintos. Os candidatos ficam a dois ou três tiles, precisam de segmento local livre, caminho A* curto e continuação válida até o destino. A direção que falhou não é repetida, nem é aceita uma continuação que volte pelo ponto onde o char prendeu. O movimento continua pelas rotinas existentes de botão segurado e ajuste curto; não altera os leitores ou grava bloqueios permanentes no mapa.

O limite global de 40 segundos, detecção de janela de NPC/baú, pausa manual, leitura implausível e confirmação final permanecem ativos. Sem saída validada ou após dois desvios, encerra com erro em vez de criar um loop. A regra de chegada ao spot de farm permanece em até dois tiles.

`cargo test -p zen-agent --offline`: 45 testes aprovados, incluindo regressão com o terreno real em `(130,130)`, validação da saída e continuação, destinos inválidos e limite de duas alternativas. Correção apenas no código local; ainda não publicada nem validada diante da estatueta no jogo.

## Janelas acidentais — CSRF/Zyro e x9901/Chaos Goblin

Evidências de 14/11: o CSRF repetia `não li o Zen do inventário`, inclusive ao tentar retornar de Lorencia. A captura do erro mostra o painel lateral Wandering Merchant Zyro aberto em (131,139). O x9901 interrompeu a Fortuna às 17:17 com `NPC: janela abriu no trajeto`; sua captura mostra a janela de combinação do Chaos Goblin, com menu central e inventários. Às 17:38 uma nova tentativa do x9901 terminou com duas Fortunas guardadas e retorno ao spot.

A detecção é aditiva: além da moldura central existente, reconhece o título ciano e o X do painel lateral (Zyro), e o título da loja de combinação do Chaos Goblin. O X sozinho não identifica NPC, porque o inventário usa o mesmo botão. Foram adicionados modelos cinza e capturas reais de regressão em `crates/agent/tests/fixtures/npc_side`.

O fechamento é limitado a três camadas, usando o botão reconhecido ou ESC, e reconfirma o desaparecimento. A leitura de Zen fecha obstruções, exige confirmação de inventário aberto e faz no máximo duas tentativas. Em falha preserva captura antes de retornar erro, sem inventar saldo ou prosseguir com conversão não verificada.

Durante a caminhada para NPCs, uma janela acidental solta primeiro o botão segurado. Depois do fechamento confirmado, reconfirma mapa e posição, escolhe um desvio local validado quando ainda está longe e retoma. No máximo duas recuperações de janela por trajeto, mantendo o orçamento global de tempo e os limites de desvio. Essa tratativa não desliga Helper nem fecha inventário como preparação do Move; não modifica leitura de memória ou a margem de dois tiles dos spots.

Validação: `cargo test -p zen-agent --offline`, 47 testes aprovados. As capturas reais distinguem Zyro de inventário e identificam a loja de combinação. Correções ainda no código local, sem publicação nem teste ao vivo desta revisão.

## Morte do LOBOLIXO e isolamento de entrada/captura — 14/11

Log do ELISMAR: às 18:10:15 locais o LOBOLIXO (PID 19944) morreu, com última amostra de HP 241. O chat identifica Genocider; a captura da morte 54 mostra Kanturu (212,39), perto do spot (211,38), e Helper desligado após a morte. O CSRF terminara a Fortuna às 18:09:43 com Helper ligado. Não existe registro de Home dirigido ao LOBOLIXO antes da morte; a versão instalada não auditava esse envio. A captura posterior não demonstra quem desligou o Helper nem quando, pois a morte pode encerrá-lo. A ausência de amostra contínua de farm deixou de ser descrita como prova de que o personagem não estava no spot.

Entrada agora possui reserva por thread, HWND e PID durante sessões, sequências e caminhada segurada. Chamadas internas do mesmo fluxo são permitidas; outra thread, outro personagem ou HWND reutilizado por outro processo falham antes de tomar o foco. Sessões não podem sobrescrever a sessão ativa. A identidade (PID e nome do personagem) e o foco são reconfirmados antes de teclas e cliques; um ponto coberto não recebe clique. O método de caminhada segurada permanece.

O controle do Helper mantém a reserva durante leitura/alteração, relê o estado depois de fechar janelas e antes do Home, e registra solicitação, tentativa e resultado. Cada Home registra HWND, PID e título da janela. Isso permite atribuir futuros comandos ao personagem correto sem inferir a origem apenas pela captura após a morte.

As capturas PrintWindow são serializadas. Quadros minimizados, incompletos, com PID alterado, janela movida/redimensionada ou origem alterada durante a captura são rejeitados. O bitmap é desselecionado antes de GetDIBits, e falhas de criação dos recursos GDI são tratadas. Mantidos PrintWindow, regiões e normalização existentes; sem mudança na leitura de memória ou detectores do jogo.

Validação: `cargo test -p zen-agent --offline`, 50 testes aprovados. Três regressões adicionais cobrem reserva aninhada, concorrência de threads e reutilização de HWND com outro PID. Alterações locais; ainda sem implantação ou teste ao vivo desta revisão.

O build de produção (`cargo build -p zen-agent --release --offline`) não conseguiu substituir `target/release/zen-agent.exe`: acesso negado (os error 5), executável em uso. O processo instalado não foi encerrado nem atualizado nesta análise.

## Cursor sobre o personagem nas trocas de janela

O cursor usa o mesmo centro de referência da caminhada `(515,400)`, convertido pela origem normalizada de cada janela. Ao trocar o foco, estaciona na janela do jogo que sai e na que entra; também reconfirma antes de teclas. Ao terminar uma ação, mantém o centro no jogo, inclusive ao devolver foco ao personagem anterior. Para outros aplicativos mantém a restauração anterior. Se o usuário tomou o foco ou pausou, não restaura por cima do controle dele.

Durante farm confirmado, o trabalhador verifica apenas o personagem em primeiro plano, no máximo a cada 10 segundos e após pelo menos três segundos sem entrada física. Centraliza se o cursor estiver a mais de quatro pixels do ponto; registra `CURSOR: reposicionado no centro do personagem durante farm`. Não alterna janelas em segundo plano para essa manutenção. Há um só cursor físico no desktop; cada janela é preparada quando recebe/devolve foco, sem prometer posições simultâneas diferentes.

As verificações usam HWND, título, origem, foco e janela sob o centro, sem captura extra, clique ou Home. A manutenção respeita automação desligada, F8, leitura pausada, personagens excluídos e sessão ocupada. Nenhum reposicionamento ocorre enquanto um botão do mouse estiver pressionado, preservando a caminhada segurada e os cliques existentes.

Validação: `cargo test -p zen-agent --offline`, 53 testes aprovados. Regressões adicionais cobrem intervalo/ociosidade/pausa/botão pressionado, tolerância de quatro pixels e origem em monitor com coordenadas negativas. Alterações locais, ainda sem publicação ou validação ao vivo.

Atualização posterior de 14/11: a validação real encontrou atraso na centralização ao devolver foco, corrigido usando a mesma confirmação de ativação da entrada. A revisão +101 passou no PC1, com centralização 184 ms após o retorno do foco. O mesmo binário foi promovido a +102 estável para todos às 18:47:41. Registro e limites da validação em [teste-cursor-pc1-102.md](../teste-cursor-pc1-102.md).

Na revisão seguinte, o cursor passou a acompanhar a área de mundo restante com inventário/atributos/expandido abertos, reaproveitando os quadros existentes em cache por janela/processo/época da interface. A referência da caminhada permanece intacta. Validado no x9901 e no agente instalado +103, com 59 testes aprovados; mesmo binário promovido a +104. Ver [validação de painéis laterais](../teste-viewport-x9901.md).
