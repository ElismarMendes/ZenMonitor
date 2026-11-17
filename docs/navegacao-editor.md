# Radar, navegação e editor de rotas

Implementação experimental na branch `codex/navegacao`, derivada de `main-v106`.
A referência estável está preservada conforme [base-main-v106.md](base-main-v106.md).

## Arquitetura

| Camada | Implementação |
|---|---|
| Contratos | `zen-core/src/routes.rs`: `Spot { world,x,y }`, observador, destino, bindings, revisões e plano |
| Terreno e A* | `zen-core/src/navigation.rs`, `terrain.rs`: terreno existente, segmentos sem cortar quinas, bloqueios temporários e orçamento de progresso |
| NPC Tracker | `zen-core/src/npc_tracker.rs`: tipo/classe/nome/posição conferidos em duas observações do mesmo processo, sessão e mapa |
| Agente | `agent/src/navigation.rs`: reutilização das amostras existentes, contexto, cancelamento e revisões explicitamente instaladas |
| Executor Windows | `agent/src/win/navigation.rs`: waypoints, chegada real, desobstrução e chamadas ao motor de mouse existente |
| Central | `central/src/navigation.rs` e `routes.rs`: radar, persistência SQLite e envio condicionado à prévia |
| Painel | `central/web/navigation.js`, integrado à aba Radar e rotas de `v4.html`; Canvas 2D e DOM mantido durante o WebSocket |

O protocolo 3 mantém a aceitação dos agentes 1 e 2 no central. Os novos comandos só
são enviados a agentes compatíveis. Atualizar o agente 3 requer atualizar primeiro
o central; o central antigo pode recusar o protocolo 3. A publicação +111 foi liberada
somente para PC1 e ELISMAR-NOTE; os demais agentes permanecem na +106.

## Isolamento e coordenadas

Cada radar usa somente uma combinação computador/personagem/PID/início do processo/
sessão/época de mapa. A posição e as entidades vêm do mesmo observador. A troca de
mapa exige duas amostras antes de habilitar seu uso; morte, DC, mapa desconhecido e
reconexão invalidam o contexto. Uma amostra com mais de 10 segundos não é utilizável.
O painel também envelhece as amostras se o WebSocket parar.

O leitor não fornece uma identificação confiável de instância. Por isso não há união
de entidades entre observadores, mesmo quando o nome do mapa coincide. `class` é a
classe/tipo do objeto do cliente, não um identificador global de instância de NPC.

Conversões preservadas: memória em unidades de 100 por tile; terreno `y*256+x`;
Canvas com X crescente para a direita e Y para baixo. São os mesmos eixos do minimapa
decodificado pelo projeto. O desenho do radar nunca é feito sobre o cliente MU.

## Edição e persistência

- Criar por clique, arrastar, excluir, reordenar e visualizar coordenadas de até 64 waypoints.
- Rotas por mapa, com associações a personagem/computador, spot ou NPC e referência a outra rota de retorno do mesmo mapa.
- Salvar, duplicar, excluir logicamente, consultar histórico e restaurar como nova revisão.
- Concorrência otimista: salvar uma revisão antiga recebe conflito; nenhum histórico é sobrescrito.
- Todas as coordenadas de uma rota e seu destino pertencem ao mesmo mapa.
- Linhas diretas tracejadas e segmentos A* contínuos têm cores diferentes.
- Prévia estática a partir do primeiro waypoint funciona sem personagem naquele mapa;
  ela não produz token de execução.

O central grava `route_revisions` e `route_runs` no SQLite existente. Editar ou salvar
não movimenta o jogo. A referência de retorno permite manter ida/volta como rotas
independentes; abrir a rota de retorno no editor permite gerar sua própria prévia.
Nenhuma cadeia de ida/volta é disparada automaticamente ao salvar uma associação.

## Envio e recuperação

1. Salvar a revisão e selecionar o observador no mapa correto.
2. Gerar a prévia com sua posição atual. A* roda fora do executor HTTP, com no máximo
   dois planejadores simultâneos e 8192 tiles de plano por rota.
