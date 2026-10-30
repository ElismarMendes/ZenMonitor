import cv2
import numpy as np
from PIL import ImageGrab
from config import TESSERACT_CMD

try:
    import pytesseract
    pytesseract.pytesseract.tesseract_cmd = TESSERACT_CMD
except ImportError:
    pytesseract = None

def ler_texto_imagem(img_np) -> int:
    if img_np is None or img_np.size == 0 or not pytesseract:
        return None
    try:
        gray = cv2.cvtColor(img_np, cv2.COLOR_BGR2GRAY) if len(img_np.shape) == 3 else img_np
        gray = cv2.resize(gray, (0, 0), fx=2, fy=2, interpolation=cv2.INTER_CUBIC)
        _, thresh = cv2.threshold(gray, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)

        config_ocr = '--psm 7 -c tessedit_char_whitelist=0123456789.,'
        texto = pytesseract.image_to_string(thresh, config=config_ocr).strip()
        texto_limpo = ''.join(filter(str.isdigit, texto))

        if texto_limpo:
            val = int(texto_limpo)
            if 0 < val <= 2000000000:
                return val
    except Exception:
        pass
    return None