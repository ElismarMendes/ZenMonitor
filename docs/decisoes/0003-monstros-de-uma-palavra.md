# 0003 — Separar monstro de player

**Regra:** quem matou com espaço no nome = monstro (player não tem espaço). Sem espaço, consulta a lista
`one_word_monsters` (Spider, Lich, Balrog, Hydra, Yeti, Gorgon, Medusa, Kundun, Valkyrie, Worm, Bahamut).

**Não verificado:** essa lista vem do MU em geral, não do The Classic. Se um player se chamar "Balrog",
ele vira monstro no ranking. Ajustar com as 48 linhas reais de PK quando `dados-reais/` chegar.
