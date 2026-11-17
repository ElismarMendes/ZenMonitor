# 0007 — Dedupe de PK

A mesma dupla (quem matou, vítima) conta 1 vez por 30 min, venha de qual PC vier. A regra de "90 s entre
PCs" do prompt mestre já está coberta por essa (30 min > 90 s), então não existe à parte.
Custo: duas mortes reais idênticas em 30 min contam como uma. Sem horário na linha, não há como distinguir.

O central também usa o próprio relógio. Relatórios guardados no buffer do agente (central fora do ar) são
reposicionados no tempo pela menor diferença medida entre relógio do PC e do central, nunca no futuro.
