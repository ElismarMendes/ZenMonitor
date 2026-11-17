# Validação de spots no PC1 e implantação +99 — 14/11/2026

A revisão foi compilada e publicada no canal exclusivo PC1 como +98. A instalação foi confirmada pelo agente antes de enviar ações. O usuário autorizou testes pela ponte e com o personagem x9901.

Teste real, horários de São Paulo:

- 11:56: Move de Kanturu para Lorencia, solicitado com GoTo sem alterar o spot. Teleporte confirmado em (119,128); caminhada até (123,129) confirmada no mapa correto. Zona segura, Helper não ligado. Total registrado: 8,76 s.
- 11:56–11:58: ReturnToSpot para o principal Kanturu (90,66). Teleporte e mapa confirmados; rota pelo terreno até (91,66), distância de 1 tile do destino. Confirmação final usou margem 2, Helper ligado e spot assumido. Total registrado: 131,63 s, incluindo a rota completa desde a entrada do mapa; não é medida exclusiva da ativação do Helper.
- O principal (world 38, x 90, y 66) foi comparado ao estado inicial e preservado. Nenhum cadastro de spot ou opção de automação foi alterado pelos comandos de teste.

A cobertura de ocupação por terceiros, reservas, ausência de leitura, liberação com falha de rede e fallback entre mapas permaneceu baseada nos testes automatizados já aprovados; não foi provocada ocupação/PK ou migração forçada no jogo.

Após o sucesso, o mesmo binário de +98 foi promovido para o canal estável como +99. SHA256 conferido: `6A56B065FA69C2E4C12CD7EC42CA07BB63E7F23E6464C23C399F0599785C43DA`.

Instalação +99 e conexão confirmadas nos cinco computadores: PC1, DESKTOP-E1GLBEF, DESKTOP-RAG9LEL, ELISMAR e ELISMAR-NOTE. Todos reportaram last_error nulo na conferência final. Somente PC1 permanece em test_machines.

Evidências: logs do agente em target/release/logs/agent.2026-11-14.log; jobs codex-validar-spots-build e codex-promover-validado-spots em ponte/saida; snapshots locais target/validacao-spots-inicial.json e target/validacao-spots-final.json.
