@echo off
echo Initializing git repository...
git init

echo Adding remote origin...
git remote add origin https://github.com/Baku-1/AuraValidationNetwork.git

echo Adding all files...
git add .

echo Committing files...
git commit -m "Initial commit - AuraValidationNetwork project"

echo Pushing to GitHub...
git push -u origin main

echo Repository setup complete!
pause