3. Confirmar explicitamente personagem, PC, mapa, destino e eventual instalação.
4. O central consome um token de uso único, válido por 30 segundos, e confere revisão,
   contexto, posição inicial, hash de terreno, conexão e compatibilidade do agente.
5. O agente valida novamente o contexto e cada chegada. O status "enviada" não é sucesso;
   somente `RouteResult` após a confirmação física encerra o trajeto.

Execução exige `navigation_execution=true` no central e a máquina no canal de teste.
O agente exige `navigation_enabled=true`, automação habilitada e personagem permitido.
Ambas as opções de navegação são falsas por padrão. O endpoint genérico de comandos
recusa o envio direto de `RunRoute`; o fluxo de prévia é obrigatório.

A instalação para recuperação só aceita destino Spot e só ocorre após execução
bem-sucedida. O agente persiste a revisão por personagem, mapa e coordenada, permitindo
múltiplos destinos. As rotinas de recuperação existentes continuam responsáveis pelo
Move, morte, DC, Fortuna e Raven. Depois do Move confirmado, podem usar os waypoints
explicitamente instalados para aquele destino. Catálogo de spots e posse compartilhada
continuam nas estruturas existentes. Excluir uma rota revoga a execução/instalação e
a reconexão sincroniza os IDs ativos, inclusive para agentes que estavam offline.

## Movimento e segurança do trajeto

O executor utiliza `Session`, `Hold`, `SendInput`, proteção do cursor, conversão de
pixels e proteções contra cliques em NPCs do código existente. O caminho legado
permanece como padrão com navegação desativada. Não há escrita de memória, injeção,
alteração de pacotes ou comando remoto arbitrário.

O mapa real é conferido nas leituras de posição. No spot, a tolerância máxima é
**2 tiles**. Não se faz reajuste se a chegada já é válida; um destino estacionário
também não desliga um Helper que já estava funcionando. Ocupação é conferida na
aproximação e na chegada, reutilizando posição; o retorno ao spot próprio preserva
as regras de posse/KS existentes.

Em trânsito, a posição usa o leitor existente; objetos não são relidos por clique.
As listas de NPCs são reaproveitadas da telemetria. Somente a interação final exige
duas leituras rápidas adicionais, seguida da confirmação visual da janela.

Para NPCs, a chegada usa a interseção entre o raio do NPC e a área da coordenada
de interação (no máximo 3 tiles, separado da tolerância de 2 dos spots). Se a janela
não abrir de um lado, há uma única aproximação à posição de interação cadastrada,
usando o motor legado; até três alturas de clique já conhecidas são tentadas, cada
uma com posição estabilizada e duas novas leituras do NPC. Nenhuma etapa seguinte
é liberada sem confirmação da janela. Ir a um destino manual mantém a suspensão do
retorno automático, como o “Ir até” existente; voltar ao próprio spot libera essa suspensão.

Ausência de progresso por 1,8 s ou oscilação provoca liberação do botão. Há até dois
replanejamentos por segmento, duas recuperações de janelas por rota, limite de 120 s
por segmento novo e 240 s por rota. O A* é reutilizado até ser necessário replanejar.
Bloqueios são temporários e não modificam o `.att` original. NPCs não viram paredes
pela simples presença: após uma janela acidental confirmada, um pequeno contorno
temporário evita cruzar novamente sua área clicável, preservando o destino de interação.

Se o novo trecho esgotar o orçamento de replanejamento, há **um** recurso ao motor
legado por rota, mantendo o waypoint e a validação obrigatória de chegada. A decisão
é registrada como `NAV_LEGACY_FALLBACK`; uma segunda necessidade interrompe a rota.
O limite global e as guardas de conexão, contexto, pausa e morte continuam ativos.

A coordenada comprovada da chegada é registrada **antes** de ativar o Helper.
Depois de ativado, o Helper pode perseguir monstros e se afastar do tile; esse movimento
de farm não invalida retroativamente a caminhada que já chegou a até 2 tiles. O mapa
continua sendo conferido e a confirmação de Helper ligado usa a rotina visual existente.

