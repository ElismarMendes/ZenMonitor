# 0008 — OCR só no recorte do campo do Zen

**O que aconteceu (07/11/2026):** com OCR na janela inteira, o agente achou "162812 Zen Obtained" no chat e
"Zen 162812" no chão, usou 162.812 como semente e varreu a memória do jogo REAL várias vezes atrás do número
errado. O campo certo (vermelho, "797,124,492") nem foi detectado: texto pequeno e colorido.

**Decisão:**
- OCR só no recorte `zen_ocr_region` (padrão [786, 609, 78, 14], coordenadas da janela inteira).
- O recorte é tratado antes do OCR: maior canal RGB (não perde o vermelho), contraste esticado, limiar
  preto/branco, ampliação 3x e margem branca. Num print real, o Tesseract leu `797,124,492` exato.
  O OCR do Windows ainda precisa ser confirmado no PC.
- Leitura do recorte é rígida: qualquer letra invalida (só dígitos, `,` `.` espaço e O→0).
- Sem região configurada: sem semente, sem varredura.
- `allowed_chars` no agent.toml: testes com o fake-client não tocam o jogo aberto.

## Atualização (mesmo dia): OCR substituído por leitura por modelo de pixels

O OCR do Windows falhou mesmo com o recorte perfeito: leu "313,637 *884" em vez de 913,697,884 e nada
em 872,359,120. A fonte do campo é de largura fixa (dígito 5 px, vírgula 3 px) e idêntica pixel a pixel em
todas as aparições (32 glifos em 3 prints, uma variante por caractere). Então o agente agora compara cada
célula com o modelo (`zen_core::glyphs`): tolerância de 1 pixel por célula, os modelos diferem entre si em
3+ pixels, formato de milhar obrigatório. Os 3 prints reais estão em `tests/fixtures/zen_field/` e são lidos
exatos no teste `prints_reais`. Sem dependência de OCR nem de pacote de idioma.

Se o servidor mudar a fonte ou a resolução, o teste com print novo vai falhar — é o sinal para refazer os modelos.
