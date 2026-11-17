# Conferência visual do radar e editor

Laboratório local: **http://127.0.0.1:18787/#navigation**.

Após a implantação do piloto de 15/11, a aba também está disponível no painel
principal: **http://127.0.0.1:8787/#navigation**. O ELISMAR-NOTE fornece radar de
FirstHand e YouSee, cada um isolado em seu próprio observador. Selecione o
personagem e o mapa correspondente (Lost Tower ou Dungeon quando estiverem nos
spots). O laboratório `:18787` pode estar sem dados quando seu observador
temporário estiver encerrado; isso não significa falha no painel principal.

Este painel é separado do painel de produção em `:8787`. A execução de rotas está
desabilitada no servidor. O observador temporário do x9901 apenas lê posição e
entidades; não registra atalhos e não envia teclado ou mouse ao jogo.

## Conferência manual

1. Abra a aba **Radar e rotas**, selecione **PC1 / x9901** e escolha **Kanturu** no campo de mapa.
2. Confira o marcador branco do personagem, nomes/distâncias das entidades e os
   filtros de jogadores, monstros, NPCs e alcance. Os dados atualizam a cada ciclo
   de leitura; amostras antigas deixam de ser exibidas.
3. Crie três waypoints por clique, arraste um ponto, altere sua ordem pelas setas e
   exclua um ponto. Confira as coordenadas exibidas ao lado de cada waypoint.
4. Digite um nome de rota e aguarde pelo menos duas atualizações do radar. O texto,
   os pontos, a seleção e a rolagem devem permanecer.
5. Para conferir **Salvar**, **Duplicar** e **Histórico**, preencha o token do
   laboratório no campo do próprio painel. Ele está no `central.toml` em
   `C:\ZenMonitorNovo\target\navigation-test\central`; não é o token da produção.
   Use uma rota nova com nome iniciado por `QA visual`, para distinguir os dados do teste.
6. Salve, altere um waypoint e salve de novo. Abra o histórico e restaure a primeira
   revisão. A restauração deve criar uma nova revisão, mantendo o histórico anterior.
7. Use **Validar / prévia A\***. A linha calculada azul deve se distinguir da linha
   direta amarela tracejada; tiles bloqueados aparecem em cinza. Um destino sem
   caminho deve mostrar erro, sem enviar movimento.
8. O botão **Confirmar envio ao agente** deve permanecer desabilitado neste laboratório.

O observador encerra automaticamente após 15 minutos. Se isso ocorrer, o painel
deixa de ter radar ao vivo, mas o rascunho e as rotas salvas continuam disponíveis.

## Estado da automação da revisão

A ponte foi reaberta e verificada em 15/11/2026, e o serviço do laboratório foi
atualizado com a implementação atual. A inspeção pelo navegador automatizado
permanece bloqueada: a ferramenta informa uma permissão salva ou política do
ambiente que nega a origem local, inclusive após a liberação informada no chat.
Não foi usado outro navegador, driver ou acesso indireto para contornar esse bloqueio.

Os testes de estado do editor usam DOM simulado e não substituem esta conferência
visual. A versão +106 e a automação do PC1 em produção permanecem preservadas.

## Aprovação visual e próxima etapa

Em 15/11/2026, o usuário aprovou o resultado visual com “ficou bom, continue”.
Essa aprovação é manual; não representa uma inspeção automatizada pelo navegador
nem confirma individualmente todos os passos de edição acima.

A próxima etapa é preparar os executáveis de release em
`target/navigation-release`, separados dos binários em uso. O piloto continua
restrito a PC1/x9901; a publicação geral e a base `main-v106` permanecem preservadas.

### Permissão persistida do navegador

A documentação oficial informa que uma negativa pode ficar salva nesta conversa,
mesmo depois de selecionar permitir nas configurações gerais. Nas configurações do
aplicativo, confira os sites permitidos/bloqueados para `http://127.0.0.1:18787`.
Se a negativa persistir, mantenha esta conversa e abra o mesmo site em uma nova
conversa, aprovando a solicitação normal. Políticas do administrador continuam
valendo e precisam ser revistas pelo administrador quando aplicáveis.

Fonte: [OpenAI — navegador integrado e permissões](https://help.openai.com/en/articles/20001277-using-the-built-in-browser-in-the-chatgpt-desktop-app).