## Verificação e implantação

Os testes abrangem conversão de coordenadas, mapa divergente, paredes/quinas, múltiplos
waypoints, expiração de bloqueios, travamento/oscilação, confirmação de NPC, isolamento,
persistência/revisões e os bloqueios de envio por falta de confirmação, nonce expirado,
reutilização, mudança de contexto, personagem ocupado e falha de comunicação.

Laboratório local em `127.0.0.1:18787`, separado do central e das configurações estáveis.
`zen-radar` é um observador temporário somente para leitura, restrito a esse laboratório
e ao x9901. O executor de teste usa configuração própria com automações espontâneas
desabilitadas. Os artefatos de execução ficam em `target/navigation-test/` (ignorados no Git).

A inspeção visual do painel pelo navegador ficou bloqueada pela autorização do navegador.
Isso não foi contornado por outro navegador/driver. Os testes de API e de matemática
da interface são independentes dessa inspeção, que segue como critério antes da produção.

Publicação geral continua dependendo de validação no PC1 e autorização do dono. A +106
e seu backup devem permanecer disponíveis para retorno.

### Resultado da rodada de 15/11/2026

| Verificação | Resultado |
|---|---|
| Workspace Rust | 231 testes aprovados, nenhum erro |
| Painel | 6 testes JavaScript aprovados: coordenadas/isolamento, preservação de rascunho/rolagem, edição durante salvamento e prévia sem movimento; DOM simulado, sem navegador/rede |
| API real de rotas | Autenticação, prévia, confirmação, ida/retorno, histórico, restauração e exclusão lógica conferidos no laboratório |
| Radar real | PC1/x9901, PID e processo identificados; entidades de Kanturu isoladas |
| Move para Lorencia | M legado com mapa e chegada confirmados, sem desligar Helper antes do teleporte |
| Caminhada e NPC | Waypoints até o Trainer; classe 226, NPC `(122,110)`, janela Trainer confirmada por memória e tela |
| Obstáculos reais | Baú aberto ao cruzar o Guardian e estrutura da entrada de Kanturu identificados; fechamento, contorno temporário e replanejamento conferidos |
| Retorno | Trajeto de até 142 tiles/8 waypoints percorrido; Helper ligado e prova de chegada separada do movimento posterior de farm |
| Chegada final | Nova execução confirmou `(160,196)`, a 1 tile de `(160,197)`, com Helper já ligado; sem desligamento/reajuste redundante |
| Persistência no agente | Revisão instalada por personagem/mapa/destino e preservada entre reinícios do trabalhador |
| Comunicação | Encerramento suportado do trabalhador invalidou a prévia anterior: HTTP 422, nenhum movimento reenviado |
| Encerramento do teste | Trabalhador experimental desligado; automação original do PC1 restaurada; cinco máquinas continuam na +106 |
| Painel visual | Usuário aprovou manualmente o visual em 15/11; inspeção automatizada ainda bloqueada pela permissão do navegador; nenhum contorno foi usado |

Os testes identificaram e corrigiram duas causas específicas: não se deve transformar
um NPC alvo em uma parede quando a interação já é possível; e a perseguição de mobs
pelo Helper não deve gerar falso erro de chegada depois de uma coordenada válida ter
sido comprovada. A tolerância de spot permaneceu em 2 tiles.

O teste abriu e confirmou a janela do Trainer; não executou uma compra/restauração de
vida do Raven. A identificação visual específica está homologada para o Trainer;
outros NPCs e a usabilidade visual do editor ainda precisam de homologação antes
de liberar a navegação em produção. O terreno do cliente não representa todos os
objetos 3D do servidor; esses bloqueios usam a detecção de falta de progresso e os
limites/fallbacks descritos acima.

O serviço HTTP do laboratório pode permanecer disponível localmente para consulta,
sem trabalhador conectado; as opções de execução foram desabilitadas nos arquivos
de configuração. O painel de produção continua na versão anterior.

