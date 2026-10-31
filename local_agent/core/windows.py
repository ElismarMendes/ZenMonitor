# -*- coding: utf-8 -*-
import os
import sys
import ctypes
from ctypes import wintypes
import re
import subprocess
import numpy as np
import cv2
from PIL import ImageGrab

# Força o DPI Awareness real para ignorar o zoom do Windows em notebooks
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
shcore = ctypes.WinDLL("shcore", use_last_error=True)


def identificar_ambiente():
    try:
        cmd = 'powershell -Command "(Get-CimInstance Win32_ComputerSystem).Manufacturer"'
        res_manu = subprocess.check_output(cmd, shell=True, text=True, timeout=2).strip().lower()
        cmd_model = 'powershell -Command "(Get-CimInstance Win32_ComputerSystem).Model"'
        res_model = subprocess.check_output(cmd_model, shell=True, text=True, timeout=2).strip().lower()
        combinado = f"{res_manu} {res_model}"
        if any(k in combinado for k in ("vmware", "virtual", "qemu", "innotek", "oracle corporation")):
            return "VMWARE / VIRTUAL"
    except Exception:
        pass
    return "FISICO"


TIPO_AMBIENTE = identificar_ambiente()


def diagnosticar_dpi_e_resolucao(hwnd):
    """
    Inspeciona o fator de escala de DPI do monitor e as dimensoes reais da janela do jogo.
    """
    fator_escala = 1.0
    try:
        hmonitor = user32.MonitorFromWindow(hwnd, 2)  # MONITOR_DEFAULTTONEAREST
        dpi_x = ctypes.c_uint()
        dpi_y = ctypes.c_uint()
        shcore.GetDpiForMonitor(hmonitor, 0, ctypes.byref(dpi_x), ctypes.byref(dpi_y))  # MDT_EFFECTIVE_DPI
        fator_escala = dpi_x.value / 96.0
    except Exception:
        pass

    rect = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(rect))
    largura_cliente = rect.right - rect.left
    altura_cliente = rect.bottom - rect.top

    return fator_escala, largura_cliente, altura_cliente


def obter_geometria_cliente(hwnd):
    """
    Obtém a resolução interna real da janela do jogo aplicando o diagnóstico de DPI.
    Normaliza automaticamente resoluções esticadas por escala (ex: 1200x900 para 800x600).
    """
    if not win32gui or not hwnd or not win32gui.IsWindow(hwnd) or win32gui.IsIconic(hwnd):
        return None
    try:
        _, largura_cliente, altura_cliente = diagnosticar_dpi_e_resolucao(hwnd)
        if largura_cliente <= 0 or altura_cliente <= 0:
            return None

        # Normalização inteligente para resoluções comuns afetadas por escala de DPI em notebooks
        if abs(largura_cliente - 1200) < 15 and abs(altura_cliente - 900) < 15:
            largura_cliente, altura_cliente = 800, 600
        elif abs(largura_cliente - 1536) < 15 and abs(altura_cliente - 1152) < 15:
            largura_cliente, altura_cliente = 1024, 768

        client_x, client_y = win32gui.ClientToScreen(hwnd, (0, 0))
        return client_x, client_y, largura_cliente, altura_cliente
    except Exception:
        return None


def converter_pct_para_box_cliente(hwnd, pct_box):
    geom = obter_geometria_cliente(hwnd)
    if not geom or not pct_box or len(pct_box) != 4:
        return None, None

    client_x, client_y, larg, alt = geom
    px1, py1, px2, py2 = pct_box

    rel_x1 = max(0, int(px1 * larg))
    rel_y1 = max(0, int(py1 * alt))
    rel_x2 = min(larg, int(px2 * larg))
    rel_y2 = min(alt, int(py2 * alt))

    return (rel_x1, rel_y1, rel_x2, rel_y2), geom


def capturar_tela_janela_fundo(hwnd, rel_box, forcar_tela_direta=False):
    if not win32gui or not hwnd or not win32gui.IsWindow(hwnd) or not rel_box:
        return None

    if win32gui.IsIconic(hwnd):
        return None

    geom = obter_geometria_cliente(hwnd)
    if not geom:
        return None

    client_x, client_y, _, _ = geom
    rel_x1, rel_y1, rel_x2, rel_y2 = rel_box
    largura = rel_x2 - rel_x1
    altura = rel_y2 - rel_y1

    if largura <= 0 or altura <= 0:
        return None

    try:
        abs_x1 = client_x + rel_x1
        abs_y1 = client_y + rel_y1
        abs_x2 = client_x + rel_x2
        abs_y2 = client_y + rel_y2

        pil_img = ImageGrab.grab(bbox=(abs_x1, abs_y1, abs_x2, abs_y2), all_screens=True)
        img_np = np.array(pil_img)
        recorte = cv2.cvtColor(img_np, cv2.COLOR_RGB2BGR)
        if recorte.size > 0 and np.mean(recorte) > 2.0:
            return recorte
    except Exception:
        pass

    return None


def extrair_nome_inteligente(titulo, pid):
    padroes_extras = [
        r"N[ao]me\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"Char(?:acter)?\s*[:=-]\s*([^\s\|\-\]\)]+)",
        r"\[\s*([A-Za-z0-9_\-]+)\s*\]",
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
        if user32.IsWindowVisible(hwnd) or (win32gui and win32gui.IsIconic(hwnd)):
            length = user32.GetWindowTextLengthW(hwnd)
            if length > 0:
                buff = ctypes.create_unicode_buffer(length + 1)
                user32.GetWindowTextW(hwnd, buff, length + 1)
                titulo = buff.value.strip()

                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value and any(k in titulo.lower() for k in ("theclassic mu", "name:", "nome:", "level", "mainmu")):
                    atual = janelas_por_pid.get(pid.value)
                    tem_nome_novo = "name:" in titulo.lower() or "nome:" in titulo.lower()
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
            "resolucao": res_str,
        }
    return clientes