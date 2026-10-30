@echo off
chcp 65001 >nul
title Calibrador Visual de Zen - ZenMonitor
cd /d "C:\Mu\ZenCloud"
.venv\Scripts\python.exe agent_sender.py --calibrar
pause