Na retomada de 15/11, a ponte foi confirmada ativa e o central do laboratório foi
atualizado, agora com `navigation_execution=false` também no processo em execução.
Um observador temporário somente para leitura voltou a fornecer o radar do x9901.
A autorização informada no chat não removeu o bloqueio persistido do navegador;
a conferência visual segue pelo [roteiro manual](verificacao-visual-navegacao.md).

### Preparação do piloto após a aprovação visual

O usuário aprovou manualmente o visual do laboratório em 15/11/2026. Os binários
otimizados do piloto são compilados separadamente em `target/navigation-release`;
o manifesto local registra commit, hashes e destinatário PC1. Compilar não publica
uma atualização e não substitui os executáveis em uso.

Ordem de implantação do piloto:

1. Conferir `test_machines = ["PC1"]`, backup `main-v106`, hashes da publicação
   estável e cópia consistente da base/configuração do central.
2. Atualizar primeiro o central compatível com os agentes anteriores. Confirmar
   as cinco reconexões e manter `navigation_execution=false` na partida.
3. Publicar o agente exclusivamente no canal de teste e confirmar a versão no PC1;
   os outros quatro computadores devem continuar na +106.
4. Habilitar o experimento apenas no PC1 (`navigation_enabled=true`), sem alterar
   atalhos, automações ou configurações dos outros agentes. A habilitação do central
   continua sujeita ao filtro de máquinas de teste e à confirmação de cada envio.
5. Repetir o cenário autorizado com x9901: Move para Lorencia, Trainer por waypoints,
   confirmação da janela e retorno a Kanturu `(160,197)`, tolerância máxima de
   dois tiles, Helper ligado. Não executar compra/restauração de Raven como parte
   deste cenário.
6. Conferir ausência de comandos repetidos, resultado persistido e isolamento do
   ReDMooN. Em falha, desabilitar o experimento e usar a restauração assinada da
   +106 no canal de teste; não reduzir manualmente a sequência de publicação.

A publicação geral é uma etapa posterior ao piloto. A aprovação visual manual não
substitui a homologação específica de interação com outros NPCs.

### Validação dos binários otimizados — 15/11/2026

O central compilado em release foi instalado pela ponte somente no laboratório
`127.0.0.1:18787`, sem trabalhadores conectados e com execução de rotas desabilitada.
O hash instalado corresponde ao manifesto `target/navigation-release/piloto.json`.
O teste real dessa release confirmou:

- As 16 rotas existentes permaneceram disponíveis após o reinício.
- Prévia sem autenticação foi recusada (401); pontos de mapas divergentes foram
  recusados (422); envio sem confirmação explícita foi recusado (400).
- A prévia estática calculou o caminho sem emitir token executável.
- Os arquivos do painel servidos pela release correspondem ao código atual.
- O laboratório permaneceu limitado ao PC1 e sem execução habilitada.

A evidência está em `target/navigation-release/smoke-result.json`. Não foi enviada
rota ao jogo nem publicada atualização dos agentes nesta rodada. Nova tentativa
normal do navegador integrado continuou bloqueada por permissão salva/política;
a aprovação visual manual permanece distinguida da inspeção automatizada.

### Piloto release e ampliação autorizada — 15/11/2026

O piloto encontrou uma condição na recuperação com rota instalada: após o Move,
o mapa já estava correto na tela, mas faltava a segunda amostra do coletor. A chamada
imediata falhava com `NAV_STALE` e repetia a recuperação cedo demais. Agora essa
integração espera até 20 segundos pelo cache normal, com validação de personagem,
PID, mapa e idade, cancelamento por pausa/desconexão e nova confirmação visual do
mapa antes da caminhada. Não há capturas ou leituras adicionais a cada espera.

