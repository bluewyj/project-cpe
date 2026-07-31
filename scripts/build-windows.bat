@echo off
setlocal EnableDelayedExpansion

call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 (
    echo Failed to initialize MSVC environment
    exit /b 1
)

set "PATH=C:\Tools\zig-windows-x86_64-0.13.0;%USERPROFILE%\.cargo\bin;%PATH%"
set SQLITE3_STATIC=1
set LIBSQLITE3_SYS_USE_PKG_CONFIG=0

cd /d "%~dp0..\backend"
set CARGO_TARGET_DIR=%~dp0..\backend\target
rustup default stable-x86_64-pc-windows-msvc
where cargo-zigbuild >nul 2>&1
if errorlevel 1 (
    echo Installing cargo-zigbuild...
    cargo install cargo-zigbuild --locked
    if errorlevel 1 exit /b 1
)
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.27
exit /b %ERRORLEVEL%
