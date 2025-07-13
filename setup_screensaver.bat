@echo off
echo ========================================
echo Aura Validation Network Screensaver Setup
echo ========================================
echo.

echo This script will help you set up the Aura Visualizer as a screensaver
echo for your Windows system. This allows the validation engine to run
echo during idle periods while providing a beautiful visualization.
echo.

echo IMPORTANT: You will be asked for permission to use idle system resources
echo for blockchain validation. This is completely optional and you maintain
echo full control over when validation occurs.
echo.

pause

echo.
echo Setting up Aura Visualizer...
echo.

REM Check if Chrome is installed
where chrome >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    echo Chrome detected - will use Chrome for best performance
    set BROWSER=chrome
) else (
    REM Check if Edge is installed
    where msedge >nul 2>nul
    if %ERRORLEVEL% EQU 0 (
        echo Edge detected - will use Edge
        set BROWSER=msedge
    ) else (
        echo Using default browser
        set BROWSER=start
    )
)

REM Create a batch file to launch the visualizer in fullscreen
echo Creating Aura Visualizer launcher...

set LAUNCHER_PATH=%~dp0launch_aura_visualizer.bat
set HTML_PATH=%~dp0AuraVisualizer.html

echo @echo off > "%LAUNCHER_PATH%"
echo REM Aura Validation Network Visualizer Launcher >> "%LAUNCHER_PATH%"
echo REM This launches the visualizer in fullscreen mode >> "%LAUNCHER_PATH%"
echo. >> "%LAUNCHER_PATH%"
echo REM Start the Rust engine first >> "%LAUNCHER_PATH%"
echo start /min "Aura Engine" "%~dp0target\debug\aura-validation-network.exe" >> "%LAUNCHER_PATH%"
echo. >> "%LAUNCHER_PATH%"
echo REM Wait a moment for the engine to start >> "%LAUNCHER_PATH%"
echo timeout /t 3 /nobreak ^>nul >> "%LAUNCHER_PATH%"
echo. >> "%LAUNCHER_PATH%"
echo REM Launch the mobile-first visualizer >> "%LAUNCHER_PATH%"
if "%BROWSER%"=="chrome" (
    echo start "Aura Mesh Visualizer" chrome --kiosk --app="file:///%HTML_PATH%" >> "%LAUNCHER_PATH%"
) else if "%BROWSER%"=="msedge" (
    echo start "Aura Mesh Visualizer" msedge --kiosk --app="file:///%HTML_PATH%" >> "%LAUNCHER_PATH%"
) else (
    echo start "Aura Mesh Visualizer" "%HTML_PATH%" >> "%LAUNCHER_PATH%"
)

echo.
echo Creating desktop shortcut...

REM Create a VBS script to create a shortcut
set VBS_PATH=%TEMP%\create_shortcut.vbs
echo Set oWS = WScript.CreateObject("WScript.Shell") > "%VBS_PATH%"
echo sLinkFile = "%USERPROFILE%\Desktop\Aura Validation Network.lnk" >> "%VBS_PATH%"
echo Set oLink = oWS.CreateShortcut(sLinkFile) >> "%VBS_PATH%"
echo oLink.TargetPath = "%LAUNCHER_PATH%" >> "%VBS_PATH%"
echo oLink.WorkingDirectory = "%~dp0" >> "%VBS_PATH%"
echo oLink.Description = "Aura Validation Network - Ronin Blockchain Mesh Utility" >> "%VBS_PATH%"
echo oLink.IconLocation = "%SystemRoot%\System32\shell32.dll,43" >> "%VBS_PATH%"
echo oLink.Save >> "%VBS_PATH%"

cscript //nologo "%VBS_PATH%"
del "%VBS_PATH%"

echo.
echo ========================================
echo Setup Complete!
echo ========================================
echo.
echo The following has been created:
echo 1. Launcher script: %LAUNCHER_PATH%
echo 2. Desktop shortcut: Aura Validation Network
echo.
echo HOW TO USE:
echo.
echo MANUAL MODE:
echo - Double-click the desktop shortcut to start the visualizer
echo - Press F11 to enter/exit fullscreen mode
echo - Press ESC to exit screensaver mode
echo.
echo SCREENSAVER MODE:
echo - The visualizer will automatically detect when you're idle
echo - After 5 minutes of inactivity, validation will begin
echo - Move your mouse or press any key to return to normal mode
echo.
echo CONTROLS:
echo - Toggle validation on/off with the switch in the interface
echo - Enable/disable mesh mode for offline gaming support
echo - View detailed statistics and network information
echo.
echo PRIVACY:
echo - You will be prompted for permission on first run
echo - You can revoke permission at any time
echo - No personal data is accessed or transmitted
echo - Validation only occurs during idle periods
echo.
echo For more information, see the README.md file.
echo.
pause