No x9901, o percurso de aproximação por waypoints passou. A interação configurada
com raio 3 falhou por clicar no cenário e foi interrompida pelo limite de correções.
Uma rota separada, validada antes do uso, definiu a interação em `(122,109)` e raio
1 para o Trainer em `(122,110)`. Essa configuração abriu a janela Trainer e foi
confirmada por memória e captura. Isso calibra a interação desse NPC; não muda a
tolerância máxima de dois tiles dos spots nem o motor de cliques.

A recuperação com a rota instalada passou pelo Move e retornou ao spot, confirmando
`(158,199)` antes de ligar o Helper. A leitura final confirmou Kanturu `(160,197)`
com Helper ligado. O executor isolado foi encerrado e a automação original do PC1
foi restaurada. Evidências locais: `target/navigation-release/pc1-final-state.json`,
resultados por comando e captura `passos_x9901_release-trainer-confirmed.png`.

O usuário incluiu o ELISMAR-NOTE como teste adicional para FirstHand e YouSee;
PC1 continua sendo a máquina oficial de teste. Foi criado backup consistente da
base/configuração/binário do central antes de sua atualização compatível, e a
publicação assinada +107 foi enviada apenas ao canal `[PC1, ELISMAR-NOTE]`.
Os outros três agentes e a publicação estável +106 foram preservados.

O opt-in remoto usa somente `set_config.navigation_enabled`. Ativar exige central
habilitado, máquina no canal de teste e agente com protocolo 3; desativar invalida
as aprovações e interrompe a rota experimental. A opção não agenda retorno legado
por si só e não habilita a automação. O envio de cada rota mantém a prévia e a
confirmação explícita. Os 232 testes Rust passaram, incluindo o bloqueio do opt-in
fora do canal, agente incompatível e central desabilitado.

No ELISMAR-NOTE, as duas sessões foram isoladas por personagem/PID e mapas distintos:
FirstHand em Lost Tower e YouSee em Dungeon. As rotas aprovadas confirmaram o Trainer
em Lorencia para ambos, em `(122,109)` e `(123,109)`, respectivamente. A restauração
manual do FirstHand confirmou `94% -> 100%` e retorno a Lost Tower; a rota de spot
confirmou depois `(55,9)`, dentro da margem de dois tiles de `(54,9)`.

O primeiro ciclo manual do YouSee falhou após abrir o título do Trainer sem confirmar
o texto de atendimento. A rotina abortou e voltou à Dungeon, sem clicar no botão
ausente. A publicação de teste +108 acrescenta uma única reabertura do menu, sem
refazer a caminhada: pausa, mapa/posição estáveis e duas leituras concordantes de
um único NPC Trainer classe 226 são exigidos antes de clicar novamente. Persistindo
a falta do botão, retorna erro e registra a captura; não continua cegamente.
Os 60 testes do agente passaram. As capturas de referência de FirstHand e YouSee
testam a identificação do menu pronto; uma área sem texto deve ser recusada.

Na repetição com +108, o YouSee falhou antes do menu: a aproximação legada aceitou
`(122,107)`, ao norte da posição calibrada, e os cliques não abriram o Trainer.
O personagem voltou à Dungeon com Helper confirmado. A +109 integra a aproximação
calibrada à própria ação de Raven quando `navigation_enabled=true`: aguarda o mapa
confirmado no cache, calcula o caminho até a área de interação `(122,109)`, raio 1,
e exige duas leituras do NPC e a janela antes de continuar com Restore life.
O motor de mouse e o fluxo de restauração permanecem os existentes. Quando o
experimento está desligado, a aproximação legada é preservada. Os 234 testes Rust
passaram, incluindo caminhos a partir dos quatro pontos reais desta rodada.

### Resultado final do piloto +110

O teste do FirstHand expôs outra condição: um clique segurado podia avançar para
além do início do caminho em cache. A seleção rejeitava o caminho pelo primeiro
ponto distante e esgotava as tentativas sem aproveitar pontos seguintes próximos.
A +110 procura um ponto próximo alcançável ao longo do caminho, preservando as
quinas e o alcance máximo. Se nenhum ponto for alcançável, solta o clique antes
de reler a posição; os dois replanejamentos e o único fallback legado continuam
limitados. O caso de chegada `(118,137)` entrou na regressão, junto à validação de
que um caminho antigo ainda fornece próximos passos após avanço do personagem.

