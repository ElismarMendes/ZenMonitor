@echo off
chcp 65001 >nul
echo Criando ferramenta de calibragem visual do OCR...

:: 1. Adiciona os arquivos de calibragem local no .gitignore para nao dar conflito entre PCs
findstr /C:"*.json" .gitignore >nul 2>&1 || echo *.json>> .gitignore

:: 2. Cria o script Python de calibragem visual (calibrar_ocr.py)
(
echo import os
echo import sys
echo import json
echo import tkinter as tk
echo from tkinter import ttk, messagebox
echo from PIL import ImageGrab, ImageTk
echo import win32gui
echo.
echo sys.path.insert^(0, os.path.join^(os.path.dirname^(__file__^), "local_agent"^)^)
echo try:
echo     from config import CONFIG_FILE
echo except ImportError:
echo     CONFIG_FILE = "ocr_config.json"
echo.
echo class CalibradorOCR:
echo     def __init__^(self, root^):
echo         self.root = root
echo         self.root.title^("Calibrador de OCR - ZenMonitor"^)
echo         self.root.geometry^("480x260"^)
echo         self.root.resizable^(False, False^)
echo.
echo         self.janelas = self.listar_janelas_mu^(^)
echo.
echo         frame = ttk.Frame^(root, padding=15^)
echo         frame.pack^(fill="both", expand=True^)
echo.
echo         ttk.Label^(frame, text="1. Selecione a janela do MU Online ^(deixe o inventario 'V' aberto^):", font=^("Segoe UI", 9, "bold"^)^).pack^(anchor="w", pady=^(0, 5^)^)
echo         self.combo = ttk.Combobox^(frame, values=[f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas], width=55, state="readonly"^)
echo         self.combo.pack^(fill="x", pady=^(0, 10^)^)
echo         if self.janelas:
echo             self.combo.current^(0^)
echo.
echo         btn_frame = ttk.Frame^(frame^)
echo         btn_frame.pack^(fill="x", pady=5^)
echo         ttk.Button^(btn_frame, text="🔄 Atualizar Lista", command=self.atualizar_lista^).pack^(side="left", padx=^(0, 10^)^)
echo         ttk.Button^(btn_frame, text="🎯 Capturar e Selecionar Área do Zen", command=self.iniciar_selecao^).pack^(side="left"^)
echo.
echo         self.lbl_status = ttk.Label^(frame, text="Coordenadas atuais: Carregando...", foreground="blue", font=^("Consolas", 9^)^)
echo         self.lbl_status.pack^(anchor="w", pady=^(15, 0^)^)
echo         self.mostrar_config_atual^(^)
echo.
echo     def listar_janelas_mu^(self^):
echo         res = []
echo         def callback^(hwnd, _^):
echo             if win32gui.IsWindowVisible^(hwnd^):
echo                 titulo = win32gui.GetWindowText^(hwnd^)
echo                 if titulo and ^("mu" in titulo.lower^(^) or "main" in titulo.lower^(^)^):
echo                     res.append^(^(hwnd, titulo^)^)
echo         win32gui.EnumWindows^(callback, None^)
echo         if not res:
echo             def callback_all^(hwnd, _^):
echo                 if win32gui.IsWindowVisible^(hwnd^) and win32gui.GetWindowText^(hwnd^):
echo                     res.append^(^(hwnd, win32gui.GetWindowText^(hwnd^)^)^)
echo             win32gui.EnumWindows^(callback_all, None^)
echo         return res
echo.
echo     def atualizar_lista^(self^):
echo         self.janelas = self.listar_janelas_mu^(^)
echo         self.combo["values"] = [f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas]
echo         if self.janelas:
echo             self.combo.current^(0^)
echo.
echo     def mostrar_config_atual^(self^):
echo         if os.path.exists^(CONFIG_FILE^):
echo             try:
echo                 with open^(CONFIG_FILE, "r", encoding="utf-8"^) as f:
echo                     dados = json.load^(f^)
echo                 self.lbl_status.config^(text=f"Salvo em {CONFIG_FILE}:\n{json.dumps(dados)}"^)
echo                 return
echo             except Exception:
echo                 pass
echo         self.lbl_status.config^(text=f"Nenhum arquivo {CONFIG_FILE} encontrado ainda."^)
echo.
echo     def iniciar_selecao^(self^):
echo         selecao = self.combo.get^(^)
echo         if not selecao:
echo             messagebox.showwarning^("Aviso", "Selecione uma janela primeiro!"^)
echo             return
echo.
echo         hwnd = int^(selecao.split^(" - "^)[0]^)
echo         try:
echo             if win32gui.IsIconic^(hwnd^):
echo                 win32gui.ShowWindow^(hwnd, 9^)
echo             win32gui.SetForegroundWindow^(hwnd^)
echo         except Exception:
echo             pass
echo.
echo         self.root.withdraw^(^)
echo         self.root.after^(400, lambda: self.abrir_tela_recorte^(hwnd^)^)
echo.
echo     def abrir_tela_recorte^(self, hwnd^):
echo         rect = win32gui.GetWindowRect^(hwnd^)
echo         x1, y1, x2, y2 = rect
echo         largura_jan = x2 - x1
echo         altura_jan = y2 - y1
echo         img = ImageGrab.grab^(bbox=^(x1, y1, x2, y2^)^)
echo.
echo         top = tk.Toplevel^(self.root^)
echo         top.title^("Arraste o mouse sobre os numeros do ZEN e solte"^)
echo         top.attributes^("-topmost", True^)
echo.
echo         canvas = tk.Canvas^(top, width=img.width, height=img.height, cursor="cross"^)
echo         canvas.pack^(^)
echo.
echo         tk_img = ImageTk.PhotoImage^(img^)
echo         canvas.img = tk_img
echo         canvas.create_image^(0, 0, anchor="nw", image=tk_img^)
echo.
echo         coords = {"start_x": 0, "start_y": 0, "rect": None}
echo.
echo         def on_press^(event^):
echo             coords["start_x"] = event.x
echo             coords["start_y"] = event.y
echo             if coords["rect"]:
echo                 canvas.delete^(coords["rect"]^)
echo             coords["rect"] = canvas.create_rectangle^(event.x, event.y, event.x, event.y, outline="#00ff00", width=2^)
echo.
echo         def on_drag^(event^):
echo             canvas.coords^(coords["rect"], coords["start_x"], coords["start_y"], event.x, event.y^)
echo.
echo         def on_release^(event^):
echo             rx1 = min^(coords["start_x"], event.x^)
echo             ry1 = min^(coords["start_y"], event.y^)
echo             rx2 = max^(coords["start_x"], event.x^)
echo             ry2 = max^(coords["start_y"], event.y^)
echo             w = rx2 - rx1
echo             h = ry2 - ry1
echo.
echo             if w ^< 5 or h ^< 5:
echo                 messagebox.showwarning^("Muito pequeno", "Selecione um retangulo maior sobre o Zen."^)
echo                 return
echo.
echo             resolucao_str = f"{largura_jan}x{altura_jan}"
echo             self.salvar_coordenadas^(rx1, ry1, rx2, ry2, w, h, resolucao_str^)
echo             top.destroy^(^)
echo             self.root.deiconify^(^)
echo.
echo         canvas.bind^("<ButtonPress-1>", on_press^)
echo         canvas.bind^("<B1-Motion>", on_drag^)
echo         canvas.bind^("<ButtonRelease-1>", on_release^)
echo         top.protocol^("WM_DELETE_WINDOW", lambda: ^(top.destroy^(^), self.root.deiconify^(^)^)^)
echo.
echo     def salvar_coordenadas^(self, x1, y1, x2, y2, w, h, res_str^):
echo         dados = {}
echo         if os.path.exists^(CONFIG_FILE^):
echo             try:
echo                 with open^(CONFIG_FILE, "r", encoding="utf-8"^) as f:
echo                     dados = json.load^(f^)
echo             except Exception:
echo                 dados = {}
echo.
echo         # Salva nos formatos mais comuns de ROI ^(x,y,w,h e x1,y1,x2,y2 e por resolucao^)
echo         novo_bloco = {"x": x1, "y": y1, "w": w, "h": h, "x1": x1, "y1": y1, "x2": x2, "y2": y2, "left": x1, "top": y1, "width": w, "height": h}
echo         dados.update^(novo_bloco^)
echo         dados[res_str] = novo_bloco
echo         dados["zen_roi"] = novo_bloco
echo.
echo         for destino in [CONFIG_FILE, os.path.join^("local_agent", os.path.basename^(CONFIG_FILE^)^)]:
echo             try:
echo                 with open^(destino, "w", encoding="utf-8"^) as f:
echo                     json.dump^(dados, f, indent=4^)
echo             except Exception:
echo                 pass
echo.
echo         self.mostrar_config_atual^(^)
echo         messagebox.showinfo^("Sucesso", f"Área do Zen calibrada para esta máquina!\nCoordenadas relativas à janela: X={x1}, Y={y1}, L={w}, A={h}"^)
echo.
echo if __name__ == "__main__":
echo     root = tk.Tk^(^)
echo     app = CalibradorOCR^(root^)
echo     root.mainloop^(^)
) > calibrar_ocr.py

:: 3. Cria o atalho .bat para abrir o calibrador facilmente em qualquer PC
(
echo @echo off
echo chcp 65001 ^>nul
echo title Calibrador de Zen - OCR
echo cd /d "%%~dp0"
echo if exist ".venv\Scripts\activate.bat" call .venv\Scripts\activate.bat
echo python calibrar_ocr.py
) > calibrar_zen.bat

echo.
echo ===================================================
echo   CALIBRADOR CRIADO COM SUCESSO!
echo   Para ajustar em qualquer PC, clique em: calibrar_zen.bat
echo ===================================================
pause