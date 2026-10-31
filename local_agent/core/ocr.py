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


def preprocessar_para_ocr(img_np, usar_canal_rg=False):
    if len(img_np.shape) == 3:
        if usar_canal_rg:
            b, g, r = cv2.split(img_np)
            gray = cv2.max(r, g)
        else:
            gray = cv2.cvtColor(img_np, cv2.COLOR_BGR2GRAY)
    else:
        gray = img_np.copy()

    gray = cv2.resize(gray, (0, 0), fx=2.0, fy=2.0, interpolation=cv2.INTER_CUBIC)
    _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)
    return thresh


def ler_texto_imagem(img_np):
    if img_np is None or img_np.size == 0 or not pytesseract:
        return None

    config_ocr = "--psm 7 -c tessedit_char_whitelist=0123456789.,"

    for usar_rg in (False, True):
        try:
            thresh = preprocessar_para_ocr(img_np, usar_canal_rg=usar_rg)
            texto = pytesseract.image_to_string(thresh, config=config_ocr).strip()
            texto_limpo = "".join(filter(str.isdigit, texto))

            if texto_limpo:
                val = int(texto_limpo)
                if 0 < val <= 2000000000:
                    return val
        except Exception:
            continue

    return None