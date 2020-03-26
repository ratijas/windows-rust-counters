@echo off
echo Make sure you are running this script with admin privileges

cd %~dp0

echo[
echo Copying dll into System32 directory
copy "..\..\..\target\debug\ExampleProvideMorseCounter.dll" "C:\Windows\System32\ExampleProvideMorseCounter.dll" || goto :EOF
echo]

echo[
echo Installing into registry
reg import ..\resources\morse.reg || goto :EOF
echo]

echo[
echo Installing counters
lodctr ..\resources\morse.ini || goto :EOF
echo]

echo[
echo Done
echo]