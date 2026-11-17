# 0012 — Recuperação automática até o spot (Partes 1-4)

Data: 08/11/2026. Pedido do dono, com testes autorizados no x9901 e no ReDMooN.

## O que foi medido
- Posição: lida na TELA ("Kanturu (X , Y)", mesma fonte do Zen; 44 recortes reais + bug do
  "4" colado corrigido). A memória teve candidatos, mas o primeiro endereço (0x1ae434) se
  mostrou rascunho do jogo (lixo durante o movimento). A tela é a fonte oficial.
- Mapa: palavra "Kanturu" seguida de "(" (Kanturu 2 não passa). Terreno do Kanturu 1 =
  Data\World38\EncTerrain38.att, decodificado (conferido com Lorencia e com 33 tiles reais).
- Helper: ícone pause (ligado) / play (desligado), 5 prints reais. Home só se precisar.
- DC: o cliente reconecta sozinho e o char aparece em Lorencia com o helper desligado.
- M -> botão Kanturu: teleporte em 3 s para a zona segura (~20, 218).
- Andar: segurar o botão guiando pela rota (dica do dono) e terminar com cliques. Tile exato
  por até 10 s, depois aceita 1 tile. Recuperação real dos 2 chars: 36 s e 50 s, tile exato.

## Regras
- Só com `automation_enabled` e `recovery_enabled`, spot salvo, PC sem uso há
  `automation_idle_s`, um char por vez, conexão ok.
- Gatilhos (duram 30 s): fora do Kanturu; ou no Kanturu com o helper desligado.
  Helper ligado = farmando: nunca mexe. Tela ilegível: nunca mexe.
- Máximo 3 tentativas por hora por char; depois de falha espera 2 min.
- Falhou no meio do caminho fora da zona segura: liga o helper onde estiver.
- Atenção: se você desligar o helper de propósito e sair do PC, o agente religa (depois de
  30 s + 60 s sem uso). Para parar de vez: botão AUTOMAÇÃO no painel.

## Limites conhecidos
- Só Kanturu 1. Outro mapa de farm precisa do terreno e da palavra do nome.
- Coordenadas do botão do M e do centro dos cliques medidas no cliente 1030x797.

## Catálogo de spots e atalhos (08/11, tarde)
- Ctrl+S no jogo: o tile atual entra no fim do catálogo (Kanturu 1). Tiles a até 2 de distância
  contam como o mesmo spot. A ordem do catálogo é a ordem de busca de spot livre (pedido do
  dono: do início do mapa até o último spot).
- Ctrl+D no jogo: o char passa a voltar sempre para o tile atual (entra no catálogo se faltar).
- Os atalhos usam gancho de teclado que só observa: valem só com a janela do jogo na frente,
  não bloqueiam a tecla e ignoram teclas injetadas (as do próprio agente).
- Painel, aba SPOTS: mini-mapa do Kanturu 1 (terreno do cliente embutido no central), spots
  numerados, cada char no mapa com linha até o spot dele, trocar spot e excluir spot.
- "Spot ocupado" (outros jogadores perto, pela memória): em pesquisa; não ligado ainda.

## Adendo (08/11 18:04) — um spot, um dono; trava
- Um spot do catálogo tem no máximo um dono (tiles a até 2 contam como o mesmo spot). O agente
  recusa (painel, Ctrl+D, Salvar spot) e o central recusa entre PCs diferentes.
- "Travar" um char: o spot dele não troca nem pode ser excluído até destravar (`locked_spots`).
- Painel: o redesenho espera 2,5 s depois de qualquer clique/tecla e enquanto um campo ou lista
  está em foco; escolhas em listas sobrevivem ao redesenho (antes o clique sumia e a lista fechava).
