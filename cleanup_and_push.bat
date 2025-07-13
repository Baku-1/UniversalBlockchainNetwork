@echo off
echo Cleaning up large files...

REM Remove large files
if exist "rustup-init (1).exe" del "rustup-init (1).exe"
if exist target rmdir /s /q target
if exist data\transactions.db del data\transactions.db
if exist aura_node_identity.key del aura_node_identity.key
if exist Cargo.lock del Cargo.lock

echo Adding files to git...
git add .
git commit -m "Clean repository - remove large files and build artifacts"

echo Pushing to GitHub...
git push -u origin main

echo Done!
pause
