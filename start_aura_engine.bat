@echo off
echo Starting Aura Validation Network - Ronin Blockchain Mesh Utility
echo.

REM Check if Rust/Cargo is available
where cargo >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Cargo not found in PATH, trying full path...
    if exist "C:\Users\jerem\.cargo\bin\cargo.exe" (
        echo Using Cargo from: C:\Users\jerem\.cargo\bin\cargo.exe
        "C:\Users\jerem\.cargo\bin\cargo.exe" run
    ) else (
        echo ERROR: Rust/Cargo not found. Please install Rust from https://rustup.rs/
        pause
        exit /b 1
    )
) else (
    echo Using Cargo from PATH
    cargo run
)

pause
