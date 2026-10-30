@echo off
chcp 65001 >nul
title Calibrador de Zen - OCR
cd /d "%~dp0"
if exist ".venv\Scripts\activate.bat" call .venv\Scripts\activate.bat
python calibrar_ocr.py
