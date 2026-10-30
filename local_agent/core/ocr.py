import os
import cv2
import numpy as np

try:
    from config import TESSERACT_CMD
except ImportError:
    TESSERACT_CMD = r"C:\Program Files\Tesseract-OCR\tesseract.exe"

try:
    import pytesseract
    if os.path.exists(TESSERACT_CMD):
        pytesseract.pytesseract.tesseract_cmd = TESSERACT_CMD
except ImportError:
    pytesseract = None


def preprocessar_para_ocr(img_np):
    """
    Prepara o recorte do MU Online (texto claro em fundo escuro)
    deixando letras PRETAS em fundo BRANCO com borda de respiro para o Tesseract.
    """
    if len(img_np.shape) == 3:
        gray = cv2.cvtColor(img_np, cv2.COLOR_BGR2GRAY)
    else:
        gray = img_np.copy()

    # Amplia 3x para definir bem a fonte pixelada do MU Online
    zoom = cv2.resize(gray, None, fx=3, fy=3, interpolation=cv2.INTER_CUBIC)

    # Binarizacao Otsu (Texto claro vira 255, fundo escuro vira 0)
    _, thresh = cv2.threshold(zoom, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)

    # Se a maioria dos pixels for preta (< 127), significa que o fundo é preto e a letra é branca.
    # Invertemos para deixar FUNDO BRANCO e LETRA PRETA (padrão ideal do Tesseract)
    if np.mean(thresh) < 127:
        thresh = cv2.bitwise_not(thresh)

    # Adiciona uma borda branca de 15px ao redor (sem isso o Tesseract erra o primeiro e o ultimo digito)
    com_borda = cv2.copyMakeBorder(
        thresh, 15, 15, 15, 15, cv2.BORDER_CONSTANT, value=255
    )
    return com_borda


def ler_texto_imagem(img_np) -> int:
    if img_np is None or img_np.size == 0 or not pytesseract:
        return None

    try:
        # Whitelist apenas com digitos (ignora virgulas/pontos para nao confundir ',' com '1')
        config_ocr = "--psm 7 --oem 3 -c tessedit_char_whitelist=0123456789"

        # Tentativa 1: Imagem tratada com fundo branco e borda
        img_tratada = preprocessar_para_ocr(img_np)
        texto = pytesseract.image_to_string(img_tratada, config=config_ocr).strip()
        texto_limpo = "".join(filter(str.isdigit, texto))

        if texto_limpo:
            val = int(texto_limpo)
            if 0 <= val <= 2000000000:
                return val

        # Tentativa 2 (Fallback): Limiar fixo para fontes amarelas/douradas de Zen
        gray = cv2.cvtColor(img_np, cv2.COLOR_BGR2GRAY) if len(img_np.shape) == 3 else img_np
        zoom = cv2.resize(gray, None, fx=3, fy=3, interpolation=cv2.INTER_CUBIC)
        _, thresh_fixo = cv2.threshold(zoom, 110, 255, cv2.THRESH_BINARY_INV)
        com_borda2 = cv2.copyMakeBorder(thresh_fixo, 15, 15, 15, 15, cv2.BORDER_CONSTANT, value=255)

        texto2 = pytesseract.image_to_string(com_borda2, config=config_ocr).strip()
        texto_limpo2 = "".join(filter(str.isdigit, texto2))
        if texto_limpo2:
            val2 = int(texto_limpo2)
            if 0 <= val2 <= 2000000000:
                return val2

    except Exception:
        pass
    return None