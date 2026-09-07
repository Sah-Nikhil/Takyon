@echo off
rem `zig cc` fixed to aarch64-macos, with cc-rs's own --target= dropped.
rem cc-rs emits --target=arm64-apple-macosx and zig's clang frontend rejects
rem "arm64" as an architecture name. Everything else passes through untouched.
rem Driven by scripts/check-macos.ps1, which puts zig on PATH first.
setlocal enabledelayedexpansion
set "ARGS="
:loop
if "%~1"=="" goto run
set "A=%~1"
if "!A:~0,9!"=="--target=" goto next
set "ARGS=!ARGS! "%~1""
:next
shift
goto loop
:run
zig.exe cc -target aarch64-macos %ARGS%
