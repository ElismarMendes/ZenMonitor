import os
import sys
import json
import tkinter as tk
from tkinter import ttk, messagebox
from PIL import ImageGrab, ImageTk
import win32gui

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "local_agent"))
try:
    from config import CONFIG_FILE
except ImportError:
    CONFIG_FILE = "ocr_config.json"

class CalibradorOCR:
    def __init__(self, root):
        self.root = root
        self.root.title("Calibrador de OCR - ZenMonitor")
        self.root.geometry("480x260")
        self.root.resizable(False, False)

        self.janelas = self.listar_janelas_mu()

        frame = ttk.Frame(root, padding=15)
        frame.pack(fill="both", expand=True)

        ttk.Label(frame, text="1. Selecione a janela do MU Online ^(deixe o inventario 'V' aberto^):", font=("Segoe UI", 9, "bold")).pack(anchor="w", pady=(0, 5))
        self.combo = ttk.Combobox(frame, values=[f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas], width=55, state="readonly")
        self.combo.pack(fill="x", pady=(0, 10))
        if self.janelas:
            self.combo.current(0)

        btn_frame = ttk.Frame(frame)
        btn_frame.pack(fill="x", pady=5)
        ttk.Button(btn_frame, text="🔄 Atualizar Lista", command=self.atualizar_lista).pack(side="left", padx=(0, 10))
        ttk.Button(btn_frame, text="🎯 Capturar e Selecionar Área do Zen", command=self.iniciar_selecao).pack(side="left")

        self.lbl_status = ttk.Label(frame, text="Coordenadas atuais: Carregando...", foreground="blue", font=("Consolas", 9))
        self.lbl_status.pack(anchor="w", pady=(15, 0))
        self.mostrar_config_atual()

    def listar_janelas_mu(self):
        res = []
        def callback(hwnd, _):
            if win32gui.IsWindowVisible(hwnd):
                titulo = win32gui.GetWindowText(hwnd)
                if titulo and ("mu" in titulo.lower() or "main" in titulo.lower()):
                    res.append((hwnd, titulo))
        win32gui.EnumWindows(callback, None)
        if not res:
            def callback_all(hwnd, _):
                if win32gui.IsWindowVisible(hwnd) and win32gui.GetWindowText(hwnd):
                    res.append((hwnd, win32gui.GetWindowText(hwnd)))
            win32gui.EnumWindows(callback_all, None)
        return res

    def atualizar_lista(self):
        self.janelas = self.listar_janelas_mu()
        self.combo["values"] = [f"{hwnd} - {titulo}" for hwnd, titulo in self.janelas]
        if self.janelas:
            self.combo.current(0)

    def mostrar_config_atual(self):
        if os.path.exists(CONFIG_FILE):
            try:
                with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                    dados = json.load(f)
                self.lbl_status.config(text=f"Salvo em {CONFIG_FILE}:\n{json.dumps(dados)}")
                return
            except Exception:
                pass
        self.lbl_status.config(text=f"Nenhum arquivo {CONFIG_FILE} encontrado ainda.")

    def iniciar_selecao(self):
        selecao = self.combo.get()
        if not selecao:
            messagebox.showwarning("Aviso", "Selecione uma janela primeiro!")
            return

        hwnd = int(selecao.split(" - ")[0])
        try:
            if win32gui.IsIconic(hwnd):
                win32gui.ShowWindow(hwnd, 9)
            win32gui.SetForegroundWindow(hwnd)
        except Exception:
            pass

        self.root.withdraw()
        self.root.after(400, lambda: self.abrir_tela_recorte(hwnd))

    def abrir_tela_recorte(self, hwnd):
        rect = win32gui.GetWindowRect(hwnd)
        x1, y1, x2, y2 = rect
        largura_jan = x2 - x1
        altura_jan = y2 - y1
        img = ImageGrab.grab(bbox=(x1, y1, x2, y2))

        top = tk.Toplevel(self.root)
        top.title("Arraste o mouse sobre os numeros do ZEN e solte")
        top.attributes("-topmost", True)

        canvas = tk.Canvas(top, width=img.width, height=img.height, cursor="cross")
        canvas.pack()

        tk_img = ImageTk.PhotoImage(img)
        canvas.img = tk_img
        canvas.create_image(0, 0, anchor="nw", image=tk_img)

        coords = {"start_x": 0, "start_y": 0, "rect": None}

        def on_press(event):
            coords["start_x"] = event.x
            coords["start_y"] = event.y
            if coords["rect"]:
                canvas.delete(coords["rect"])
            coords["rect"] = canvas.create_rectangle(event.x, event.y, event.x, event.y, outline="#00ff00", width=2)

        def on_drag(event):
            canvas.coords(coords["rect"], coords["start_x"], coords["start_y"], event.x, event.y)

        def on_release(event):
            rx1 = min(coords["start_x"], event.x)
            ry1 = min(coords["start_y"], event.y)
            rx2 = max(coords["start_x"], event.x)
            ry2 = max(coords["start_y"], event.y)
            w = rx2 - rx1
            h = ry2 - ry1

            if w < 5 or h < 5:
                messagebox.showwarning("Muito pequeno", "Selecione um retangulo maior sobre o Zen.")
                return

            resolucao_str = f"{largura_jan}x{altura_jan}"
            self.salvar_coordenadas(rx1, ry1, rx2, ry2, w, h, resolucao_str)
            top.destroy()
            self.root.deiconify()

        canvas.bind("<ButtonPress-1>", on_press)
        canvas.bind("<B1-Motion>", on_drag)
        canvas.bind("<ButtonRelease-1>", on_release)
        top.protocol("WM_DELETE_WINDOW", lambda: (top.destroy(), self.root.deiconify()))

    def salvar_coordenadas(self, x1, y1, x2, y2, w, h, res_str):
        dados = {}
        if os.path.exists(CONFIG_FILE):
            try:
                with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                    dados = json.load(f)
            except Exception:
                dados = {}

        # Salva nos formatos mais comuns de ROI (x,y,w,h e x1,y1,x2,y2 e por resolucao)
        novo_bloco = {"x": x1, "y": y1, "w": w, "h": h, "x1": x1, "y1": y1, "x2": x2, "y2": y2, "left": x1, "top": y1, "width": w, "height": h}
        dados.update(novo_bloco)
        dados[res_str] = novo_bloco
        dados["zen_roi"] = novo_bloco

        for destino in [CONFIG_FILE, os.path.join("local_agent", os.path.basename(CONFIG_FILE))]:
            try:
                with open(destino, "w", encoding="utf-8") as f:
                    json.dump(dados, f, indent=4)
            except Exception:
                pass

        self.mostrar_config_atual()
        messagebox.showinfo("Sucesso", f"Área do Zen calibrada para esta máquina!\nCoordenadas relativas à janela: X={x1}, Y={y1}, L={w}, A={h}")

if __name__ == "__main__":
    root = tk.Tk()
    app = CalibradorOCR(root)
    root.mainloop()
