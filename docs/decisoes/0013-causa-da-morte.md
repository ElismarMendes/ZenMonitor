# 0013 — Quem matou: monstro volta, PK espera

Pedido (08/11, 13:20): "se foi monstro pode retornar; em caso de PK só se o spot estiver vazio".

## Como o agente sabe quem matou
O servidor manda para todos a linha `[PK] : <quem matou> killed <vítima>` (monstros também:
`Death Rider killed sephiroth`). Quando a vida trava em 0 e a tela confirma a morte, o agente varre
a memória do cliente DA VÍTIMA atrás de `... killed <nome>` (1,5 s depois e, se nada, mais 3 s
depois). Cada varredura leva ~10 s (medido: 2409 linhas em 9,9 s) e roda numa thread; o ciclo
de 5 s continua. Isto é varredura completa, mas só na hora da morte — não no ciclo normal
(regra da decisão 0005 mantida; o `chat_enabled` continua desligado).

Linha velha não conta: guardamos (endereço, texto) de cada morte; só vale linha nova.
Quem matou com espaço no nome (ou na lista de monstros de uma palavra) = monstro.

## Regra
- Monstro: volta ao spot como antes.
- 3 mortes para monstros em 30 min: para de voltar (spot pesado demais / helper sem poção).
- Player (PK): NÃO volta sozinho. "Spot vazio" ainda não dá para saber (opção B, lista de
  players na memória, não feita). Por isso hoje PK = espera você.
- Nenhuma linha achada: causa desconhecida = NÃO volta (lado seguro).
- Libera: botão Voltar no painel, ou o char voltar a farmar perto do spot (você o levou).

## Ponto aberto
Em 08/11 às 13:24 não havia nenhuma linha `killed ReDMooN` na memória dos dois clientes, ~20 min
depois das mortes das 12:59/13:00. Ou as linhas saem da memória em minutos, ou a morte do próprio
char não vira linha `[PK]`. A próxima morte real responde; se for o segundo caso, toda morte vira
"causa desconhecida" e o char espera — seguro, mas sem volta automática.

## Revisão (08/11, 14:37) — "spot meu"
Pedido: "se já estou num spot há um tempo, o spot é meu; levei PK, volto o mais rápido possível.
Só interrompo alguém se eu matar; aqui eu sou a vítima".

Medido às 13:52: a morte do PRÓPRIO char não gera linha `[PK] ... killed <char>` na memória dele
(2 varreduras, 0 linhas). Então "quem matou" quase nunca é conhecido e não pode ser a regra principal.

Regra nova (vale para PK, monstro ou causa desconhecida):
- Farmava no spot (Kanturu, helper ligado, a até 10 tiles) havia **15 min ou mais** quando morreu
  = o spot é dele: volta já (espera 5 s em vez de 30; ainda exige o PC sem uso há 60 s).
- Menos de 15 min = provavelmente entrou no spot de outro, OU voltou e morreu de novo (o
  assassino está acampando): NÃO volta; espera você (Voltar no painel).
  Isso é também a trava contra ciclo de morte: depois de voltar, o relógio dos 15 min recomeça.
- Monstro confirmado pelo chat continua voltando; 3 mortes por monstro em 30 min param.
- O relógio dos 15 min zera se o agente reiniciar (lado seguro).

## Revisão (08/11, 16:23)
- "1 minuto que estou lá já é meu": `OWN_SPOT_S` = 60 s.
- Botão por char "Volta após morte: SIM/NÃO" (card e aba SPOTS); gravado em
  `agent.toml` -> `no_return_on_death`. NÃO = toda morte espera você. DC continua voltando.
- Volta rápida: 5 s depois de ver o char fora do Kanturu. O que ainda segura: o PC tem de estar
  sem uso há `automation_idle_s` (60 s) — de propósito, para não brigar com você pelo mouse.
- Trava contra ciclo: voltou e morreu antes de 1 min no spot = não volta. Com 1 min, um
  acampador pode matar várias vezes; o limite de 3 recuperações por hora segura o resto.

## Revisão (08/11, 16:29) — regras no painel (aba Máquinas, por PC)
- "N voltas em M minutos" (todas as recuperações: morte, DC, helper desligado, longe do spot).
  Começa em 3 em 10 min (era 3 por hora). Limites: 1–20 voltas, 1–240 min.
- Espera depois de morrer no spot: sorteada entre mínimo e máximo. Começa em 1 a 10 min, como
  pedido. Limite: até 30 min. Gravado no agent.toml (return_*).
