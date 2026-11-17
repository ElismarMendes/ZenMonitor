# 0014 — Depósito automático no baú do Kanturu

Pedido (08/11, 16:45): char no máximo de Zen -> move Kanturu -> baú (Safety Guardian, 15 abas)
-> deposita deixando 40 mi -> volta ao spot.

## Medido (gravação de um depósito à mão, 08/11 16:48)
- Move para Kanturu custou **20.000.000** (o pedido dizia 21 mil). Reserva combinada: **40 mi**
  (2 moves de emergência). Cada aba guarda até **2 bi** (confirmado pelo dono).
- Depósito sem taxa. Janela do baú: "Storage N/15", seta de próxima aba, campo Zen da aba, botão
  Deposit -> caixa "Enter the amount..." -> OK/CANCEL. V fecha baú + inventário.
- O Zen da aba e do inventário usam a mesma fonte do campo do Zen: lidos com `read_number`
  (conferido nas imagens gravadas: 150.618.098, 0, 1.900.000.000, 1.980.000.000).

## Fluxo (`agent::win::nav::vault`)
helper off -> (M Kanturu se longe) -> anda até (33,217) (aceita 1 tile ao lado e corrige o
clique no NPC pelo modelo pixel->tile) -> clica no NPC -> espera o campo Zen do baú -> lê o Zen
do char -> aba atual cabe tudo? senão próxima seta, até 15 -> Deposit -> digita só dígitos ->
OK -> **confere**: aba subiu exatamente o valor e o char caiu o valor -> V -> volta ao spot.
Qualquer leitura que não bate: CANCEL, fecha, avisa no painel e não tenta de novo por 1 h.

## Teste real (08/11 17:04, x9901)
Leu as 15 abas (abas 1-6 quase cheias, 7-15 com pouco ou nada). Depositou 1.940.000.000 e ficou
com 40.000.000, conferido na tela; voltou ao spot em 30 s. Total ~55 s.

## Achado
O baú reabre na ÚLTIMA aba vista, não na 1. O número da aba no log é a contagem desde a
abertura, não o "N/15" da tela (ainda não lido). A escolha da aba não depende disso: o agente
lê o Zen da aba que está na tela antes de depositar.