| Cenário na +110 | Resultado |
|---|---|
| FirstHand — Raven manual | Ciclo concluído com `100% -> 100%` (já estava cheio); voltou a Lost Tower, chegada `(55,10)` a 1 tile de `(54,9)`, Helper confirmado ligado |
| YouSee — Raven manual | `99% -> 100%`, voltou à Dungeon; leitura após a conclusão `(40,55)`, Helper confirmado pelo fluxo de recuperação |
| Recuperação efetiva em rodada anterior | FirstHand `94% -> 100%`; YouSee `82% -> 100%` |
| Isolamento no notebook | FirstHand/PID 23788 em World5 e YouSee/PID 8444 em World1 durante o deslocamento; observações e entidades separadas |
| Atualização | PC1 e ELISMAR-NOTE confirmaram +110; os outros três computadores permaneceram na +106 |
| Base estável | Tag `main-v106`, backup e executável publicado estável +106 com hash original preservados |
| Testes Rust | 234 aprovados, nenhum erro |

Uma solicitação do FirstHand teve leitura momentaneamente desconhecida da barra e
foi recusada antes de mover; a confirmação seguinte permitiu iniciar o teste. Isso
foi distinguido de falha de caminho e não foi reportado como restauração concluída.
O movimento posterior do Helper durante o farm não altera a prova de chegada nem
amplia a tolerância máxima de dois tiles.

O laboratório ficou sem executor; a automação original dos PCs está ligada. O
opt-in de navegação permanece limitado às máquinas de teste. O painel principal
em `:8787/#navigation` é o local para acompanhar o radar dos agentes atualizados;
`:18787` continua sendo o laboratório, que depende do observador temporário.
Evidências privadas locais: `target/navigation-release/final-pilot-evidence.json`,
`piloto.json`, resultados por comando e capturas. A inspeção automatizada do
navegador continua bloqueada; não foi contornada pela ponte.

### Painel vazio apesar de conexão ativa — 15/11/2026

O servidor principal entregava 10 personagens por HTTP e WebSocket, mas foi
reproduzido um bloqueio na primeira exibição: o campo de token em foco acionava
a proteção global de edição e mantinha o conteúdo vazio. O horário de recepção
já era atualizado, por isso o indicador ainda dizia “ao vivo”.

A proteção agora preserva controles de `main`/drawer já desenhados, sem bloquear
a primeira carga nem a digitação no cabeçalho. Erros de renderização deixam de
ser descartados silenciosamente: aparecem no aviso e no indicador. Conexão sem
personagens é exibida como “conectado · sem personagens”. Os testes de edição,
rolagem, foco e primeiro desenho passaram. No teste com os dados reais, o mesmo
cenário passou de zero para dez personagens desenhados, e o WebSocket confirmou
cinco computadores e quatro observadores de radar.

O central/painel principal e o laboratório receberam o executável corrigido com
backup e conferência das reconexões. A publicação estável +106, a de teste +110
e todos os tokens permaneceram preservados. O laboratório continua sem agente
conectado; o endereço dos dados reais é `http://127.0.0.1:8787/`.
A conferência do JavaScript foi por simulação de DOM e API, sem automatizar o
navegador cujo acesso estava bloqueado.

### Radar permanecia em Lorencia ao escolher personagem — 15/11/2026

Foi identificado um segundo problema, específico do radar: o mapa exibido era
sempre `draft.world`, inicialmente Lorencia. Selecionar um observador em Kanturu,
Lost Tower ou Dungeon não alterava esse mapa, e as entidades eram corretamente
ocultadas por pertencerem a outro mundo. O campo de nome da rota também não
identificava claramente sua finalidade.

