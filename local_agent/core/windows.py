import os
import sys
import ctypes
from ctypes import wintypes
import re
import numpy as np
import cv2
from PIL import ImageGrab

# Garante que a pasta local_agent e a raiz estejam no path mesmo rodando o arquivo direto
PASTA_CORE = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_CORE)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

try:
    from config import PADRAO_TITULO
except ImportError:
    PADRAO_TITULO = re.compile(r"Name:\s*([^\s\|\-]+)", re.IGNORECASE)

try:
    import win32gui
    import win32ui
    import win32con
except ImportError:
    win32gui = None

user32 = ctypes.WinDLL("user32", use_last_error=True)

def obter_geometria_cliente(hwnd):
    if not win32gui or not hwnd or not win32gui.IsWindow(hwnd):
        return None
    try:
        _, _, largura, altura = win32gui.GetClientRect(hwnd)
        if largura <= 0 or altura <= 0:
            return None
        client_x, client_y = win32gui.ClientToScreen(hwnd, (0, 0))
        return client_x, client_y, largura, altura
    except Exception:
        return None

def capturar_tela_janela_fundo(hwnd, rel_box):
    if not win32gui or not hwnd or not win32gui.IsWindow(hwnd) or not rel_box:
        return None

    rel_x1, rel_y1, rel_x2, rel_y2 = rel_box
    largura = rel_x2 - rel_x1
    altura = rel_y2 - rel_y1

    if largura <= 0 or altura <= 0:
        return None

    hwndDC = mfcDC = saveDC = saveBitMap = None
    try:
        hwndDC = win32gui.GetDC(hwnd)
        mfcDC = win32ui.CreateDCFromHandle(hwndDC)
        saveDC = mfcDC.CreateCompatibleDC()

        saveBitMap = win32ui.CreateBitmap()
        saveBitMap.CreateCompatibleBitmap(mfcDC, largura, altura)
        saveDC.SelectObject(saveBitMap)

        saveDC.BitBlt((0, 0), (largura, altura), mfcDC, (rel_x1, rel_y1), win32con.SRCCOPY)

        bmpinfo = saveBitMap.GetInfo()
        bmpstr = saveBitMap.GetBitmapBits(True)

        img = np.frombuffer(bmpstr, dtype=np.uint8)
        img = np.reshape(img, (bmpinfo['bmHeight'], bmpinfo['bmWidth'], 4))
        img_bgr = cv2.cvtColor(img, cv2.COLOR_BGRA2BGR)

        if img_bgr.size > 0 and np.any(img_bgr):
            return img_bgr
    except Exception:
        pass
    finally:
        if saveBitMap:
            try: win32gui.DeleteObject(saveBitMap.GetHandle())
            except Exception: pass
        if saveDC: saveDC.DeleteDC()
        if mfcDC: mfcDC.DeleteDC()
        if hwndDC: win32gui.ReleaseDC(hwnd, hwndDC)

    return None

def extrair_nome_inteligente(titulo: str, pid: int) -> str:
    if PADRAO_TITULO:
        m = PADRAO_TITULO.search(titulo)
        if m:
            return m.group(1).strip()

    padroes_extras = [
        r"Name\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"Character\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"\[\s*([A-Za-z0-9_\-]+)\s*\]"
    ]
    for p in padroes_extras:
        m2 = re.search(p, titulo, re.IGNORECASE)
        if m2 and m2.group(1).lower() not in ("theclassic", "mainmu", "mu"):
            return m2.group(1).strip()

    return f"PID:{pid}"

def obter_clientes_mainmu():
    janelas_por_pid = {}
    WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def callback(hwnd, lParam):
        if user32.IsWindowVisible(hwnd):
            length = user32.GetWindowTextLengthW(hwnd)
            if length > 0:
                buff = ctypes.create_unicode_buffer(length + 1)
                user32.GetWindowTextW(hwnd, buff, length + 1)
                titulo = buff.value.strip()

                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value and any(k.lower() in titulo.lower() for k in ("theclassic mu", "name:", "level", "mainmu")):
                    atual = janelas_por_pid.get(pid.value)
                    if not atual or ("name:" in titulo.lower() and "name:" not in atual["titulo"].lower()):
                        janelas_por_pid[pid.value] = {"hwnd": hwnd, "titulo": titulo}
        return True

    user32.EnumWindows(WNDENUMPROC(callback), 0)

    clientes = {}
    for pid, info in janelas_por_pid.items():
        name = extrair_nome_inteligente(info["titulo"], pid)
        geom = obter_geometria_cliente(info["hwnd"])
        res_str = f"{geom[2]}x{geom[3]}" if geom else "Minimizado"

        clientes[pid] = {
            "name": name,
            "hwnd": info["hwnd"],
            "titulo": info["titulo"],
            "resolucao": res_str
        }
    return clientes

if __name__ == "__main__":
    print("=== TESTE DE DETECCAO DE JANELAS MU ===")
    detectados = obter_clientes_mainmu()
    if not detectados:
        print("Nenhuma janela do MU encontrada aberta.")
    for pid, dados in detectados.items():
        print(f"PID: {pid} | Char: {dados['name']} | Res: {dados['resolucao']} | Titulo Real: '{dados['titulo']}'")