@echo off
setlocal
cd /d "%~dp0"
if errorlevel 1 exit /b 1

if /i "%~1"=="dev" goto dev
if /i "%~1"=="build" goto build
if /i "%~1"=="frontend" goto frontend

echo Usage: build.bat [dev^|build^|frontend]
exit /b 2

:dev
call npm run tauri:dev
exit /b %errorlevel%

:build
call "%~dp0..\..\build_tauri.bat"
exit /b %errorlevel%

:frontend
call npm run build
exit /b %errorlevel%
