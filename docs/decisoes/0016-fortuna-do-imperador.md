# 0016 — Zen vira Fortuna do Imperador (substitui o depósito no baú do Kanturu)

Data: 09/11/2026. Pedido do dono às 14:15–14:17.

## Decisão
Quando o char chega em 1,99B, o agente faz a viagem da Fortuna em vez de depositar no baú do Kanturu:

1. Desliga o helper e dá M → Lorencia (custa 2.000 zen).
2. No Lugard, a partir do tile (149,114): cada clique com ≥1B no inventário converte 1B na hora. A Fortuna vai para o Gremory Case (K). Cada conversão é conferida pelo Zen do inventário.
3. No baú de Lorencia (Safety Guardian, a partir de (146,112)), saca o que falta para 1B + 20M e converte mais uma.
4. No K, só nas linhas "Fortuna do Imperador" (conferidas pelo texto da linha e pelo texto da caixa): "Use It". A Fortuna vai para o inventário. A cada 20, e no fim: J → Fortuna do Imperador → Deposit All.
5. Volta com 20M (o move para Kanturu custa 20M) e anda até o spot.

## Regras de segurança
- A aba 1 do baú sempre guarda 1B. A conversão nunca saca abaixo disso; só o dinheiro do move (emergência) pode usar essa reserva.
- Se o char estiver em Lorencia com menos de 20M (inclusive na recuperação), primeiro busca no baú.
- O K é esvaziado em toda viagem, mesmo se a conversão falhar. Motivos: máx. 50 itens (o mais antigo é apagado) e prazo de 24–26 dias por item.
- Nunca clica em "Use It" sem ler "Fortuna do Imperador" na caixa. Se a caixa não mostrar Fortuna: Cancel e para.

## Medições (ponte, ReDMooN, 09/11)
- Safety Guardian no tile ~(144,5;110,5); Lugard em ~(149,0;115,9). O clique é calculado pelo modelo pixel→tile.
- Um clique no Lugard converte só 1B, mesmo com 2B no inventário.
- O Item Bank (J) é por char.
- Diag: `zen-diag fortuna <nome> [tudo]` (`tudo` converte o baú inteiro, menos a reserva).

## Correção da aproximação em 13/11/2026

O log do x9901 confirmou duas conversões, mas não tinha rastreio dos cliques intermediários. A revisão encontrou aproximação com exigência do tile exato por até 10 s, cliques com mínimo de 90 px e retorno ao mesmo tile em cada falha de interação. A rotina de Fortuna também tentava converter novamente após falha.

A aproximação de Lorencia agora aceita um tile imediatamente, usa passos proporcionais à rota (sem ângulos alternados), espera duas leituras iguais antes de clicar e interrompe retorno ao tile anterior ou duas tentativas sem movimento. Há limite de 32 passos e 40 s, verificado entre passos. Baú e Lugard fazem no máximo dois cliques de interação, sem refazer a rota. Uma falha encerra as viagens a NPCs; a limpeza do K/J continua sendo tentada antes de retornar o erro. As etapas ficam registradas no log.

A validação automatizada cobre distância dos cliques, tolerância de chegada e interrupção de oscilação. A calibração e o fluxo completo da nova aproximação ainda precisam ser conferidos no jogo.

### Movimento com botão segurado em Lorencia

O trajeto até três tiles do baú/Lugard usa uma única pressão do botão esquerdo, guiada pela rota a cada leitura de posição. Ao chegar perto, solta o botão e espera estabilizar antes dos ajustes curtos e do clique de interação. Não alterna soltar/pressionar em cada waypoint. Se a posição não mudar por 1,8 s, houver retorno ao tile anterior, perda de leitura, pausa ou janela de NPC no caminho, interrompe e solta o botão por RAII. O limite total continua em 40 s entre passos; o ajuste final admite no máximo seis cliques, mantendo a tolerância de um tile.

Validação: cinco testes de aproximação passaram, incluindo o limite entre trajeto segurado e ajuste final. Esta alteração ainda não foi publicada nem testada no jogo.

### Confirmação de mapa e destino

O move pelo M mantém duas tentativas: só prossegue com o nome do mapa esperado e coordenada dentro da região calibrada de chegada (16 tiles de tolerância para spawn variável). A confirmação final relê mapa e posição no mesmo quadro, até três vezes. A caminhada preserva o controle legado e suas tolerâncias (até dois tiles; recuperação com mobs até três), mas nunca substitui uma posição ilegível pelo destino desejado. A aproximação do NPC confirma o mapa do terreno e chegada até um tile antes da interação. Cada validação e tentativa é registrada com posição esperada e observada.

### ReDMooN: rota do spawn e segunda viagem automática

Em 13/11 às 21:30, a Fortuna automática falhou em Lorencia (125,121). O terreno real marca esse tile como 5 (SAFEZONE|NOMOVE), embora o servidor coloque o char ali. O A* recusava a origem. A aproximação agora permite apenas uma saída cardinal para um vizinho livre com rota válida; não atravessa tiles sem chão nem altera a malha original. O movimento segurado e as confirmações continuam iguais.

Falhas mantêm a espera de 15 min, agora registrada com o horário de liberação quando o Zen exige nova viagem. Um char em hold, com automação desligada ou recuperação desistida não bloqueia mais a Fortuna de outro char por prioridade de recuperação. Uma viagem concluída limpa o bloqueio anterior de Fortuna.
