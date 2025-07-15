@echo off
echo Renaming branch to main...
git branch -M main

echo Pushing to GitHub...
git push -u origin main

echo Repository successfully pushed!
pause
