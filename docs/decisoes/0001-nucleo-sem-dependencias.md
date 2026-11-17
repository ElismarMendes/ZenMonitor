# 0001 — Regras num crate sem dependências

**Decisão:** toda regra (parsers, Zen/h, alertas, dedupe, trava de RAM) mora em `zen-core`, sem I/O,
sem relógio e sem dependência obrigatória. O `serde` entra só como feature.

**Por quê:** é o que dá para testar em segundos em qualquer máquina, inclusive no CI Linux, e garante que
agente e central decidem igual. Tudo que toca Windows fica atrás da trait `GameAccess`, com versão falsa
nos testes.

**Custo:** os testes por propriedade usam um gerador próprio em vez do `proptest`. Migrar quando quiser.
