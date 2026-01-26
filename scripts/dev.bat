@echo off
echo Starting Tapo Controller Development Environment...
echo.

:: Start backend in new window
start "Backend" cmd /k "cd /d %~dp0\..\backend && py -3 -m uvicorn main:app --reload --host 127.0.0.1 --port 8742"

:: Wait for backend
echo Waiting for backend...
:wait_backend
timeout /t 1 /nobreak >nul
curl -s http://localhost:8742/health >nul 2>&1
if errorlevel 1 goto wait_backend
echo Backend ready!

:: Start frontend in new window
start "Frontend" cmd /k "cd /d %~dp0\.. && npx vite"

:: Wait for frontend
echo Waiting for frontend...
:wait_frontend
timeout /t 1 /nobreak >nul
curl -s http://localhost:5173 >nul 2>&1
if errorlevel 1 goto wait_frontend
echo Frontend ready!

:: Start Electron
echo Starting Electron...
cd /d %~dp0\..
npx electron .