Agora o radar seleciona inicialmente um observador confirmado e acompanha seu
mapa real. A seleção está identificada como **Personagem / computador**. A opção
**Acompanhar mapa do personagem** também segue teleportes confirmados; **Ver mapa
da rota** mantém disponível a edição estática. Nome e mapa da rota têm rótulos
próprios. Pontos, nomes e revisões existentes são preservados quando o observador
muda de mapa. Rotas de outro mundo são ocultadas na visualização do personagem,
e cliques nesse mapa não adicionam pontos ao mundo da rota antiga. Prévia e
confirmação continuam obrigatórias para qualquer envio ao agente.

Nove testes JavaScript passaram, incluindo seleção inicial, mudança de mapa,
isolamento, preservação do rascunho e ausência de movimentos automáticos. Os
quatro fluxos reais foram reproduzidos no JavaScript servido pelo central:
FirstHand/Lost Tower, YouSee/Dungeon e ReDMooN+x9901/Kanturu. O teste geral continuou
desenhando os dez personagens com o campo de token em foco. Os dois centrais foram
atualizados pela ponte com backup; agentes +106/+110 e tokens não mudaram.
Evidência local: `target/navigation-release/radar-map-result.json`. Estes testes
usam DOM simulado e API, sem controlar o navegador bloqueado.

### Revisão e próxima etapa — publicação +111

Após a confirmação manual do usuário de que o radar passou a funcionar, foram
revistos os contratos, contexto do agente, caminho/cache, prévia e envio, SQLite,
editor e implantação. A revisão corrigiu:

- Invalidação por morte/desconexão do personagem: agora afeta somente aquele
  personagem, preservando o radar dos demais no mesmo PC. A reconexão do agente
  continua invalidando todos os seus observadores. São exigidas duas novas amostras
  após invalidar; início do processo desconhecido não autoriza navegação.
- Associação de uma rota: observar outro personagem não altera sua associação
  existente ao salvar. O painel mostra a associação e oferece uma ação explícita
  para mudá-la. Salvamentos continuam sem enviar movimento.
- Terreno/rede: downloads em andamento são compartilhados; falhas têm intervalo
  de 30 segundos antes de nova tentativa automática.

A etapa seguinte implementada foi o histórico persistente de execuções do editor:
`GET /api/navigation/runs`, protegido pelo token de admin, com filtros por máquina,
personagem e rota. O banco registra personagem, início e término. A interface
distingue envio aguardando chegada, conclusão confirmada, falha e interrupção.
Atualizações são limitadas a cada 15 segundos, além de ações explícitas e resultados;
respostas atrasadas de outro personagem são descartadas. Registros antigos não
ganham um personagem inferido de sua rota: aparecem como não identificado.

A migração foi testada primeiro no laboratório, preservando 17 rotas e 18 execuções.
Depois foram atualizados o central e somente os agentes PC1/ELISMAR-NOTE para +111.
No x9901, uma rota curta até o próprio spot confirmou `(158,199)` em Kanturu, dentro
dos dois tiles de `(160,197)`, com Helper já ligado e sem alterná-lo. A conclusão foi
conferida no endpoint e por conexão SQLite independente, com personagem `x9901`,
estado `concluída` e término registrado. O filtro do ReDMooN não retornou essa execução.

Validação: 238 testes Rust, 12 testes JavaScript, regressões do painel e reprodução
dos quatro fluxos reais aprovados. `PRAGMA quick_check` retornou `ok`. Os cinco PCs
ficaram conectados e com automação ligada; os três fora do piloto continuaram +106.
O hash da publicação estável e a tag `main-v106` foram preservados. Evidências locais:
`target/navigation-release/review111-live-result.json` e `review-backup-path.txt`.

Próximos critérios antes de publicação geral: mostrar no painel as revisões realmente
instaladas por personagem; homologar outros NPCs além do Trainer; executar a medição
prolongada de CPU/rede e o replay/caos já previstos no roteiro. Esta rodada não matou
personagens para testar invalidação: esse caso foi exercitado com relatórios simulados
e coberto por regressão. A inspeção automática do navegador permanece indisponível;
a confirmação visual do radar é a fornecida pelo usuário.
