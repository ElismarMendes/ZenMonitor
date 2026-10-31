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

def preprocessar_para_ocr(img_np, limiar_manual=None):
    if len(img_np.shape) == 3:
        b, g, r = cv2.split(img_np)
        gray = cv2.max(r, g)
    else:
        gray = img_np.copy()

    zoom = cv2.resize(gray, None, fx=3, fy=3, interpolation=cv2.INTER_LANCZOS4)
    zoom = cv2.normalize(zoom, None, 0, 255, cv2.NORM_MINMAX)

    if limiar_manual is not None:
        _, thresh = cv2.threshold(zoom, limiar_manual, 255, cv2.THRESH_BINARY_INV)
    else:
        _, thresh = cv2.threshold(zoom, 0, 255, cv2.THRESH_BINARY_INV + cv2.THRESH_OTSU)
        if np.mean(thresh) < 127:
            thresh = cv2.bitwise_not(thresh)

    kernel = np.ones((2, 2), np.uint8)
    thresh = cv2.dilate(thresh, kernel, iterations=1)

    com_borda = cv2.copyMakeBorder(
        thresh, 18, 18, 18, 18, cv2.BORDER_CONSTANT, value=255
    )
    return com_borda

def ler_texto_imagem(img_np) -> int:
    if img_np is None or img_np.size == 0 or not pytesseract:
        return None

    configs = [
        "--psm 7 --oem 3 -c tessedit_char_whitelist=0123456789",
        "--psm 6 --oem 3 -c tessedit_char_whitelist=0123456789"
    ]

    for limiar in (None, 150, 125, 175):
        try:
            img_tratada = preprocessar_para_ocr(img_np, limiar_manual=limiar)
            for cfg in configs:
                texto = pytesseract.image_to_string(img_tratada, config=cfg).strip()
                texto_limpo = "".join(filter(str.isdigit, texto))
                if texto_limpo:
                    val = int(texto_limpo)
                    if 0 < val <= 2000000000:
                        return val
        except Exception:
            continue

    return None