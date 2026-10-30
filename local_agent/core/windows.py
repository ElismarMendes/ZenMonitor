import os
import sys
import ctypes
from ctypes import wintypes
import re
import numpy as np
import cv2
from PIL import ImageGrab

try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

PASTA_CORE = os.path.dirname(os.path.abspath(__file__))
PASTA_AGENT = os.path.dirname(PASTA_CORE)
if PASTA_AGENT not in sys.path:
    sys.path.insert(0, PASTA_AGENT)

try:
    from config import PADRAO_TITULO
except ImportError:
    PADRAO_TITULO = None

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
        if win32gui.IsIconic(hwnd):
            return None
        _, _, largura, altura = win32gui.GetClientRect(hwnd)
        if largura <= 0 or altura <= 0:
            return None
        client_x, client_y = win32gui.ClientToScreen(hwnd, (0, 0))
        return client_x, client_y, largura, altura
    except Exception:
        return None


def capturar_tela_janela_fundo(hwnd, rel_box, forcar_tela_direta=False):
    if not win32gui or not hwnd or not win32gui.IsWindow(hwnd) or not rel_box:
        return None

    geom = obter_geometria_cliente(hwnd)
    if not geom:
        return None

    client_x, client_y, larg_total, alt_total = geom
    rel_x1, rel_y1, rel_x2, rel_y2 = rel_box
    largura = rel_x2 - rel_x1
    altura = rel_y2 - rel_y1

    if largura <= 0 or altura <= 0:
        return None

    if not forcar_tela_direta:
        hwndDC = mfcDC = saveDC = saveBitMap = None
        try:
            hwndDC = win32gui.GetWindowDC(hwnd)
            mfcDC = win32ui.CreateDCFromHandle(hwndDC)
            saveDC = mfcDC.CreateCompatibleDC()

            saveBitMap = win32ui.CreateBitmap()
            saveBitMap.CreateCompatibleBitmap(mfcDC, larg_total, alt_total)
            saveDC.SelectObject(saveBitMap)

            res_pw = user32.PrintWindow(hwnd, saveDC.GetSafeHdc(), 3)
            if res_pw == 0:
                win32gui.ReleaseDC(hwnd, hwndDC)
                hwndDC = win32gui.GetDC(hwnd)
                mfcDC_client = win32ui.CreateDCFromHandle(hwndDC)
                saveDC.BitBlt((0, 0), (larg_total, alt_total), mfcDC_client, (0, 0), win32con.SRCCOPY)
                mfcDC_client.DeleteDC()

            bmpinfo = saveBitMap.GetInfo()
            bmpstr = saveBitMap.GetBitmapBits(True)

            img = np.frombuffer(bmpstr, dtype=np.uint8)
            img = np.reshape(img, (bmpinfo["bmHeight"], bmpinfo["bmWidth"], 4))
            img_bgr = cv2.cvtColor(img, cv2.COLOR_BGRA2BGR)

            recorte = img_bgr[rel_y1:rel_y2, rel_x1:rel_x2]
            if recorte.size > 0 and np.mean(recorte) > 2.0:
                return recorte
        except Exception:
            pass
        finally:
            if saveBitMap:
                try: win32gui.DeleteObject(saveBitMap.GetHandle())
                except Exception: pass
            if saveDC:
                try: saveDC.DeleteDC()
                except Exception: pass
            if mfcDC:
                try: mfcDC.DeleteDC()
                except Exception: pass
            if hwndDC:
                try: win32gui.ReleaseDC(hwnd, hwndDC)
                except Exception: pass

    try:
        abs_x1 = client_x + rel_x1
        abs_y1 = client_y + rel_y1
        abs_x2 = client_x + rel_x2
        abs_y2 = client_y + rel_y2
        pil_img = ImageGrab.grab(bbox=(abs_x1, abs_y1, abs_x2, abs_y2), all_screens=True)
        img_np = np.array(pil_img)
        return cv2.cvtColor(img_np, cv2.COLOR_RGB2BGR)
    except Exception:
        return None


def extrair_nome_inteligente(titulo: str, pid: int) -> str:
    padroes_extras = [
        r"N[ao]me\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"Char(?:acter)?\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"\[\s*([A-Za-z0-9_\-]+)\s*\]"
    ]
    for p in padroes_extras:
        m2 = re.search(p, titulo, re.IGNORECASE)
        if m2 and m2.group(1).lower() not in ("theclassic", "mainmu", "mu"):
            return m2.group(1).strip()

    if PADRAO_TITULO:
        m = PADRAO_TITULO.search(titulo)
        if m:
            return m.group(1).strip()

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
                if pid.value and any(k in titulo.lower() for k in ("theclassic mu", "name:", "nome:", "level", "mainmu")):
                    atual = janelas_por_pid.get(pid.value)
                    tem_nome_novo = ("name:" in titulo.lower() or "nome:" in titulo.lower())
                    tem_nome_atual = atual and ("name:" in atual["titulo"].lower() or "nome:" in atual["titulo"].lower())
                    if not atual or (tem_nome_novo and not tem_nome_atual):
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